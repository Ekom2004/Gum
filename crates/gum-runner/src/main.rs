use std::time::Duration;

use gum_runner::runner_loop::{
    AppendLogRequest, CompleteAttemptRequest, LeaseRunRequest, LeaseStateResponse, LeasedRun,
    RegisterRunnerRequest, RunnerHeartbeatRequest, RunnerLoopConfig,
};
use gum_types::AttemptStatus;
use reqwest::StatusCode;
use tokio::sync::{oneshot, watch};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let config = RunnerLoopConfig {
        runner_id: "runner_dev".to_string(),
        poll_interval_ms: 1_000,
        lease_ttl_secs: 30,
        heartbeat_timeout_secs: 30,
        compute_class: std::env::var("GUM_RUNNER_COMPUTE_CLASS")
            .unwrap_or_else(|_| "standard".to_string()),
        memory_mb: env_u32("GUM_RUNNER_MEMORY_MB", 1024),
        max_concurrent_leases: env_u32("GUM_RUNNER_MAX_CONCURRENT_LEASES", 1),
        internal_key: std::env::var("GUM_INTERNAL_KEY")
            .unwrap_or_else(|_| "gum-dev-internal".to_string()),
    };
    let client = reqwest::Client::new();
    let base_url = match std::env::var("GUM_API_BASE_URL") {
        Ok(value) => value,
        Err(_) => "http://127.0.0.1:8000".to_string(),
    };

    tracing::info!("gum-runner polling {}", base_url);
    register_once(&client, &base_url, &config).await?;

    loop {
        match lease_once(&client, &base_url, &config).await {
            Ok(Some(leased)) => {
                tracing::info!(run_id = %leased.run_id, attempt_id = %leased.attempt_id, "leased run");
                let (heartbeat_stop_tx, heartbeat_stop_rx) = oneshot::channel();
                let (cancel_stop_tx, cancel_stop_rx) = oneshot::channel();
                let (cancel_tx, cancel_rx) = watch::channel(false);
                let heartbeat_task = tokio::spawn(heartbeat_loop(
                    client.clone(),
                    base_url.clone(),
                    config.clone(),
                    vec![leased.lease_id.clone()],
                    heartbeat_stop_rx,
                ));
                let cancel_task = tokio::spawn(cancel_poll_loop(
                    client.clone(),
                    base_url.clone(),
                    config.internal_key.clone(),
                    leased.lease_id.clone(),
                    cancel_tx,
                    cancel_stop_rx,
                ));
                let outcome =
                    gum_runner::execution::execute_leased_run_with_cancel(&leased, cancel_rx).await;
                let _ = heartbeat_stop_tx.send(());
                let _ = cancel_stop_tx.send(());
                let _ = heartbeat_task.await;
                let _ = cancel_task.await;
                append_logs(
                    &client,
                    &base_url,
                    &config.internal_key,
                    &leased,
                    "stdout",
                    &outcome.stdout,
                )
                .await?;
                append_logs(
                    &client,
                    &base_url,
                    &config.internal_key,
                    &leased,
                    "stderr",
                    &outcome.stderr,
                )
                .await?;
                if let Some(message) = &outcome.app_message {
                    append_log_once(
                        &client,
                        &base_url,
                        &config.internal_key,
                        &leased.run_id,
                        &leased.attempt_id,
                        "app",
                        message,
                    )
                    .await?;
                }

                complete_once(
                    &client,
                    &base_url,
                    &config,
                    &leased,
                    outcome.status,
                    outcome.failure_reason,
                    outcome.failure_class,
                )
                .await?;
            }
            Ok(None) => {
                heartbeat_once(&client, &base_url, &config, Vec::new()).await?;
                tokio::time::sleep(Duration::from_millis(config.poll_interval_ms)).await;
            }
            Err(error) => {
                tracing::error!("runner loop error: {}", error);
                let _ = heartbeat_once(&client, &base_url, &config, Vec::new()).await;
                tokio::time::sleep(Duration::from_millis(config.poll_interval_ms)).await;
            }
        }
    }
}

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

