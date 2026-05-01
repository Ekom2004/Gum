use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{cmp::Reverse, collections::HashSet};

use base64::{engine::general_purpose, Engine as _};
use gum_types::AttemptStatus;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::watch;
use tokio::time::Instant;

use crate::runner_loop::{LeasedRun, LeasedRuntimePrepare};

pub struct ExecutionOutcome {
    pub status: AttemptStatus,
    pub failure_reason: Option<String>,
    pub failure_class: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub app_message: Option<String>,
}

const FAILURE_MARKER_PREFIX: &str = "__gum_failure__=";
const DEFAULT_PYTHON_VERSION: &str = "3.11";
const DEFAULT_DEP_INSTALL_TIMEOUT_SECS: u64 = 300;
const DEFAULT_CACHE_MAX_DIRS: usize = 40;
const DEFAULT_CACHE_MAX_AGE_SECS: u64 = 60 * 60 * 24 * 7;
const DEFAULT_BUILD_LOCK_TIMEOUT_SECS: u64 = 120;
const DEFAULT_BUILD_LOCK_STALE_SECS: u64 = 900;

struct RuntimeCacheMetrics {
    hits: AtomicU64,
    misses: AtomicU64,
    installs: AtomicU64,
    install_failures: AtomicU64,
}

impl RuntimeCacheMetrics {
    const fn new() -> Self {
        Self {
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            installs: AtomicU64::new(0),
            install_failures: AtomicU64::new(0),
        }
    }
}

static RUNTIME_CACHE_METRICS: OnceLock<RuntimeCacheMetrics> = OnceLock::new();

fn runtime_cache_metrics() -> &'static RuntimeCacheMetrics {
    RUNTIME_CACHE_METRICS.get_or_init(RuntimeCacheMetrics::new)
}

struct RuntimeBuildLock {
    path: PathBuf,
}

impl Drop for RuntimeBuildLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub async fn execute_leased_run(leased: &LeasedRun) -> ExecutionOutcome {
    let (_, cancel_rx) = watch::channel(false);
    execute_leased_run_with_cancel(leased, cancel_rx).await
}

pub async fn prepare_runtime(prepare: &LeasedRuntimePrepare) -> Result<(), String> {
    if prepare.deps_mode.as_deref().is_none() || prepare.deps_hash.as_deref().is_none() {
        return Ok(());
    }

    let bundle_path = resolve_bundle_path(&prepare.bundle_url, &prepare.deploy_id, "prepare")?;
    let extract_dir = prepare_extract_dir(&prepare.deploy_id, "prepare")?;
    extract_bundle(&bundle_path, &extract_dir).await?;
    let python_bin = python_bin();
    resolve_runtime_python_fields(
        prepare.python_version.as_deref(),
        prepare.deps_mode.as_deref(),
        prepare.deps_hash.as_deref(),
        &extract_dir,
        &python_bin,
    )
    .await
    .map(|_| ())
}

pub async fn execute_leased_run_with_cancel(
    leased: &LeasedRun,
    cancel_rx: watch::Receiver<bool>,
) -> ExecutionOutcome {
    match execute_leased_run_inner(leased, cancel_rx).await {
        Ok(outcome) => outcome,
        Err(message) => ExecutionOutcome {
            status: AttemptStatus::Failed,
            failure_reason: Some(message.clone()),
            failure_class: Some("gum_internal_error".to_string()),
            stdout: String::new(),
            stderr: String::new(),
            app_message: Some(message),
        },
    }
}

