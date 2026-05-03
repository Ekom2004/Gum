use std::collections::HashMap;
use std::time::Duration;

use gum_store::models::{
    ProviderCheckStatus, ProviderHealthRecord, ProviderHealthState, ProviderTargetRecord,
};
use gum_store::queries::{GumStore, RecordProviderCheckParams, SetProviderHealthParams};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};
use serde::Deserialize;
use serde_json::Value;

const DEFAULT_TIMEOUT_SECS: u64 = 5;
const DEFAULT_INTERVAL_SECS: u64 = 30;
const DEFAULT_DEGRADED_LATENCY_MS: u32 = 1_500;
const DOWN_SCORE_THRESHOLD: i32 = 3;
const MAX_SCORE: i32 = 10;

#[derive(Debug, Clone, Deserialize)]
struct HttpProbeConfig {
    url: String,
    #[serde(default)]
    interval_secs: Option<u64>,
    #[serde(default)]
    timeout_secs: Option<u64>,
    #[serde(default)]
    degraded_latency_ms: Option<u32>,
    #[serde(default)]
    bearer_env: Option<String>,
    #[serde(default)]
    headers: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct LoadedProbeConfig {
    url: String,
    interval_secs: u64,
    timeout_secs: u64,
    degraded_latency_ms: u32,
    headers: HeaderMap,
}

#[derive(Debug, Clone)]
struct ProbeResult {
    status: ProviderCheckStatus,
    latency_ms: Option<u32>,
    error_class: Option<String>,
    status_code: Option<u16>,
}

pub fn run_provider_probes<S: GumStore>(
    store: &S,
    client: &reqwest::blocking::Client,
    now_epoch_ms: i64,
) -> Result<Vec<ProviderHealthRecord>, String> {
    let targets = store.list_provider_targets()?;
    let health_by_target = store
        .list_provider_health()?
        .into_iter()
        .map(|record| (record.provider_target_id.clone(), record))
        .collect::<HashMap<_, _>>();
    let mut updated = Vec::new();

    for target in targets {
        if !target.enabled {
            continue;
        }

        let previous = health_by_target.get(&target.id);
        let (probe_result, degraded_latency_ms, config_error) =
            match load_probe_config(&target.probe_kind, &target.probe_config_json) {
                Ok(config) => {
                    if !probe_due(previous, config.interval_secs, now_epoch_ms) {
                        continue;
                    }
                    let degraded_latency_ms = config.degraded_latency_ms;
                    (execute_http_probe(client, &config), degraded_latency_ms, None)
                }
                Err(message) => (
                    ProbeResult {
                        status: ProviderCheckStatus::Failure,
                        latency_ms: None,
                        error_class: Some("provider_probe_config_error".to_string()),
                        status_code: None,
                    },
                    DEFAULT_DEGRADED_LATENCY_MS,
                    Some(message),
                ),
            };
        if let Some(message) = config_error.as_deref() {
            tracing::warn!(provider = %target.slug, error = %message, "provider probe configuration is invalid");
        }

        store.record_provider_check(RecordProviderCheckParams {
            provider_target_id: target.id.clone(),
            status: probe_result.status,
            latency_ms: probe_result.latency_ms,
            error_class: probe_result.error_class.clone(),
            status_code: probe_result.status_code,
            checked_at_epoch_ms: now_epoch_ms,
        })?;

        let health_params = next_health_params(
            &target,
            previous,
            &probe_result,
            now_epoch_ms,
            degraded_latency_ms,
        );
        let record = store.set_provider_health(health_params)?;
        updated.push(record);
    }

    Ok(updated)
}

fn load_probe_config(probe_kind: &str, config: &Value) -> Result<LoadedProbeConfig, String> {
    match probe_kind {
        "http" => load_http_probe_config(config),
        other => Err(format!("unsupported probe kind: {other}")),
    }
}

fn load_http_probe_config(config: &Value) -> Result<LoadedProbeConfig, String> {
    let parsed: HttpProbeConfig = serde_json::from_value(config.clone())
        .map_err(|error| format!("invalid probe config: {error}"))?;
    if parsed.url.trim().is_empty() {
        return Err("probe config url must not be empty".to_string());
    }

    let mut headers = HeaderMap::new();
    for (name, value) in parsed.headers {
        let header_name = HeaderName::try_from(name.as_str())
            .map_err(|error| format!("invalid probe header name {name}: {error}"))?;
        let header_value = HeaderValue::try_from(value.as_str())
            .map_err(|error| format!("invalid probe header value for {name}: {error}"))?;
        headers.insert(header_name, header_value);
    }

    if let Some(env_name) = parsed.bearer_env {
        let token = std::env::var(&env_name)
            .map_err(|_| format!("missing bearer token env var: {env_name}"))?;
        let bearer_value = HeaderValue::try_from(format!("Bearer {token}"))
            .map_err(|error| format!("invalid bearer token for {env_name}: {error}"))?;
        headers.insert(AUTHORIZATION, bearer_value);
    }

    Ok(LoadedProbeConfig {
        url: parsed.url,
        interval_secs: parsed.interval_secs.unwrap_or(DEFAULT_INTERVAL_SECS),
        timeout_secs: parsed.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS),
        degraded_latency_ms: parsed
            .degraded_latency_ms
            .unwrap_or(DEFAULT_DEGRADED_LATENCY_MS),
        headers,
    })
}

