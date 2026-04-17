use std::time::Duration;

use gum_runner::execution::execute_leased_run;
use gum_runner::runner_loop::{
    AppendLogRequest, CompleteAttemptRequest, LeaseRunRequest, LeasedRun, RunnerLoopConfig,
};
use gum_types::AttemptStatus;
use reqwest::StatusCode;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let config = RunnerLoopConfig {
        runner_id: "runner_dev".to_string(),
        poll_interval_ms: 1_000,
        lease_ttl_secs: 30,
        };
    let client = reqwest::Client::new();
    let base_url = match std::env::var("GUM_API_BASE_URL") {
        Ok(value) => value,
        Err(_) => "http://127.0.0.1:8000".to_string(),
    };

    tracing::info!("gum-runner polling {}", base_url);

    loop {
        match lease_once(&client, &base_url, &config).await {
            Ok(Some(leased)) => {
                tracing::info!(run_id = %leased.run_id, attempt_id = %leased.attempt_id, "leased run");
                let outcome = execute_leased_run(&leased).await;
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
                tokio::time::sleep(Duration::from_millis(config.poll_interval_ms)).await;
            }
            Err(error) => {
                tracing::error!("runner loop error: {}", error);
                tokio::time::sleep(Duration::from_millis(config.poll_interval_ms)).await;
            }
        }
    }
}

async fn lease_once(
    client: &reqwest::Client,
    base_url: &str,
    config: &RunnerLoopConfig,
) -> Result<Option<LeasedRun>, String> {
    let response = client
        .post(format!("{}/internal/runs/lease", base_url.trim_end_matches('/')))
        .json(&LeaseRunRequest {
            runner_id: config.runner_id.clone(),
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

        append_log_once(client, base_url, &leased.run_id, &leased.attempt_id, stream, line).await?;
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
