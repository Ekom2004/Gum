use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gum_types::AttemptStatus;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::runner_loop::LeasedRun;

pub struct ExecutionOutcome {
    pub status: AttemptStatus,
    pub failure_reason: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub app_message: Option<String>,
}

pub async fn execute_leased_run(leased: &LeasedRun) -> ExecutionOutcome {
    match execute_leased_run_inner(leased).await {
        Ok(outcome) => outcome,
        Err(message) => ExecutionOutcome {
            status: AttemptStatus::Failed,
            failure_reason: Some(message.clone()),
            stdout: String::new(),
            stderr: String::new(),
            app_message: Some(message),
        },
    }
}

async fn execute_leased_run_inner(leased: &LeasedRun) -> Result<ExecutionOutcome, String> {
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
        .arg("--attempt")
        .arg("1")
        .current_dir(&extract_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PYTHONPATH", python_path_value(&extract_dir, &sdk_path));

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

    let wait_result = timeout(Duration::from_secs(u64::from(leased.timeout_secs)), child.wait()).await;
    let timed_out = wait_result.is_err();
    let status = match wait_result {
        Ok(result) => result.map_err(|error| format!("failed waiting for python handler: {error}"))?,
        Err(_) => {
            child
                .kill()
                .await
                .map_err(|error| format!("failed to kill timed out python handler: {error}"))?;
            child
                .wait()
                .await
                .map_err(|error| format!("failed to reap timed out python handler: {error}"))?
        }
    };

    let stdout = join_output(stdout_task, "stdout").await?;
    let stderr = join_output(stderr_task, "stderr").await?;

    if timed_out {
        return Ok(ExecutionOutcome {
            status: AttemptStatus::TimedOut,
            failure_reason: Some(format!("job timed out after {}s", leased.timeout_secs)),
            stdout,
            stderr,
            app_message: None,
        });
    }

    if status.success() {
        return Ok(ExecutionOutcome {
            status: AttemptStatus::Succeeded,
            failure_reason: None,
            stdout,
            stderr,
            app_message: None,
        });
    }

    Ok(ExecutionOutcome {
        status: AttemptStatus::Failed,
        failure_reason: Some(match status.code() {
            Some(code) => format!("python handler exited with status code {code}"),
            None => "python handler exited without a status code".to_string(),
        }),
        stdout,
        stderr,
        app_message: None,
    })
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