fn probe_due(
    previous: Option<&ProviderHealthRecord>,
    interval_secs: u64,
    now_epoch_ms: i64,
) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    let last_probe_epoch_ms = previous
        .last_success_at_epoch_ms
        .into_iter()
        .chain(previous.last_failure_at_epoch_ms)
        .max()
        .unwrap_or(previous.last_changed_at_epoch_ms);
    now_epoch_ms.saturating_sub(last_probe_epoch_ms) >= (interval_secs as i64) * 1000
}

fn execute_http_probe(client: &reqwest::blocking::Client, config: &LoadedProbeConfig) -> ProbeResult {
    let started_at = std::time::Instant::now();
    let response = client
        .get(&config.url)
        .headers(config.headers.clone())
        .timeout(Duration::from_secs(config.timeout_secs))
        .send();
    let elapsed_ms = elapsed_ms(started_at.elapsed());

    match response {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                ProbeResult {
                    status: ProviderCheckStatus::Success,
                    latency_ms: Some(elapsed_ms),
                    error_class: None,
                    status_code: Some(status.as_u16()),
                }
            } else {
                ProbeResult {
                    status: ProviderCheckStatus::Failure,
                    latency_ms: Some(elapsed_ms),
                    error_class: Some(classify_http_status(status)),
                    status_code: Some(status.as_u16()),
                }
            }
        }
        Err(error) => ProbeResult {
            status: ProviderCheckStatus::Failure,
            latency_ms: Some(elapsed_ms),
            error_class: Some(classify_transport_error(&error)),
            status_code: None,
        },
    }
}

