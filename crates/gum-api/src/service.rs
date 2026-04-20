use std::collections::{HashMap, HashSet};

use gum_store::models::{ConcurrencyStatusRecord, LogRecord, ProviderHealthState, RunRecord};
use gum_store::queries::{
    CancelRunParams, CompleteAttemptParams, EnqueueRunParams, GumStore, HeartbeatRunnerParams,
    LeaseNextAttemptParams, RegisterDeployParams, RegisterJobParams, RegisterRunnerParams,
    ReplayRunParams,
};
use gum_types::AttemptStatus;
use serde_json::Value;

use crate::routes::{
    AppendLogRequest, CancelRunRequest, CompleteAttemptRequest, ConcurrencyListResponse,
    ConcurrencyStatusResponse, EnqueueRunRequest, EnqueueRunResponse, LeaseRunRequest,
    LeaseRunResponse, LeaseStateResponse, LeaseStatusResponse, LeasesListResponse, LogLine,
    ProviderHealthListResponse, ProviderHealthResponse, RateLimitListResponse,
    RateLimitStatusResponse, RegisterDeployRequest, RegisterDeployResponse, RegisterRunnerRequest,
    ReplayRunResponse, RunResponse, RunnerHeartbeatRequest, RunnerStatusResponse,
    RunnersListResponse, RunsListResponse,
};

pub fn register_deploy<S: GumStore>(
    store: &S,
    request: RegisterDeployRequest,
) -> Result<RegisterDeployResponse, String> {
    let params = RegisterDeployParams {
        project_id: request.project_id,
        version: request.version,
        bundle_url: request.bundle_url,
        bundle_sha256: request.bundle_sha256,
        sdk_language: request.sdk_language,
        entrypoint: request.entrypoint,
        jobs: request
            .jobs
            .into_iter()
            .map(|job| RegisterJobParams {
                id: job.id,
                name: job.name,
                handler_ref: job.handler_ref,
                trigger_mode: job.trigger_mode,
                schedule_expr: job.schedule_expr,
                retries: job.retries,
                timeout_secs: job.timeout_secs,
                rate_limit_spec: job.rate_limit_spec,
                concurrency_limit: job.concurrency_limit,
                key_field: job.key_field,
                compute_class: job.compute_class,
            })
            .collect(),
    };

    let (deploy, jobs) = store.register_deploy(params)?;
    Ok(RegisterDeployResponse {
        id: deploy.id,
        registered_jobs: jobs.len(),
    })
}

pub fn enqueue_run<S: GumStore>(
    store: &S,
    project_id: &str,
    job_id: &str,
    request: EnqueueRunRequest,
) -> Result<EnqueueRunResponse, String> {
    let job = store
        .get_job(job_id)?
        .ok_or_else(|| "job not found".to_string())?;
    let dedupe_key_value = resolve_key_value(job.key_field.as_deref(), &request.input)?;
    let enqueued = store.enqueue_run(EnqueueRunParams {
        project_id: project_id.to_string(),
        job_id: job_id.to_string(),
        deploy_id: job.deploy_id,
        input_json: request.input,
        dedupe_key_value,
    })?;
    Ok(EnqueueRunResponse {
        id: enqueued.run.id,
        status: enqueued.run.status,
        deduped: enqueued.deduped,
    })
}

pub fn get_run<S: GumStore>(store: &S, run_id: &str) -> Result<Option<RunResponse>, String> {
    let concurrency = concurrency_status_map(store)?;
    let rate_limit_waiting = rate_limit_waiting_run_ids(store)?;
    Ok(store.get_run(run_id)?.map(|run| {
        run_response(
            run.clone(),
            concurrency.get(&run.job_id),
            rate_limit_waiting.contains(&run.id),
        )
    }))
}

pub fn replay_run<S: GumStore>(store: &S, run_id: &str) -> Result<ReplayRunResponse, String> {
    let replay = store.replay_run(ReplayRunParams {
        source_run_id: run_id.to_string(),
    })?;
    let replay_of = replay
        .replay_of_run_id
        .clone()
        .ok_or_else(|| "replayed run missing source lineage".to_string())?;
    Ok(ReplayRunResponse {
        id: replay.id,
        status: replay.status,
        replay_of,
    })
}

