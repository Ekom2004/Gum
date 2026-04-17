use std::time::Duration;

use gum_runner::execution::execute_leased_run;
use gum_runner::runner_loop::{
    AppendLogRequest, CompleteAttemptRequest, LeaseRunRequest, LeasedRun, RegisterRunnerRequest,
    RunnerHeartbeatRequest, RunnerLoopConfig,
};
use gum_types::AttemptStatus;
use reqwest::StatusCode;
use tokio::sync::oneshot;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let config = RunnerLoopConfig {
        runner_id: "runner_dev".to_string(),
        poll_interval_ms: 1_000,
        lease_ttl_secs: 30,
        heartbeat_timeout_secs: 30,
        compute_class: "standard".to_string(),
        max_concurrent_leases: 1,
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
                let (stop_tx, stop_rx) = oneshot::channel();
                let heartbeat_task = tokio::spawn(heartbeat_loop(
                    client.clone(),
                    base_url.clone(),
                    config.clone(),
                    vec![leased.lease_id.clone()],
                    stop_rx,
                ));
                let outcome = execute_leased_run(&leased).await;
                let _ = stop_tx.send(());
                let _ = heartbeat_task.await;
                append_logs(&client, &base_url, &leased, "stdout", &outcome.stdout).await?;
                append_logs(&client, &base_url, &leased, "stderr", &outcome.stderr).await?;
                if let Some(message) = &outcome.app_message {
                    append_log_once(
                        &client,
                        &base_url,
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
        .json(&RegisterRunnerRequest {
            runner_id: config.runner_id.clone(),
            compute_class: config.compute_class.clone(),
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
        .json(&RunnerHeartbeatRequest {
            runner_id: config.runner_id.clone(),
            compute_class: config.compute_class.clone(),
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

async fn append_logs(
    client: &reqwest::Client,
    base_url: &str,
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
) -> Result<(), String> {
    let response = client
        .post(format!(
            "{}/internal/attempts/{}/complete",
            base_url.trim_end_matches('/'),
            leased.attempt_id
        ))
        .json(&CompleteAttemptRequest {
            runner_id: config.runner_id.clone(),
            status,
            failure_reason,
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