fn next_health_params(
    target: &ProviderTargetRecord,
    previous: Option<&ProviderHealthRecord>,
    probe_result: &ProbeResult,
    now_epoch_ms: i64,
    degraded_latency_ms: u32,
) -> SetProviderHealthParams {
    let previous_state = previous
        .map(|record| record.state)
        .unwrap_or(ProviderHealthState::Healthy);
    let previous_changed_at = previous
        .map(|record| record.last_changed_at_epoch_ms)
        .unwrap_or(now_epoch_ms);
    let previous_success = previous.and_then(|record| record.last_success_at_epoch_ms);
    let previous_failure = previous.and_then(|record| record.last_failure_at_epoch_ms);
    let previous_degraded_score = previous.map(|record| record.degraded_score).unwrap_or(0);
    let previous_down_score = previous.map(|record| record.down_score).unwrap_or(0);

    let (
        state,
        reason,
        last_success_at_epoch_ms,
        last_failure_at_epoch_ms,
        degraded_score,
        down_score,
    ) = match probe_result.status {
        ProviderCheckStatus::Success => {
            let latency_ms = probe_result.latency_ms.unwrap_or_default();
            if latency_ms > degraded_latency_ms {
                (
                    ProviderHealthState::Degraded,
                    Some(format!(
                        "{} probe latency {}ms exceeded {}ms",
                        target.slug, latency_ms, degraded_latency_ms
                    )),
                    Some(now_epoch_ms),
                    previous_failure,
                    saturating_score(previous_degraded_score + 1),
                    0,
                )
            } else {
                (
                    ProviderHealthState::Healthy,
                    None,
                    Some(now_epoch_ms),
                    previous_failure,
                    previous_degraded_score.saturating_sub(2),
                    0,
                )
            }
        }
        ProviderCheckStatus::Failure => {
            let down_score = saturating_score(previous_down_score + 1);
            let state = if down_score >= DOWN_SCORE_THRESHOLD {
                ProviderHealthState::Down
            } else {
                ProviderHealthState::Degraded
            };
            (
                state,
                Some(probe_failure_reason(target, probe_result)),
                previous_success,
                Some(now_epoch_ms),
                saturating_score(previous_degraded_score + 1),
                down_score,
            )
        }
    };

    let last_changed_at_epoch_ms = if state == previous_state {
        previous_changed_at
    } else {
        now_epoch_ms
    };

    SetProviderHealthParams {
        provider_target_id: target.id.clone(),
        state,
        reason,
        last_changed_at_epoch_ms,
        last_success_at_epoch_ms,
        last_failure_at_epoch_ms,
        degraded_score,
        down_score,
    }
}

fn probe_failure_reason(target: &ProviderTargetRecord, probe_result: &ProbeResult) -> String {
    match (&probe_result.error_class, probe_result.status_code) {
        (Some(error_class), Some(status_code)) => {
            format!(
                "{} probe failed: {} ({})",
                target.slug, error_class, status_code
            )
        }
        (Some(error_class), None) => format!("{} probe failed: {}", target.slug, error_class),
        (None, Some(status_code)) => format!("{} probe failed: {}", target.slug, status_code),
        (None, None) => format!("{} probe failed", target.slug),
    }
}

fn classify_http_status(status: reqwest::StatusCode) -> String {
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return "provider_429".to_string();
    }
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return "provider_auth_error".to_string();
    }
    if status.is_server_error() {
        return "provider_5xx".to_string();
    }
    "provider_http_error".to_string()
}

fn classify_transport_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        return "provider_timeout".to_string();
    }
    if error.is_connect() {
        return "provider_connect_error".to_string();
    }
    "provider_probe_error".to_string()
}

fn elapsed_ms(duration: Duration) -> u32 {
    duration.as_millis().min(u128::from(u32::MAX)) as u32
}