pub fn get_logs<S: GumStore>(store: &S, run_id: &str) -> Result<Vec<LogLine>, String> {
    Ok(store
        .list_run_logs(run_id)?
        .into_iter()
        .map(|entry| LogLine {
            attempt_id: entry.attempt_id,
            stream: entry.stream,
            message: entry.message,
        })
        .collect())
}

pub fn list_runs<S: GumStore>(store: &S, limit: usize) -> Result<RunsListResponse, String> {
    let concurrency = concurrency_status_map(store)?;
    let rate_limit_waiting = rate_limit_waiting_run_ids(store)?;
    Ok(RunsListResponse {
        runs: store
            .list_recent_runs(limit)?
            .into_iter()
            .map(|run| {
                run_response(
                    run.clone(),
                    concurrency.get(&run.job_id),
                    rate_limit_waiting.contains(&run.id),
                )
            })
            .collect(),
    })
}

pub fn list_runners<S: GumStore>(store: &S) -> Result<RunnersListResponse, String> {
    Ok(RunnersListResponse {
        runners: store
            .list_runners()?
            .into_iter()
            .map(|runner| RunnerStatusResponse {
                id: runner.id,
                compute_class: runner.compute_class,
                max_concurrent_leases: runner.max_concurrent_leases,
                last_heartbeat_at_epoch_ms: runner.last_heartbeat_at_epoch_ms,
                active_lease_count: runner.active_lease_count,
            })
            .collect(),
    })
}

pub fn list_leases<S: GumStore>(store: &S) -> Result<LeasesListResponse, String> {
    Ok(LeasesListResponse {
        leases: store
            .list_active_leases()?
            .into_iter()
            .map(|lease| LeaseStatusResponse {
                lease_id: lease.lease_id,
                run_id: lease.run_id,
                attempt_id: lease.attempt_id,
                runner_id: lease.runner_id,
                expires_at_epoch_ms: lease.expires_at_epoch_ms,
                cancel_requested: lease.cancel_requested,
            })
            .collect(),
    })
}

pub fn list_concurrency<S: GumStore>(store: &S) -> Result<ConcurrencyListResponse, String> {
    Ok(ConcurrencyListResponse {
        concurrency: store
            .list_concurrency_status()?
            .into_iter()
            .map(|status| ConcurrencyStatusResponse {
                job_id: status.job_id,
                job_name: status.job_name,
                concurrency_limit: status.concurrency_limit,
                active_count: status.active_run_ids.len() as u32,
                queued_count: status.queued_run_ids.len() as u32,
                active_run_ids: status.active_run_ids,
                queued_run_ids: status.queued_run_ids,
            })
            .collect(),
    })
}

pub fn list_rate_limits<S: GumStore>(store: &S) -> Result<RateLimitListResponse, String> {
    Ok(RateLimitListResponse {
        rate_limits: store
            .list_rate_limit_status()?
            .into_iter()
            .map(|status| RateLimitStatusResponse {
                scope_key: status.scope_key,
                scope_kind: status.scope_kind,
                pool_name: status.pool_name,
                limit: status.limit,
                window_ms: status.window_ms,
                recent_start_count: status.recent_start_count,
                waiting_count: status.waiting_run_ids.len() as u32,
                job_ids: status.job_ids,
                job_names: status.job_names,
                waiting_run_ids: status.waiting_run_ids,
            })
            .collect(),
    })
}

pub fn list_provider_health<S: GumStore>(store: &S) -> Result<ProviderHealthListResponse, String> {
    Ok(ProviderHealthListResponse {
        providers: store
            .list_provider_health()?
            .into_iter()
            .map(|provider| ProviderHealthResponse {
                provider_target_id: provider.provider_target_id,
                provider_name: provider.provider_name,
                provider_slug: provider.provider_slug,
                state: provider_health_state_to_str(provider.state).to_string(),
                reason: provider.reason,
                last_changed_at_epoch_ms: provider.last_changed_at_epoch_ms,
                last_success_at_epoch_ms: provider.last_success_at_epoch_ms,
                last_failure_at_epoch_ms: provider.last_failure_at_epoch_ms,
                degraded_score: provider.degraded_score,
                down_score: provider.down_score,
            })
            .collect(),
    })
}