async fn execute_leased_run_inner(
    leased: &LeasedRun,
    mut cancel_rx: watch::Receiver<bool>,
) -> Result<ExecutionOutcome, String> {
    if let Some(missing_name) = leased
        .required_secret_names
        .iter()
        .find(|name| !leased.resolved_secrets.contains_key(*name))
    {
        return Ok(ExecutionOutcome {
            status: AttemptStatus::Failed,
            failure_reason: Some(format!("missing required secret: {missing_name}")),
            failure_class: Some("missing_secret".to_string()),
            stdout: String::new(),
            stderr: String::new(),
            app_message: None,
        });
    }

    let bundle_path = resolve_bundle_path(&leased.bundle_url, &leased.run_id, &leased.attempt_id)?;
    let extract_dir = prepare_extract_dir(&leased.run_id, &leased.attempt_id)?;
    extract_bundle(&bundle_path, &extract_dir).await?;

    let python_bin = python_bin();
    let sdk_path = sdk_path()?;
    let payload_json = serde_json::to_string(&leased.input)
        .map_err(|error| format!("failed to encode payload: {error}"))?;
    let runtime_python = match resolve_runtime_python(leased, &extract_dir, &python_bin).await {
        Ok(value) => value,
        Err(message) => {
            return Ok(ExecutionOutcome {
                status: AttemptStatus::Failed,
                failure_reason: Some(message),
                failure_class: Some("dependency_install_failed".to_string()),
                stdout: String::new(),
                stderr: String::new(),
                app_message: None,
            });
        }
    };

    let mut command = Command::new(runtime_python);
    command
        .arg("-m")
        .arg("gum.runtime")
        .arg("--handler")
        .arg(&leased.handler_ref)
        .arg("--payload-json")
        .arg(payload_json)
        .arg("--run-id")
        .arg(&leased.run_id)
        .arg("--attempt-id")
        .arg(&leased.attempt_id)
        .arg("--job-id")
        .arg(&leased.job_id)
        .current_dir(&extract_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PYTHONPATH", python_path_value(&extract_dir, &sdk_path));
    command.envs(
        leased
            .resolved_secrets
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    );

    if let Some(key) = &leased.key {
        command.arg("--key").arg(key);
    }
    if let Some(replay_of) = &leased.replay_of {
        command.arg("--replay-of").arg(replay_of);
    }

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to spawn python handler: {error}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "python child missing stdout pipe".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "python child missing stderr pipe".to_string())?;

    let stdout_task = tokio::spawn(read_output(stdout));
    let stderr_task = tokio::spawn(read_output(stderr));

    let deadline = Instant::now() + Duration::from_secs(u64::from(leased.timeout_secs));
    let wait_result = loop {
        tokio::select! {
            result = child.wait() => {
                let status = result
                    .map_err(|error| format!("failed waiting for python handler: {error}"))?;
                break WaitResult::Exited(status);
            }
            _ = sleep_until(deadline) => {
                kill_child(&mut child, "timed out").await?;
                break WaitResult::TimedOut;
            }
            changed = cancel_rx.changed() => {
                match changed {
                    Ok(()) => {
                        if *cancel_rx.borrow() {
                            kill_child(&mut child, "canceled").await?;
                            break WaitResult::Canceled;
                        }
                    }
                    Err(_) => {}
                }
            }
        }
    };

    let stdout = join_output(stdout_task, "stdout").await?;
    let stderr = join_output(stderr_task, "stderr").await?;
    let (stderr, failure_metadata) = extract_failure_metadata(&stderr);
    let stdout = redact_secrets(&stdout, &leased.resolved_secrets);
    let stderr = redact_secrets(&stderr, &leased.resolved_secrets);

    match wait_result {
        WaitResult::TimedOut => Ok(ExecutionOutcome {
            status: AttemptStatus::TimedOut,
            failure_reason: redact_optional_secret(
                Some(format!("job timed out after {}s", leased.timeout_secs)),
                &leased.resolved_secrets,
            ),
            failure_class: Some("job_timeout".to_string()),
            stdout,
            stderr,
            app_message: None,
        }),
        WaitResult::Canceled => Ok(ExecutionOutcome {
            status: AttemptStatus::Canceled,
            failure_reason: redact_optional_secret(
                Some("job canceled".to_string()),
                &leased.resolved_secrets,
            ),
            failure_class: None,
            stdout,
            stderr,
            app_message: None,
        }),
        WaitResult::Exited(status) if status.success() => Ok(ExecutionOutcome {
            status: AttemptStatus::Succeeded,
            failure_reason: None,
            failure_class: None,
            stdout,
            stderr,
            app_message: None,
        }),
        WaitResult::Exited(status) => Ok(ExecutionOutcome {
            status: AttemptStatus::Failed,
            failure_reason: redact_optional_secret(
                failure_metadata
                    .as_ref()
                    .and_then(|metadata| metadata.message.clone())
                    .or_else(|| {
                        Some(match status.code() {
                            Some(code) => format!("python handler exited with status code {code}"),
                            None => "python handler exited without a status code".to_string(),
                        })
                    }),
                &leased.resolved_secrets,
            ),
            failure_class: Some(
                failure_metadata
                    .map(|metadata| metadata.failure_class)
                    .unwrap_or_else(|| "user_code_error".to_string()),
            ),
            stdout,
            stderr,
            app_message: None,
        }),
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct FailureMetadata {
    failure_class: String,
    #[serde(default)]
    message: Option<String>,
}

fn extract_failure_metadata(stderr: &str) -> (String, Option<FailureMetadata>) {
    let mut lines = Vec::new();
    let mut metadata = None;

    for line in stderr.lines() {
        if let Some(payload) = line.strip_prefix(FAILURE_MARKER_PREFIX) {
            if let Ok(parsed) = serde_json::from_str::<FailureMetadata>(payload) {
                metadata = Some(parsed);
                continue;
            }
        }
        lines.push(line);
    }

    (lines.join("\n"), metadata)
}

fn redact_optional_secret(
    value: Option<String>,
    secrets: &std::collections::HashMap<String, String>,
) -> Option<String> {
    value.map(|message| redact_secrets(&message, secrets))
}

fn redact_secrets(text: &str, secrets: &std::collections::HashMap<String, String>) -> String {
    let mut redacted = text.to_string();
    let mut secret_values: Vec<String> = secrets
        .values()
        .filter_map(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect();
    secret_values.sort_by_key(|value| Reverse(value.len()));

    let mut seen = HashSet::new();
    for secret in secret_values {
        if !seen.insert(secret.clone()) {
            continue;
        }
        redacted = redacted.replace(&secret, "[REDACTED]");
    }

    redacted
}

async fn read_output<T>(mut stream: T) -> Result<String, String>
where
    T: tokio::io::AsyncRead + Unpin,
{
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| format!("failed to read process output: {error}"))?;
    String::from_utf8(bytes).map_err(|error| format!("process output was not valid utf-8: {error}"))
}

async fn join_output(
    handle: tokio::task::JoinHandle<Result<String, String>>,
    stream_name: &str,
) -> Result<String, String> {
    handle
        .await
        .map_err(|error| format!("failed to join {stream_name} reader task: {error}"))?
}

async fn kill_child(child: &mut tokio::process::Child, reason: &str) -> Result<(), String> {
    child
        .kill()
        .await
        .map_err(|error| format!("failed to kill {reason} python handler: {error}"))?;
    child
        .wait()
        .await
        .map_err(|error| format!("failed to reap {reason} python handler: {error}"))?;
    Ok(())
}

async fn sleep_until(deadline: Instant) {
    tokio::time::sleep_until(deadline).await;
}

fn resolve_bundle_path(
    bundle_url: &str,
    run_id: &str,
    attempt_id: &str,
) -> Result<PathBuf, String> {
    if let Some(path) = bundle_url.strip_prefix("file://") {
        return Ok(PathBuf::from(path));
    }
    if let Some(encoded) = bundle_url.strip_prefix("inline://") {
        return write_inline_bundle(encoded, run_id, attempt_id);
    }
    Err("runner only supports file:// or inline:// bundle urls".to_string())
}

fn write_inline_bundle(encoded: &str, run_id: &str, attempt_id: &str) -> Result<PathBuf, String> {
    let bytes = general_purpose::URL_SAFE_NO_PAD
        .decode(encoded.as_bytes())
        .or_else(|_| general_purpose::URL_SAFE.decode(encoded.as_bytes()))
        .map_err(|error| format!("failed to decode inline bundle: {error}"))?;

    let base = std::env::temp_dir().join("gum-runner").join("bundles");
    std::fs::create_dir_all(&base)
        .map_err(|error| format!("failed to create runner bundle dir: {error}"))?;
    let path = base.join(format!(
        "{run_id}-{attempt_id}-{}.tar.gz",
        timestamp_suffix()
    ));
    std::fs::write(&path, bytes)
        .map_err(|error| format!("failed to write inline bundle: {error}"))?;
    Ok(path)
}

fn prepare_extract_dir(run_id: &str, attempt_id: &str) -> Result<PathBuf, String> {
    let base = std::env::temp_dir().join("gum-runner");
    std::fs::create_dir_all(&base)
        .map_err(|error| format!("failed to create runner temp base: {error}"))?;
    let dir = base.join(format!("{run_id}-{attempt_id}-{}", timestamp_suffix()));
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("failed to create runner extract dir: {error}"))?;
    Ok(dir)
}

async fn extract_bundle(bundle_path: &Path, extract_dir: &Path) -> Result<(), String> {
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(bundle_path)
        .arg("-C")
        .arg(extract_dir)
        .status()
        .await
        .map_err(|error| format!("failed to extract bundle: {error}"))?;

    if status.success() {
        return Ok(());
    }

    Err(match status.code() {
        Some(code) => format!("tar exited with status code {code} while extracting bundle"),
        None => "tar exited without a status code while extracting bundle".to_string(),
    })
}

async fn resolve_runtime_python(
    leased: &LeasedRun,
    extract_dir: &Path,
    base_python: &str,
) -> Result<String, String> {
    resolve_runtime_python_fields(
        leased.python_version.as_deref(),
        leased.deps_mode.as_deref(),
        leased.deps_hash.as_deref(),
        extract_dir,
        base_python,
    )
    .await
}

async fn resolve_runtime_python_fields(
    python_version: Option<&str>,
    mode: Option<&str>,
    deps_hash: Option<&str>,
    extract_dir: &Path,
    base_python: &str,
) -> Result<String, String> {
    let Some(mode) = mode else {
        return Ok(base_python.to_string());
    };
    let Some(deps_hash) = deps_hash else {
        return Ok(base_python.to_string());
    };
    if deps_hash.trim().is_empty() {
        return Ok(base_python.to_string());
    }

    match mode {
        "uv_lock" | "requirements_txt" => {}
        _ => return Ok(base_python.to_string()),
    }

    let python_version = python_version.unwrap_or(DEFAULT_PYTHON_VERSION);
    let cache_root = std::env::var("GUM_RUNNER_VENV_CACHE_DIR")
        .unwrap_or_else(|_| "/tmp/gum-runner/venvs".to_string());
    let cache_root = PathBuf::from(cache_root);
    std::fs::create_dir_all(&cache_root)
        .map_err(|error| format!("failed to create runtime cache root: {error}"))?;
    let venv_dir = Path::new(&cache_root).join(format!(
        "{}-{}-{}",
        sanitize_runtime_component(python_version),
        sanitize_runtime_component(mode),
        sanitize_runtime_component(deps_hash),
    ));
    let lock_path = cache_root.join(format!(
        "{}-{}-{}.lock",
        sanitize_runtime_component(python_version),
        sanitize_runtime_component(mode),
        sanitize_runtime_component(deps_hash),
    ));
    let venv_python = venv_dir.join("bin").join("python");
    if venv_python.exists() {
        touch_last_used(&venv_dir);
        let hit_total = runtime_cache_metrics().hits.fetch_add(1, Ordering::Relaxed) + 1;
        tracing::info!(
            cache_hit_total = hit_total,
            venv_dir = %venv_dir.display(),
            "runtime dependency cache hit"
        );
        let _ = maybe_gc_runtime_cache(&cache_root);
        return Ok(venv_python.display().to_string());
    }

    let _build_lock = acquire_runtime_build_lock(&lock_path).await?;
    if venv_python.exists() {
        touch_last_used(&venv_dir);
        let hit_total = runtime_cache_metrics().hits.fetch_add(1, Ordering::Relaxed) + 1;
        tracing::info!(
            cache_hit_total = hit_total,
            venv_dir = %venv_dir.display(),
            "runtime dependency cache hit after lock wait"
        );
        let _ = maybe_gc_runtime_cache(&cache_root);
        return Ok(venv_python.display().to_string());
    }

    let miss_total = runtime_cache_metrics()
        .misses
        .fetch_add(1, Ordering::Relaxed)
        + 1;
    tracing::info!(
        cache_miss_total = miss_total,
        mode = mode,
        deps_hash = deps_hash,
        "runtime dependency cache miss; building venv"
    );
    let install_started = Instant::now();
    let install_result: Result<(), String> = async {
        std::fs::create_dir_all(&venv_dir)
            .map_err(|error| format!("failed to create runtime venv dir: {error}"))?;
        create_venv(base_python, &venv_dir).await?;
        if mode == "uv_lock" {
            install_uv_locked_dependencies(extract_dir, &venv_dir).await?;
        } else {
            install_requirements_dependencies(&venv_dir, extract_dir).await?;
        }
        Ok(())
    }
    .await;
    let install_ms = install_started.elapsed().as_millis() as u64;
    if let Err(error) = install_result {
        let failures = runtime_cache_metrics()
            .install_failures
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        tracing::error!(
            install_failure_total = failures,
            install_ms = install_ms,
            venv_dir = %venv_dir.display(),
            "runtime dependency install failed"
        );
        let _ = std::fs::remove_dir_all(&venv_dir);
        return Err(error);
    }
    touch_last_used(&venv_dir);
    let installs = runtime_cache_metrics()
        .installs
        .fetch_add(1, Ordering::Relaxed)
        + 1;
    tracing::info!(
        install_total = installs,
        install_ms = install_ms,
        venv_dir = %venv_dir.display(),
        "runtime dependency install completed"
    );

    if !venv_python.exists() {
        return Err("runtime venv python binary missing after setup".to_string());
    }
    let _ = maybe_gc_runtime_cache(&cache_root);
    Ok(venv_python.display().to_string())
}

async fn acquire_runtime_build_lock(lock_path: &Path) -> Result<RuntimeBuildLock, String> {
    let started = Instant::now();
    let timeout_secs = env_u64(
        "GUM_RUNTIME_BUILD_LOCK_TIMEOUT_SECS",
        DEFAULT_BUILD_LOCK_TIMEOUT_SECS,
    );
    let stale_secs = env_u64(
        "GUM_RUNTIME_BUILD_LOCK_STALE_SECS",
        DEFAULT_BUILD_LOCK_STALE_SECS,
    );
    loop {
        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(lock_path)
        {
            Ok(mut file) => {
                let _ = writeln!(file, "pid={}", std::process::id());
                let _ = writeln!(file, "created_ms={}", timestamp_suffix());
                return Ok(RuntimeBuildLock {
                    path: lock_path.to_path_buf(),
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if is_stale_lock(lock_path, stale_secs) {
                    let _ = std::fs::remove_file(lock_path);
                }
                if started.elapsed() >= Duration::from_secs(timeout_secs) {
                    return Err(format!(
                        "timed out waiting for runtime build lock after {timeout_secs}s"
                    ));
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) => {
                return Err(format!("failed to acquire runtime build lock: {error}"));
            }
        }
    }
}

fn is_stale_lock(lock_path: &Path, stale_secs: u64) -> bool {
    let Ok(metadata) = std::fs::metadata(lock_path) else {
        return false;
    };
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    match modified.elapsed() {
        Ok(elapsed) => elapsed > Duration::from_secs(stale_secs),
        Err(_) => false,
    }
}

fn touch_last_used(venv_dir: &Path) {
    let marker = venv_dir.join(".gum_last_used");
    let _ = std::fs::write(marker, timestamp_suffix().to_string());
}

fn maybe_gc_runtime_cache(cache_root: &Path) -> Result<(), String> {
    if std::env::var("GUM_RUNTIME_CACHE_GC")
        .ok()
        .map(|value| value == "0" || value.eq_ignore_ascii_case("false"))
        .unwrap_or(false)
    {
        return Ok(());
    }
    let max_dirs = env_usize("GUM_RUNTIME_CACHE_MAX_DIRS", DEFAULT_CACHE_MAX_DIRS);
    let max_age_secs = env_u64("GUM_RUNTIME_CACHE_MAX_AGE_SECS", DEFAULT_CACHE_MAX_AGE_SECS);
    let now = SystemTime::now();

    let mut entries = Vec::new();
    for entry in std::fs::read_dir(cache_root)
        .map_err(|error| format!("failed to read runtime cache root: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("failed to read runtime cache entry: {error}"))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let marker = path.join(".gum_last_used");
        let timestamp = std::fs::metadata(&marker)
            .and_then(|metadata| metadata.modified())
            .or_else(|_| std::fs::metadata(&path).and_then(|metadata| metadata.modified()))
            .unwrap_or(now);
        entries.push((path, timestamp));
    }

    for (path, timestamp) in &entries {
        if now
            .duration_since(*timestamp)
            .unwrap_or(Duration::from_secs(0))
            > Duration::from_secs(max_age_secs)
        {
            let _ = std::fs::remove_dir_all(path);
        }
    }

    let mut remaining = Vec::new();
    for entry in std::fs::read_dir(cache_root)
        .map_err(|error| format!("failed to read runtime cache root post-age-gc: {error}"))?
    {
        let entry = entry
            .map_err(|error| format!("failed to read runtime cache entry post-age-gc: {error}"))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let marker = path.join(".gum_last_used");
        let timestamp = std::fs::metadata(&marker)
            .and_then(|metadata| metadata.modified())
            .or_else(|_| std::fs::metadata(&path).and_then(|metadata| metadata.modified()))
            .unwrap_or(now);
        remaining.push((path, timestamp));
    }
    if remaining.len() <= max_dirs {
        return Ok(());
    }
    remaining.sort_by_key(|(_, timestamp)| *timestamp);
    let to_remove = remaining.len() - max_dirs;
    for (path, _) in remaining.into_iter().take(to_remove) {
        let _ = std::fs::remove_dir_all(path);
    }
    Ok(())
}

async fn create_venv(base_python: &str, venv_dir: &Path) -> Result<(), String> {
    let timeout_secs = dependency_install_timeout_secs();
    let status = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        Command::new(base_python)
            .arg("-m")
            .arg("venv")
            .arg(venv_dir)
            .status(),
    )
    .await
    .map_err(|_| format!("python venv creation timed out after {timeout_secs}s"))?
    .map_err(|error| format!("failed to start python venv creation: {error}"))?;
    if status.success() {
        return Ok(());
    }
    Err(match status.code() {
        Some(code) => format!("python venv creation failed with status code {code}"),
        None => "python venv creation failed without a status code".to_string(),
    })
}

async fn install_uv_locked_dependencies(extract_dir: &Path, venv_dir: &Path) -> Result<(), String> {
    let lock_path = extract_dir.join("uv.lock");
    if !lock_path.exists() {
        return Err("deps_mode=uv_lock but uv.lock is missing from bundle".to_string());
    }
    let timeout_secs = dependency_install_timeout_secs();
    let status = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        Command::new("uv")
            .arg("sync")
            .arg("--frozen")
            .arg("--no-dev")
            .current_dir(extract_dir)
            .env("UV_PROJECT_ENVIRONMENT", venv_dir)
            .status(),
    )
    .await
    .map_err(|_| format!("uv sync timed out after {timeout_secs}s"))?
    .map_err(|error| format!("failed to start dependency install via uv sync: {error}"))?;
    if status.success() {
        return Ok(());
    }
    Err(match status.code() {
        Some(code) => format!("uv sync failed with status code {code}"),
        None => "uv sync failed without a status code".to_string(),
    })
}

async fn install_requirements_dependencies(
    venv_dir: &Path,
    extract_dir: &Path,
) -> Result<(), String> {
    let requirements = extract_dir.join("requirements.txt");
    if !requirements.exists() {
        return Err(
            "deps_mode=requirements_txt but requirements.txt is missing from bundle".to_string(),
        );
    }
    let venv_python = venv_dir.join("bin").join("python");
    let timeout_secs = dependency_install_timeout_secs();
    let status = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        Command::new(&venv_python)
            .arg("-m")
            .arg("pip")
            .arg("install")
            .arg("-r")
            .arg(&requirements)
            .status(),
    )
    .await
    .map_err(|_| format!("pip install -r requirements.txt timed out after {timeout_secs}s"))?
    .map_err(|error| format!("failed to start pip dependency install: {error}"))?;
    if status.success() {
        return Ok(());
    }
    Err(match status.code() {
        Some(code) => format!("pip install -r requirements.txt failed with status code {code}"),
        None => "pip install -r requirements.txt failed without a status code".to_string(),
    })
}

fn sanitize_runtime_component(raw: &str) -> String {
    raw.chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => ch,
            _ => '_',
        })
        .collect()
}