async fn register_once(
    client: &reqwest::Client,
    base_url: &str,
    config: &RunnerLoopConfig,
) -> Result<(), String> {
    let response = client
        .post(format!(
            "{}/internal/runners/register",
            base_url.trim_end_matches('/')
        ))
        .bearer_auth(&config.internal_key)
        .json(&RegisterRunnerRequest {
            runner_id: config.runner_id.clone(),
            compute_class: config.compute_class.clone(),
            memory_mb: config.memory_mb,
            max_concurrent_leases: config.max_concurrent_leases,
            heartbeat_timeout_secs: config.heartbeat_timeout_secs,
        })
        .send()
        .await
        .map_err(|error| format!("failed to register runner: {error}"))?;

    if response.status().is_success() {
        return Ok(());
    }

    let body = response
        .text()
        .await
        .map_err(|error| format!("failed to read register runner error body: {error}"))?;
    Err(format!("runner registration failed: {body}"))
}

async fn lease_once(
    client: &reqwest::Client,
    base_url: &str,
    config: &RunnerLoopConfig,
) -> Result<Option<LeasedRun>, String> {
    let response = client
        .post(format!(
            "{}/internal/runs/lease",
            base_url.trim_end_matches('/')
        ))
        .bearer_auth(&config.internal_key)
        .json(&LeaseRunRequest {
            runner_id: config.runner_id.clone(),
            lease_ttl_secs: config.lease_ttl_secs,
        })
        .send()
        .await
        .map_err(|error| format!("failed to request lease: {error}"))?;

    if response.status() == StatusCode::NO_CONTENT {
        return Ok(None);
    }

    if !response.status().is_success() {
        let body = response
            .text()
            .await
            .map_err(|error| format!("failed to read lease error body: {error}"))?;
        return Err(format!("lease request failed: {body}"));
    }

    response
        .json::<LeasedRun>()
        .await
        .map(Some)
        .map_err(|error| format!("failed to decode leased run: {error}"))
}

async fn heartbeat_loop(
    client: reqwest::Client,
    base_url: String,
    config: RunnerLoopConfig,
    active_lease_ids: Vec<String>,
    mut stop_rx: oneshot::Receiver<()>,
) {
    let interval_ms = heartbeat_interval_ms(config.lease_ttl_secs);

    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(interval_ms)) => {
                if let Err(error) = heartbeat_once(&client, &base_url, &config, active_lease_ids.clone()).await {
                    tracing::warn!(runner_id = %config.runner_id, "heartbeat failed: {}", error);
                }
            }
            _ = &mut stop_rx => {
                break;
            }
        }
    }
}

async fn cancel_poll_loop(
    client: reqwest::Client,
    base_url: String,
    internal_key: String,
    lease_id: String,
    cancel_tx: watch::Sender<bool>,
    mut stop_rx: oneshot::Receiver<()>,
) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                match lease_state_once(&client, &base_url, &internal_key, &lease_id).await {
                    Ok(Some(state)) if state.cancel_requested => {
                        let _ = cancel_tx.send(true);
                        break;
                    }
                    Ok(Some(_)) | Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(lease_id = %lease_id, "lease state poll failed: {}", error);
                    }
                }
            }
            _ = &mut stop_rx => {
                break;
            }
        }
    }
}