pub fn tick_schedules<S: GumStore>(
    store: &S,
    now_epoch_ms: i64,
) -> Result<Vec<RunResponse>, String> {
    let concurrency = concurrency_status_map(store)?;
    let rate_limit_waiting = rate_limit_waiting_run_ids(store)?;
    Ok(store
        .tick_schedules(now_epoch_ms)?
        .into_iter()
        .map(|run| {
            run_response(
                run.clone(),
                concurrency.get(&run.job_id),
                rate_limit_waiting.contains(&run.id),
            )
        })
        .collect())
}

pub fn lease_run<S: GumStore>(
    store: &S,
    request: LeaseRunRequest,
) -> Result<Option<LeaseRunResponse>, String> {
    let Some((run, attempt, lease)) = store.lease_next_attempt(LeaseNextAttemptParams {
        runner_id: request.runner_id,
        lease_ttl_secs: request.lease_ttl_secs,
    })?
    else {
        return Ok(None);
    };

    let deploy = store
        .get_deploy(&run.deploy_id)?
        .ok_or_else(|| "deploy not found".to_string())?;
    let job = store
        .get_job(&run.job_id)?
        .ok_or_else(|| "job not found".to_string())?;
    let key = resolve_key_value(job.key_field.as_deref(), &run.input_json)?;

    Ok(Some(LeaseRunResponse {
        lease_id: lease.id,
        attempt_id: attempt.id,
        run_id: run.id,
        job_id: run.job_id,
        key,
        replay_of: run.replay_of_run_id,
        deploy_id: run.deploy_id,
        input: run.input_json,
        bundle_url: deploy.bundle_url,
        entrypoint: deploy.entrypoint,
        handler_ref: job.handler_ref,
        timeout_secs: job.timeout_secs,
        lease_ttl_secs: request.lease_ttl_secs,
    }))
}

pub fn register_runner<S: GumStore>(
    store: &S,
    request: RegisterRunnerRequest,
) -> Result<(), String> {
    store.register_runner(RegisterRunnerParams {
        runner_id: request.runner_id,
        compute_class: request.compute_class,
        max_concurrent_leases: request.max_concurrent_leases,
        heartbeat_timeout_secs: request.heartbeat_timeout_secs,
    })?;
    Ok(())
}

pub fn heartbeat_runner<S: GumStore>(
    store: &S,
    request: RunnerHeartbeatRequest,
) -> Result<(), String> {
    store.heartbeat_runner(HeartbeatRunnerParams {
        runner_id: request.runner_id,
        compute_class: request.compute_class,
        max_concurrent_leases: request.max_concurrent_leases,
        heartbeat_timeout_secs: request.heartbeat_timeout_secs,
        lease_ttl_secs: request.lease_ttl_secs,
        active_lease_ids: request.active_lease_ids,
    })?;
    Ok(())
}

pub fn get_lease_state<S: GumStore>(
    store: &S,
    lease_id: &str,
) -> Result<Option<LeaseStateResponse>, String> {
    Ok(store
        .get_lease_state(lease_id)?
        .map(|state| LeaseStateResponse {
            lease_id: state.lease_id,
            run_id: state.run_id,
            attempt_id: state.attempt_id,
            cancel_requested: state.cancel_requested,
        }))
}

pub fn complete_attempt<S: GumStore>(
    store: &S,
    attempt_id: &str,
    request: CompleteAttemptRequest,
) -> Result<RunResponse, String> {
    let (_, run) = store.complete_attempt(CompleteAttemptParams {
        attempt_id: attempt_id.to_string(),
        runner_id: request.runner_id,
        status: request.status,
        failure_reason: request.failure_reason,
        failure_class: request.failure_class,
    })?;
    let concurrency = concurrency_status_map(store)?;
    let rate_limit_waiting = rate_limit_waiting_run_ids(store)?;
    Ok(run_response(
        run.clone(),
        concurrency.get(&run.job_id),
        rate_limit_waiting.contains(&run.id),
    ))
}

pub fn append_log<S: GumStore>(
    store: &S,
    run_id: &str,
    attempt_id: &str,
    request: AppendLogRequest,
) -> Result<(), String> {
    store.append_log(LogRecord {
        id: format!(
            "log_{run_id}_{attempt_id}_{}_{}",
            request.stream,
            message_fingerprint(&request.message)
        ),
        run_id: run_id.to_string(),
        attempt_id: attempt_id.to_string(),
        stream: request.stream,
        message: request.message,
    })
}