fn dependency_install_timeout_secs() -> u64 {
    env_u64(
        "GUM_DEP_INSTALL_TIMEOUT_SECS",
        DEFAULT_DEP_INSTALL_TIMEOUT_SECS,
    )
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn sdk_path() -> Result<PathBuf, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk");
    if path.exists() {
        return Ok(path);
    }
    Err(format!("sdk path does not exist: {}", path.display()))
}

fn python_path_value(extract_dir: &Path, sdk_path: &Path) -> String {
    format!("{}:{}", extract_dir.display(), sdk_path.display())
}

fn timestamp_suffix() -> u128 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(_) => 0,
    }
}

fn python_bin() -> String {
    if let Ok(value) = std::env::var("GUM_PYTHON_BIN") {
        return value;
    }

    let preferred = Path::new("/opt/homebrew/bin/python3.11");
    if preferred.exists() {
        return preferred.display().to_string();
    }

    "python3".to_string()
}

enum WaitResult {
    Exited(std::process::ExitStatus),
    TimedOut,
    Canceled,
}

#[cfg(test)]
mod tests {
    use super::{
        acquire_runtime_build_lock, maybe_gc_runtime_cache, redact_secrets, RuntimeBuildLock,
    };
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let suffix = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_millis(),
            Err(_) => 0,
        };
        let dir = std::env::temp_dir().join(format!("{prefix}-{suffix}"));
        std::fs::create_dir_all(&dir).expect("temp dir should be created");
        dir
    }

    #[test]
    fn redact_secrets_hides_all_resolved_values() {
        let mut secrets = HashMap::new();
        secrets.insert("RESEND_API_KEY".to_string(), "re_12345".to_string());
        secrets.insert("OPENAI_API_KEY".to_string(), "sk-live-abcdef".to_string());
        secrets.insert("EMPTY".to_string(), "".to_string());
        let text = "keys: re_12345 and sk-live-abcdef";

        let redacted = redact_secrets(text, &secrets);
        assert_eq!(redacted, "keys: [REDACTED] and [REDACTED]");
    }

    #[tokio::test]
    async fn build_lock_is_mutually_exclusive() {
        let dir = unique_temp_dir("gum-lock-test");
        let lock_path = dir.join("runtime.lock");
        let lock_a: RuntimeBuildLock = acquire_runtime_build_lock(&lock_path)
            .await
            .expect("first lock acquisition should work");

        let path_clone = lock_path.clone();
        let waiter = tokio::spawn(async move {
            acquire_runtime_build_lock(&path_clone)
                .await
                .expect("second lock acquisition should eventually work")
        });

        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(
            !waiter.is_finished(),
            "second lock should not be acquired while first holder is active"
        );
        drop(lock_a);

        let lock_b = waiter.await.expect("waiter should join");
        drop(lock_b);
        assert!(
            !lock_path.exists(),
            "lock file should be removed when holders are dropped"
        );
    }

    #[test]
    fn cache_gc_enforces_max_dirs() {
        let dir = unique_temp_dir("gum-cache-gc-test");
        std::env::set_var("GUM_RUNTIME_CACHE_MAX_DIRS", "2");
        std::env::set_var("GUM_RUNTIME_CACHE_MAX_AGE_SECS", "3600");
        for index in 0..4 {
            let candidate = dir.join(format!("venv-{index}"));
            std::fs::create_dir_all(&candidate).expect("cache dir should be created");
            std::fs::write(candidate.join(".gum_last_used"), format!("{index}"))
                .expect("marker should be written");
            std::thread::sleep(Duration::from_millis(5));
        }

        maybe_gc_runtime_cache(&dir).expect("gc should succeed");
        let count = std::fs::read_dir(&dir)
            .expect("cache root should be readable")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .count();
        assert!(count <= 2, "gc should cap cache dir count");
    }
}
