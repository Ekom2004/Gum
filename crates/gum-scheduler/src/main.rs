use std::time::{SystemTime, UNIX_EPOCH};

use gum_api::{app_state::AppState, service};

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

        tracing::info!("gum-scheduler started");
        loop {
            let now_epoch_ms = now_epoch_ms();
            let created = service::tick_schedules(&state.store, now_epoch_ms).map_err(|message| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("scheduler tick failed: {message}"),
                )
            })?;

            if !created.is_empty() {
                tracing::info!("scheduled {} run(s)", created.len());
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