pub fn cancel_run<S: GumStore>(
    store: &S,
    run_id: &str,
    _request: CancelRunRequest,
) -> Result<RunResponse, String> {
    let run = store.cancel_run(CancelRunParams {
        run_id: run_id.to_string(),
        requested_at_epoch_ms: now_epoch_ms(),
    })?;
    let concurrency = concurrency_status_map(store)?;
    let rate_limit_waiting = rate_limit_waiting_run_ids(store)?;
    Ok(run_response(
        run.clone(),
        concurrency.get(&run.job_id),
        rate_limit_waiting.contains(&run.id),
    ))
}

fn run_response(
    run: RunRecord,
    concurrency: Option<&ConcurrencyStatusRecord>,
    waiting_on_rate_limit: bool,
) -> RunResponse {
    let waiting_reason = derive_waiting_reason(&run, concurrency, waiting_on_rate_limit);
    RunResponse {
        id: run.id,
        job_id: run.job_id,
        status: run.status,
        attempt: run.attempt_count,
        failure_reason: run.failure_reason,
        failure_class: run.failure_class,
        retry_after_epoch_ms: run.retry_after_epoch_ms,
        waiting_reason,
        waiting_for_provider_slug: run.waiting_for_provider_slug,
        replay_of: run.replay_of_run_id,
    }
}

fn concurrency_status_map<S: GumStore>(
    store: &S,
) -> Result<HashMap<String, ConcurrencyStatusRecord>, String> {
    Ok(store
        .list_concurrency_status()?
        .into_iter()
        .map(|status| (status.job_id.clone(), status))
        .collect())
}

fn rate_limit_waiting_run_ids<S: GumStore>(store: &S) -> Result<HashSet<String>, String> {
    Ok(store
        .list_rate_limit_status()?
        .into_iter()
        .flat_map(|status| status.waiting_run_ids.into_iter())
        .collect())
}

fn derive_waiting_reason(
    run: &RunRecord,
    concurrency: Option<&ConcurrencyStatusRecord>,
    waiting_on_rate_limit: bool,
) -> Option<String> {
    if run.status != gum_types::RunStatus::Queued {
        return None;
    }
    if run.failure_class.as_deref() == Some("blocked_by_downstream") {
        return Some("waiting_for_function_health".to_string());
    }
    if run.retry_after_epoch_ms.is_some() {
        return None;
    }
    if let Some(concurrency) = concurrency {
        if concurrency.active_run_ids.len() as u32 >= concurrency.concurrency_limit {
            return Some("waiting_on_concurrency".to_string());
        }
    }
    if waiting_on_rate_limit {
        return Some("waiting_on_rate_limit".to_string());
    }
    None
}

fn provider_health_state_to_str(state: ProviderHealthState) -> &'static str {
    match state {
        ProviderHealthState::Healthy => "healthy",
        ProviderHealthState::Degraded => "degraded",
        ProviderHealthState::Down => "down",
    }
}

pub fn is_terminal_attempt(status: AttemptStatus) -> bool {
    matches!(
        status,
        AttemptStatus::Succeeded
            | AttemptStatus::Failed
            | AttemptStatus::TimedOut
            | AttemptStatus::Canceled
    )
}

fn message_fingerprint(message: &str) -> u64 {
    let mut hash = 1469598103934665603_u64;
    for byte in message.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

fn now_epoch_ms() -> i64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_millis() as i64,
        Err(_) => 0,
    }
}

fn resolve_key_value(key_field: Option<&str>, input: &Value) -> Result<Option<String>, String> {
    let Some(key_field) = key_field else {
        return Ok(None);
    };
    let value = input
        .get(key_field)
        .ok_or_else(|| format!("key field \"{key_field}\" missing from input"))?;
    let resolved = match value {
        Value::String(inner) => inner.clone(),
        Value::Number(inner) => inner.to_string(),
        Value::Bool(inner) => inner.to_string(),
        Value::Null => return Err(format!("key field \"{key_field}\" must not be null")),
        Value::Array(_) | Value::Object(_) => {
            return Err(format!(
                "key field \"{key_field}\" must resolve to a scalar value"
            ));
        }
    };
    Ok(Some(resolved))
}
