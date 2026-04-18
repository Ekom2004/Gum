use std::time::{SystemTime, UNIX_EPOCH};

use gum_api::{app_state::AppState, service};
use gum_store::queries::{ControlLeaseParams, GumStore};

mod probes;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let state = AppState::for_dev().map_err(|message| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("failed to initialize scheduler state: {message}"),
        )
    })?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("failed to build tokio runtime: {error}"),
            )
        })?;

    runtime.block_on(async move {
        let tick_interval = std::time::Duration::from_secs(5);
        let leader_ttl_secs = 15;
        let leader_id = format!("scheduler-{}", std::process::id());
        let probe_client = reqwest::Client::builder().build().map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("failed to build probe client: {error}"),
            )
        })?;

        tracing::info!(leader_id = %leader_id, "gum-scheduler started");
        loop {
            let now_epoch_ms = now_epoch_ms();
            let is_leader = state
                .store
                .try_acquire_control_lease(ControlLeaseParams {
                    lease_name: "scheduler".to_string(),
                    holder_id: leader_id.clone(),
                    ttl_secs: leader_ttl_secs,
                    now_epoch_ms,
                })
                .map_err(|message| {
                    std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("control lease acquisition failed: {message}"),
                    )
                })?;
            if !is_leader {
                tokio::time::sleep(tick_interval).await;
                continue;
            }

            let recovered = state
                .store
                .recover_lost_attempts(now_epoch_ms)
                .map_err(|message| {
                    std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("lost-attempt recovery failed: {message}"),
                    )
                })?;
            let created =
                service::tick_schedules(&state.store, now_epoch_ms).map_err(|message| {
                    std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("scheduler tick failed: {message}"),
                    )
                })?;

            if !created.is_empty() {
                tracing::info!("scheduled {} run(s)", created.len());
            }
            if !recovered.is_empty() {
                tracing::info!("recovered {} lost run(s)", recovered.len());
            }

            let provider_updates =
                probes::run_provider_probes(&state.store, &probe_client, now_epoch_ms)
                    .await
                    .map_err(|message| {
                        std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("provider probe loop failed: {message}"),
                        )
                    })?;
            for update in provider_updates {
                tracing::info!(
                    provider = %update.provider_slug,
                    state = ?update.state,
                    reason = update.reason.as_deref().unwrap_or(""),
                    "provider health updated"
                );
            }

            tokio::time::sleep(tick_interval).await;
        }
    })
}

fn now_epoch_ms() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis() as i64,
        Err(_) => 0,
    }
}
