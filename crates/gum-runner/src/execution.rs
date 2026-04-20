use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gum_types::AttemptStatus;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::watch;
use tokio::time::Instant;

use crate::runner_loop::LeasedRun;

pub struct ExecutionOutcome {
    pub status: AttemptStatus,
    pub failure_reason: Option<String>,
    pub failure_class: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub app_message: Option<String>,
}

const FAILURE_MARKER_PREFIX: &str = "__gum_failure__=";

pub async fn execute_leased_run(leased: &LeasedRun) -> ExecutionOutcome {
    let (_, cancel_rx) = watch::channel(false);
    execute_leased_run_with_cancel(leased, cancel_rx).await
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
    let bundle_path = file_bundle_path(&leased.bundle_url)?;
    let extract_dir = prepare_extract_dir(&leased.run_id, &leased.attempt_id)?;
    extract_bundle(&bundle_path, &extract_dir).await?;

    let python_bin = python_bin();
    let sdk_path = sdk_path()?;
    let payload_json = serde_json::to_string(&leased.input)
        .map_err(|error| format!("failed to encode payload: {error}"))?;

    let mut command = Command::new(python_bin);
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

    match wait_result {
        WaitResult::TimedOut => Ok(ExecutionOutcome {
            status: AttemptStatus::TimedOut,
            failure_reason: Some(format!("job timed out after {}s", leased.timeout_secs)),
            failure_class: Some("job_timeout".to_string()),
            stdout,
            stderr,
            app_message: None,
        }),
        WaitResult::Canceled => Ok(ExecutionOutcome {
            status: AttemptStatus::Canceled,
            failure_reason: Some("job canceled".to_string()),
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
            failure_reason: failure_metadata
                .as_ref()
                .and_then(|metadata| metadata.message.clone())
                .or_else(|| {
                    Some(match status.code() {
                        Some(code) => format!("python handler exited with status code {code}"),
                        None => "python handler exited without a status code".to_string(),
                    })
                }),
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

fn file_bundle_path(bundle_url: &str) -> Result<PathBuf, String> {
    let Some(path) = bundle_url.strip_prefix("file://") else {
        return Err("runner only supports file:// bundle urls in local mode".to_string());
    };
    Ok(PathBuf::from(path))
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