async fn heartbeat_once(
    client: &reqwest::Client,
    base_url: &str,
    config: &RunnerLoopConfig,
    active_lease_ids: Vec<String>,
) -> Result<(), String> {
    let response = client
        .post(format!(
            "{}/internal/runners/heartbeat",
            base_url.trim_end_matches('/')
        ))
        .bearer_auth(&config.internal_key)
        .json(&RunnerHeartbeatRequest {
            runner_id: config.runner_id.clone(),
            compute_class: config.compute_class.clone(),
            memory_mb: config.memory_mb,
            max_concurrent_leases: config.max_concurrent_leases,
            heartbeat_timeout_secs: config.heartbeat_timeout_secs,
            lease_ttl_secs: config.lease_ttl_secs,
            active_lease_ids,
        })
        .send()
        .await
        .map_err(|error| format!("failed to send runner heartbeat: {error}"))?;

    if response.status().is_success() {
        return Ok(());
    }

    let body = response
        .text()
        .await
        .map_err(|error| format!("failed to read heartbeat error body: {error}"))?;
    Err(format!("runner heartbeat failed: {body}"))
}

async fn lease_state_once(
    client: &reqwest::Client,
    base_url: &str,
    internal_key: &str,
    lease_id: &str,
) -> Result<Option<LeaseStateResponse>, String> {
    let response = client
        .get(format!(
            "{}/internal/leases/{}",
            base_url.trim_end_matches('/'),
            lease_id
        ))
        .bearer_auth(internal_key)
        .send()
        .await
        .map_err(|error| format!("failed to fetch lease state: {error}"))?;

    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }

    if !response.status().is_success() {
        let body = response
            .text()
            .await
            .map_err(|error| format!("failed to read lease state error body: {error}"))?;
        return Err(format!("lease state request failed: {body}"));
    }

    response
        .json::<LeaseStateResponse>()
        .await
        .map(Some)
        .map_err(|error| format!("failed to decode lease state: {error}"))
}

async fn append_logs(
    client: &reqwest::Client,
    base_url: &str,
    internal_key: &str,
    leased: &LeasedRun,
    stream: &str,
    output: &str,
) -> Result<(), String> {
    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }

        append_log_once(
            client,
            base_url,
            internal_key,
            &leased.run_id,
            &leased.attempt_id,
            stream,
            line,
        )
        .await?;
    }

    Ok(())
}

async fn append_log_once(
    client: &reqwest::Client,
    base_url: &str,
    internal_key: &str,
    run_id: &str,
    attempt_id: &str,
    stream: &str,
    message: &str,
) -> Result<(), String> {
    let response = client
        .post(format!(
            "{}/internal/runs/{}/attempts/{}/logs",
            base_url.trim_end_matches('/'),
            run_id,
            attempt_id
        ))
        .bearer_auth(internal_key)
        .json(&AppendLogRequest {
            stream: stream.to_string(),
            message: message.to_string(),
        })
        .send()
        .await
        .map_err(|error| format!("failed to append log: {error}"))?;

    if response.status().is_success() {
        return Ok(());
    }

    let body = response
        .text()
        .await
        .map_err(|error| format!("failed to read log append error body: {error}"))?;
    Err(format!("log append failed: {body}"))
}

async fn complete_once(
    client: &reqwest::Client,
    base_url: &str,
    config: &RunnerLoopConfig,
    leased: &LeasedRun,
    status: AttemptStatus,
    failure_reason: Option<String>,
    failure_class: Option<String>,
) -> Result<(), String> {
    let response = client
        .post(format!(
            "{}/internal/attempts/{}/complete",
            base_url.trim_end_matches('/'),
            leased.attempt_id
        ))
        .bearer_auth(&config.internal_key)
        .json(&CompleteAttemptRequest {
            runner_id: config.runner_id.clone(),
            status,
            failure_reason,
            failure_class,
        })
        .send()
        .await
        .map_err(|error| format!("failed to complete attempt: {error}"))?;

    if response.status().is_success() {
        return Ok(());
    }

    let body = response
        .text()
        .await
        .map_err(|error| format!("failed to read completion error body: {error}"))?;
    Err(format!("attempt completion failed: {body}"))
}

fn heartbeat_interval_ms(lease_ttl_secs: u64) -> u64 {
    let ttl_ms = lease_ttl_secs.saturating_mul(1_000);
    ttl_ms.saturating_div(3).max(1_000)
}