fn saturating_score(score: i32) -> i32 {
    score.clamp(0, MAX_SCORE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gum_store::models::{ProviderHealthRecord, ProviderHealthState, ProviderTargetRecord};
    use serde_json::json;

    fn target() -> ProviderTargetRecord {
        ProviderTargetRecord {
            id: "provider_openai".to_string(),
            name: "OpenAI".to_string(),
            slug: "openai".to_string(),
            probe_kind: "http".to_string(),
            probe_config_json: json!({
                "url": "https://example.com/health"
            }),
            enabled: true,
            created_at_epoch_ms: 1,
        }
    }

    fn previous_health(state: ProviderHealthState) -> ProviderHealthRecord {
        ProviderHealthRecord {
            provider_target_id: "provider_openai".to_string(),
            provider_name: "OpenAI".to_string(),
            provider_slug: "openai".to_string(),
            state,
            reason: None,
            last_changed_at_epoch_ms: 100,
            last_success_at_epoch_ms: Some(100),
            last_failure_at_epoch_ms: None,
            degraded_score: 0,
            down_score: 0,
        }
    }

    #[test]
    fn failed_probes_escalate_to_down() {
        let target = target();
        let first = next_health_params(
            &target,
            None,
            &ProbeResult {
                status: ProviderCheckStatus::Failure,
                latency_ms: Some(900),
                error_class: Some("provider_timeout".to_string()),
                status_code: None,
            },
            1_000,
            DEFAULT_DEGRADED_LATENCY_MS,
        );
        assert_eq!(first.state, ProviderHealthState::Degraded);
        assert_eq!(first.down_score, 1);

        let second_previous = ProviderHealthRecord {
            provider_target_id: target.id.clone(),
            provider_name: target.name.clone(),
            provider_slug: target.slug.clone(),
            state: first.state,
            reason: first.reason.clone(),
            last_changed_at_epoch_ms: first.last_changed_at_epoch_ms,
            last_success_at_epoch_ms: first.last_success_at_epoch_ms,
            last_failure_at_epoch_ms: first.last_failure_at_epoch_ms,
            degraded_score: first.degraded_score,
            down_score: first.down_score,
        };
        let second = next_health_params(
            &target,
            Some(&second_previous),
            &ProbeResult {
                status: ProviderCheckStatus::Failure,
                latency_ms: Some(950),
                error_class: Some("provider_timeout".to_string()),
                status_code: None,
            },
            2_000,
            DEFAULT_DEGRADED_LATENCY_MS,
        );
        assert_eq!(second.state, ProviderHealthState::Degraded);
        assert_eq!(second.down_score, 2);

        let third_previous = ProviderHealthRecord {
            provider_target_id: target.id.clone(),
            provider_name: target.name.clone(),
            provider_slug: target.slug.clone(),
            state: second.state,
            reason: second.reason.clone(),
            last_changed_at_epoch_ms: second.last_changed_at_epoch_ms,
            last_success_at_epoch_ms: second.last_success_at_epoch_ms,
            last_failure_at_epoch_ms: second.last_failure_at_epoch_ms,
            degraded_score: second.degraded_score,
            down_score: second.down_score,
        };
        let third = next_health_params(
            &target,
            Some(&third_previous),
            &ProbeResult {
                status: ProviderCheckStatus::Failure,
                latency_ms: Some(980),
                error_class: Some("provider_timeout".to_string()),
                status_code: None,
            },
            3_000,
            DEFAULT_DEGRADED_LATENCY_MS,
        );
        assert_eq!(third.state, ProviderHealthState::Down);
        assert_eq!(third.down_score, 3);
    }

    #[test]
    fn healthy_probe_recovers_from_degraded_state() {
        let target = target();
        let previous = ProviderHealthRecord {
            provider_target_id: target.id.clone(),
            provider_name: target.name.clone(),
            provider_slug: target.slug.clone(),
            state: ProviderHealthState::Degraded,
            reason: Some("openai probe failed: provider_timeout".to_string()),
            last_changed_at_epoch_ms: 100,
            last_success_at_epoch_ms: Some(50),
            last_failure_at_epoch_ms: Some(100),
            degraded_score: 2,
            down_score: 2,
        };
        let next = next_health_params(
            &target,
            Some(&previous),
            &ProbeResult {
                status: ProviderCheckStatus::Success,
                latency_ms: Some(250),
                error_class: None,
                status_code: Some(200),
            },
            2_000,
            DEFAULT_DEGRADED_LATENCY_MS,
        );
        assert_eq!(next.state, ProviderHealthState::Healthy);
        assert_eq!(next.down_score, 0);
        assert_eq!(next.last_success_at_epoch_ms, Some(2_000));
        assert_eq!(next.reason, None);
    }

    #[test]
    fn probe_interval_uses_last_observed_probe_time() {
        let previous = previous_health(ProviderHealthState::Healthy);
        assert!(!probe_due(Some(&previous), 30, 20_000));
        assert!(probe_due(Some(&previous), 30, 31_000));
        assert!(probe_due(None, 30, 0));
    }
}
