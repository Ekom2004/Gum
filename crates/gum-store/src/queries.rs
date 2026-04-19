use serde_json::Value;

use crate::models::{
    AttemptRecord, ConcurrencyStatusRecord, DeployRecord, FunctionHealthRecord,
    FunctionHealthState, JobRecord, LeaseRecord, LeaseStateRecord, LeaseStatusRecord, LogRecord,
    ProviderCheckRecord, ProviderCheckStatus, ProviderHealthRecord, ProviderHealthState,
    ProviderTargetRecord, RunRecord, RunnerRecord, RunnerStatusRecord,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterDeployParams {
    pub project_id: String,
    pub version: String,
    pub bundle_url: String,
    pub bundle_sha256: String,
    pub sdk_language: String,
    pub entrypoint: String,
    pub jobs: Vec<RegisterJobParams>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterJobParams {
    pub id: String,
    pub name: String,
    pub handler_ref: String,
    pub trigger_mode: String,
    pub schedule_expr: Option<String>,
    pub retries: u32,
    pub timeout_secs: u32,
    pub rate_limit_spec: Option<String>,
    pub concurrency_limit: Option<u32>,
    pub key_field: Option<String>,
    pub compute_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnqueueRunParams {
    pub project_id: String,
    pub job_id: String,
    pub deploy_id: String,
    pub input_json: Value,
    pub dedupe_key_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnqueueRunResult {
    pub run: RunRecord,
    pub deduped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayRunParams {
    pub source_run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseNextAttemptParams {
    pub runner_id: String,
    pub lease_ttl_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterRunnerParams {
    pub runner_id: String,
    pub compute_class: String,
    pub max_concurrent_leases: u32,
    pub heartbeat_timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatRunnerParams {
    pub runner_id: String,
    pub compute_class: String,
    pub max_concurrent_leases: u32,
    pub heartbeat_timeout_secs: u64,
    pub lease_ttl_secs: u64,
    pub active_lease_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlLeaseParams {
    pub lease_name: String,
    pub holder_id: String,
    pub ttl_secs: u64,
    pub now_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteAttemptParams {
    pub attempt_id: String,
    pub runner_id: String,
    pub status: gum_types::AttemptStatus,
    pub failure_reason: Option<String>,
    pub failure_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelRunParams {
    pub run_id: String,
    pub requested_at_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpsertProviderTargetParams {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub probe_kind: String,
    pub probe_config_json: Value,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordProviderCheckParams {
    pub provider_target_id: String,
    pub status: ProviderCheckStatus,
    pub latency_ms: Option<u32>,
    pub error_class: Option<String>,
    pub status_code: Option<u16>,
    pub checked_at_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetProviderHealthParams {
    pub provider_target_id: String,
    pub state: ProviderHealthState,
    pub reason: Option<String>,
    pub last_changed_at_epoch_ms: i64,
    pub last_success_at_epoch_ms: Option<i64>,
    pub last_failure_at_epoch_ms: Option<i64>,
    pub degraded_score: i32,
    pub down_score: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetFunctionHealthParams {
    pub job_id: String,
    pub state: FunctionHealthState,
    pub consecutive_infra_failures: u32,
    pub reason: Option<String>,
    pub hold_until_epoch_ms: Option<i64>,
    pub last_changed_at_epoch_ms: i64,
    pub last_success_at_epoch_ms: Option<i64>,
    pub last_failure_at_epoch_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryDisposition {
    pub next_status: gum_types::RunStatus,
    pub failure_reason: Option<String>,
    pub failure_class: Option<String>,
    pub retry_after_epoch_ms: Option<i64>,
    pub waiting_for_scope_key: Option<String>,
    pub finished_now: bool,
}

pub fn parse_schedule_interval_ms(expr: &str) -> Result<i64, String> {
    if expr.len() < 2 {
        return Err(format!("invalid schedule expression: {expr}"));
    }

    let (amount, unit) = expr.split_at(expr.len() - 1);
    let value: i64 = amount
        .parse()
        .map_err(|_| format!("invalid schedule expression: {expr}"))?;
    if value <= 0 {
        return Err(format!("schedule expression must be positive: {expr}"));
    }

    let multiplier = match unit {
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        _ => return Err(format!("unsupported schedule expression: {expr}")),
    };

    Ok(value.saturating_mul(multiplier))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitSpec {
    pub pool: Option<String>,
    pub limit: u32,
    pub window_ms: i64,
}

pub fn parse_rate_limit_spec(spec: &str) -> Result<RateLimitSpec, String> {
    let (pool, quota) = match spec.rsplit_once(':') {
        Some((pool_name, quota)) => {
            if pool_name.is_empty() {
                return Err(format!("rate limit pool must not be empty: {spec}"));
            }
            (Some(pool_name.to_string()), quota)
        }
        None => (None, spec),
    };

    let (limit_raw, unit) = quota
        .split_once('/')
        .ok_or_else(|| format!("invalid rate limit spec: {spec}"))?;
    let limit: u32 = limit_raw
        .parse()
        .map_err(|_| format!("invalid rate limit spec: {spec}"))?;
    if limit == 0 {
        return Err(format!("rate limit must be positive: {spec}"));
    }

    let window_ms = match unit {
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        _ => return Err(format!("unsupported rate limit spec: {spec}")),
    };

    Ok(RateLimitSpec {
        pool,
        limit,
        window_ms,
    })
}

pub fn provider_slug_from_job(job: &JobRecord) -> Result<Option<String>, String> {
    let Some(spec) = job.rate_limit_spec.as_deref() else {
        return Ok(None);
    };
    Ok(parse_rate_limit_spec(spec)?.pool)
}

pub fn is_provider_failure_class(failure_class: Option<&str>) -> bool {
    matches!(
        failure_class,
        Some(
            "provider_timeout"
                | "provider_connect_error"
                | "provider_5xx"
                | "provider_429"
                | "provider_auth_error"
        )
    )
}

pub fn is_retryable_failure_class(failure_class: Option<&str>) -> bool {
    match failure_class {
        None => true,
        Some("user_code_error" | "provider_auth_error") => false,
        Some(
            "provider_timeout"
            | "provider_connect_error"
            | "provider_5xx"
            | "provider_429"
            | "gum_internal_error"
            | "job_timeout"
            | "provider_probe_error"
            | "provider_probe_config_error"
            | "provider_http_error",
        ) => true,
        Some(_) => true,
    }
}

pub fn is_infrastructure_failure_class(failure_class: Option<&str>) -> bool {
    matches!(
        failure_class,
        Some("provider_timeout" | "provider_connect_error" | "provider_5xx" | "provider_429")
    )
}

pub fn function_health_hold_delay_ms() -> i64 {
    30_000
}

pub fn key_retention_ms() -> i64 {
    86_400_000
}

pub fn compute_retry_disposition(
    run_id: &str,
    attempt_count: u32,
    max_attempts: u32,
    status: gum_types::AttemptStatus,
    failure_reason: Option<String>,
    failure_class: Option<String>,
    function_health: Option<&FunctionHealthRecord>,
    now_epoch_ms: i64,
) -> RetryDisposition {
    use gum_types::{AttemptStatus, RunStatus};

    match status {
        AttemptStatus::Succeeded => RetryDisposition {
            next_status: RunStatus::Succeeded,
            failure_reason: None,
            failure_class: None,
            retry_after_epoch_ms: None,
            waiting_for_scope_key: None,
            finished_now: true,
        },
        AttemptStatus::Canceled => RetryDisposition {
            next_status: RunStatus::Canceled,
            failure_reason,
            failure_class,
            retry_after_epoch_ms: None,
            waiting_for_scope_key: None,
            finished_now: true,
        },
        AttemptStatus::Failed | AttemptStatus::TimedOut => {
            let terminal_status = match status {
                AttemptStatus::TimedOut => RunStatus::TimedOut,
                _ => RunStatus::Failed,
            };
            if attempt_count >= max_attempts
                || !is_retryable_failure_class(failure_class.as_deref())
            {
                return RetryDisposition {
                    next_status: terminal_status,
                    failure_reason,
                    failure_class,
                    retry_after_epoch_ms: None,
                    waiting_for_scope_key: None,
                    finished_now: true,
                };
            }

            if function_health
                .map(|health| {
                    matches!(
                        health.state,
                        FunctionHealthState::Degraded | FunctionHealthState::Down
                    )
                })
                .unwrap_or(false)
                && is_infrastructure_failure_class(failure_class.as_deref())
            {
                let hold_until_epoch_ms = function_health
                    .and_then(|health| health.hold_until_epoch_ms)
                    .unwrap_or(now_epoch_ms + function_health_hold_delay_ms());
                return RetryDisposition {
                    next_status: RunStatus::Queued,
                    failure_reason: Some("waiting for function health recovery".to_string()),
                    failure_class: Some("blocked_by_downstream".to_string()),
                    retry_after_epoch_ms: Some(hold_until_epoch_ms),
                    waiting_for_scope_key: function_health.map(|health| health.job_id.clone()),
                    finished_now: false,
                };
            }

            let delay_ms = retry_backoff_delay_ms(run_id, attempt_count, failure_class.as_deref());
            let retry_reason = match failure_class.as_deref() {
                Some(class_name) => {
                    format!("retrying after {} in {}s", class_name, delay_ms / 1000)
                }
                None => format!("retrying in {}s", delay_ms / 1000),
            };
            RetryDisposition {
                next_status: RunStatus::Queued,
                failure_reason: Some(retry_reason),
                failure_class,
                retry_after_epoch_ms: Some(now_epoch_ms + delay_ms),
                waiting_for_scope_key: None,
                finished_now: false,
            }
        }
        AttemptStatus::Queued | AttemptStatus::Leased | AttemptStatus::Running => {
            RetryDisposition {
                next_status: gum_types::RunStatus::Queued,
                failure_reason,
                failure_class,
                retry_after_epoch_ms: None,
                waiting_for_scope_key: None,
                finished_now: false,
            }
        }
    }
}

pub fn retry_backoff_delay_ms(
    run_id: &str,
    attempt_count: u32,
    failure_class: Option<&str>,
) -> i64 {
    let capped_attempt = attempt_count.min(6);
    let base_ms: i64 = match failure_class {
        Some("provider_429") => 15_000,
        Some("provider_timeout" | "provider_connect_error" | "provider_5xx") => 2_000,
        Some("gum_internal_error") => 5_000,
        Some("job_timeout") => 10_000,
        _ => 3_000,
    };
    let exponential_ms = base_ms.saturating_mul(1_i64 << capped_attempt.saturating_sub(1));
    let capped_ms = exponential_ms.min(300_000);
    capped_ms + deterministic_jitter_ms(run_id, attempt_count, capped_ms / 5)
}

// Deterministic jitter keeps retries from stampeding together without making tests flaky.
fn deterministic_jitter_ms(run_id: &str, attempt_count: u32, max_jitter_ms: i64) -> i64 {
    if max_jitter_ms <= 0 {
        return 0;
    }

    let mut hash = 1469598103934665603_u64;
    for byte in run_id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1099511628211);
    }
    hash ^= u64::from(attempt_count);
    (hash % max_jitter_ms as u64) as i64
}

pub trait GumStore {
    fn register_deploy(
        &self,
        params: RegisterDeployParams,
    ) -> Result<(DeployRecord, Vec<JobRecord>), String>;
    fn upsert_provider_target(
        &self,
        params: UpsertProviderTargetParams,
    ) -> Result<ProviderTargetRecord, String>;
    fn record_provider_check(
        &self,
        params: RecordProviderCheckParams,
    ) -> Result<ProviderCheckRecord, String>;
    fn set_provider_health(
        &self,
        params: SetProviderHealthParams,
    ) -> Result<ProviderHealthRecord, String>;
    fn set_function_health(
        &self,
        params: SetFunctionHealthParams,
    ) -> Result<FunctionHealthRecord, String>;
    fn get_function_health(&self, job_id: &str) -> Result<Option<FunctionHealthRecord>, String>;
    fn list_provider_targets(&self) -> Result<Vec<ProviderTargetRecord>, String>;
    fn list_provider_health(&self) -> Result<Vec<ProviderHealthRecord>, String>;
    fn register_runner(&self, params: RegisterRunnerParams) -> Result<RunnerRecord, String>;
    fn heartbeat_runner(&self, params: HeartbeatRunnerParams) -> Result<RunnerRecord, String>;
    fn try_acquire_control_lease(&self, params: ControlLeaseParams) -> Result<bool, String>;
    fn get_job(&self, job_id: &str) -> Result<Option<JobRecord>, String>;
    fn get_run(&self, run_id: &str) -> Result<Option<RunRecord>, String>;
    fn get_deploy(&self, deploy_id: &str) -> Result<Option<DeployRecord>, String>;
    fn get_lease_state(&self, lease_id: &str) -> Result<Option<LeaseStateRecord>, String>;
    fn list_recent_runs(&self, limit: usize) -> Result<Vec<RunRecord>, String>;
    fn list_runners(&self) -> Result<Vec<RunnerStatusRecord>, String>;
    fn list_active_leases(&self) -> Result<Vec<LeaseStatusRecord>, String>;
    fn list_concurrency_status(&self) -> Result<Vec<ConcurrencyStatusRecord>, String>;
    fn enqueue_run(&self, params: EnqueueRunParams) -> Result<EnqueueRunResult, String>;
    fn replay_run(&self, params: ReplayRunParams) -> Result<RunRecord, String>;
    fn lease_next_attempt(
        &self,
        params: LeaseNextAttemptParams,
    ) -> Result<Option<(RunRecord, AttemptRecord, LeaseRecord)>, String>;
    fn complete_attempt(
        &self,
        params: CompleteAttemptParams,
    ) -> Result<(AttemptRecord, RunRecord), String>;
    fn cancel_run(&self, params: CancelRunParams) -> Result<RunRecord, String>;
    fn recover_lost_attempts(&self, now_epoch_ms: i64) -> Result<Vec<RunRecord>, String>;
    fn tick_schedules(&self, now_epoch_ms: i64) -> Result<Vec<RunRecord>, String>;
    fn append_log(&self, log: LogRecord) -> Result<(), String>;
    fn list_run_logs(&self, run_id: &str) -> Result<Vec<LogRecord>, String>;
}
