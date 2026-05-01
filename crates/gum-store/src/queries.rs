use chrono::{Datelike, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use serde_json::Value;

use crate::models::{
    AttemptRecord, ConcurrencyStatusRecord, DeployRecord, FunctionHealthRecord,
    FunctionHealthState, JobRecord, LeaseRecord, LeaseStateRecord, LeaseStatusRecord, LogRecord,
    ProviderCheckRecord, ProviderCheckStatus, ProviderHealthRecord, ProviderHealthState,
    ProviderTargetRecord, RateLimitStatusRecord, RunRecord, RunnerRecord, RunnerStatusRecord,
};
use gum_types::DeployStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterDeployParams {
    pub project_id: String,
    pub version: String,
    pub bundle_url: String,
    pub bundle_sha256: String,
    pub sdk_language: String,
    pub entrypoint: String,
    pub python_version: Option<String>,
    pub deps_mode: Option<String>,
    pub deps_hash: Option<String>,
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
    pub cpu_cores: Option<u32>,
    pub memory_mb: Option<u32>,
    pub key_field: Option<String>,
    pub compute_class: Option<String>,
    pub required_secret_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnqueueRunParams {
    pub project_id: String,
    pub job_id: String,
    pub deploy_id: String,
    pub input_json: Value,
    pub dedupe_key_value: Option<String>,
    pub delay_ms: Option<i64>,
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
pub struct SetDeployStatusParams {
    pub deploy_id: String,
    pub status: DeployStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterRunnerParams {
    pub runner_id: String,
    pub compute_class: String,
    pub cpu_cores: u32,
    pub memory_mb: u32,
    pub max_concurrent_leases: u32,
    pub heartbeat_timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatRunnerParams {
    pub runner_id: String,
    pub compute_class: String,
    pub cpu_cores: u32,
    pub memory_mb: u32,
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
enum RecurringSchedule {
    IntervalMs(i64),
    Cron(CronSchedule),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CronField {
    allowed: Vec<bool>,
    wildcard: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CronSchedule {
    minute: CronField,
    hour: CronField,
    day_of_month: CronField,
    month: CronField,
    day_of_week: CronField,
    timezone: Tz,
}

pub fn validate_recurring_schedule_expr(expr: &str) -> Result<(), String> {
    let _ = parse_recurring_schedule(expr)?;
    Ok(())
}

pub fn recurring_due_times_ms(
    expr: &str,
    created_at_epoch_ms: i64,
    latest_scheduled_epoch_ms: Option<i64>,
    now_epoch_ms: i64,
) -> Result<Vec<i64>, String> {
    let schedule = parse_recurring_schedule(expr)?;
    if now_epoch_ms < created_at_epoch_ms {
        return Ok(Vec::new());
    }

    match schedule {
        RecurringSchedule::IntervalMs(interval_ms) => {
            let mut due = Vec::new();
            let mut next_due_ms = latest_scheduled_epoch_ms
                .unwrap_or(created_at_epoch_ms)
                .saturating_add(interval_ms);
            while next_due_ms <= now_epoch_ms {
                due.push(next_due_ms);
                next_due_ms = next_due_ms.saturating_add(interval_ms);
            }
            Ok(due)
        }
        RecurringSchedule::Cron(cron) => {
            let mut due = Vec::new();
            let mut cursor_ms = latest_scheduled_epoch_ms.unwrap_or(created_at_epoch_ms);
            loop {
                let Some(next_due_ms) = cron.next_match_after_epoch_ms(cursor_ms)? else {
                    break;
                };
                if next_due_ms > now_epoch_ms {
                    break;
                }
                due.push(next_due_ms);
                cursor_ms = next_due_ms;
            }
            Ok(due)
        }
    }
}

fn parse_recurring_schedule(expr: &str) -> Result<RecurringSchedule, String> {
    let trimmed = expr.trim();
    if let Some(raw_cron) = trimmed.strip_prefix("cron:") {
        return Ok(RecurringSchedule::Cron(parse_cron_schedule(
            raw_cron, expr,
        )?));
    }
    Ok(RecurringSchedule::IntervalMs(parse_schedule_interval_ms(
        trimmed,
    )?))
}

fn parse_cron_schedule(raw: &str, full_expr: &str) -> Result<CronSchedule, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(format!("invalid cron expression: {full_expr}"));
    }

    if let Some(without_tz_prefix) = trimmed.strip_prefix("tz=") {
        let (timezone_raw, cron_raw) = without_tz_prefix
            .split_once(';')
            .ok_or_else(|| format!("invalid cron expression: {full_expr}"))?;
        let timezone_name = timezone_raw.trim();
        if timezone_name.is_empty() {
            return Err(format!("invalid cron timezone in expression: {full_expr}"));
        }
        let timezone = parse_cron_timezone(timezone_name)?;
        let cron_expr = cron_raw.trim();
        if cron_expr.is_empty() {
            return Err(format!("invalid cron expression: {full_expr}"));
        }
        return CronSchedule::parse(cron_expr, timezone);
    }

    CronSchedule::parse(trimmed, chrono_tz::UTC)
}

fn parse_cron_timezone(raw: &str) -> Result<Tz, String> {
    raw.parse::<Tz>()
        .map_err(|_| format!("invalid cron timezone: {raw}"))
}

impl CronField {
    fn parse(
        field_expr: &str,
        min: u32,
        max: u32,
        field_name: &str,
        allow_sunday_seven: bool,
    ) -> Result<Self, String> {
        let expression = field_expr.trim();
        if expression.is_empty() {
            return Err(format!("invalid cron expression: empty {field_name} field"));
        }

        let mut allowed = vec![false; (max + 1) as usize];
        let wildcard = expression == "*";
        for segment in expression.split(',') {
            let segment = segment.trim();
            if segment.is_empty() {
                return Err(format!("invalid cron expression segment in {field_name}"));
            }
            let (base, step) = parse_cron_step(segment, field_name)?;
            let (start, end) =
                parse_cron_base_range(base, min, max, field_name, allow_sunday_seven)?;
            if step == 0 {
                return Err(format!("cron step must be positive in {field_name}"));
            }
            let mut value = start;
            while value <= end {
                allowed[value as usize] = true;
                let next = value.saturating_add(step);
                if next <= value {
                    break;
                }
                value = next;
            }
        }

        if allowed.iter().skip(min as usize).all(|flag| !*flag) {
            return Err(format!(
                "invalid cron expression: {field_name} has no values"
            ));
        }

        Ok(Self { allowed, wildcard })
    }

    fn contains(&self, value: u32) -> bool {
        self.allowed.get(value as usize).copied().unwrap_or(false)
    }
}

impl CronSchedule {
    fn parse(expr: &str, timezone: Tz) -> Result<Self, String> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(format!("invalid cron expression: {expr}"));
        }
        Ok(Self {
            minute: CronField::parse(parts[0], 0, 59, "minute", false)?,
            hour: CronField::parse(parts[1], 0, 23, "hour", false)?,
            day_of_month: CronField::parse(parts[2], 1, 31, "day_of_month", false)?,
            month: CronField::parse(parts[3], 1, 12, "month", false)?,
            day_of_week: CronField::parse(parts[4], 0, 6, "day_of_week", true)?,
            timezone,
        })
    }

    fn next_match_after_epoch_ms(&self, cursor_epoch_ms: i64) -> Result<Option<i64>, String> {
        const MAX_SEARCH_MINUTES: i64 = 5 * 366 * 24 * 60;
        let mut candidate_ms = floor_to_minute_epoch_ms(cursor_epoch_ms).saturating_add(60_000);

        for _ in 0..MAX_SEARCH_MINUTES {
            if self.matches_epoch_ms(candidate_ms) {
                return Ok(Some(candidate_ms));
            }
            let next_candidate = candidate_ms.saturating_add(60_000);
            if next_candidate <= candidate_ms {
                return Ok(None);
            }
            candidate_ms = next_candidate;
        }

        Err("invalid cron expression: no matching fire time within 5 years".to_string())
    }

    fn matches_epoch_ms(&self, epoch_ms: i64) -> bool {
        let Some(utc_dt) = Utc.timestamp_millis_opt(epoch_ms).single() else {
            return false;
        };
        let local_dt = utc_dt.with_timezone(&self.timezone);
        let minute = local_dt.minute();
        let hour = local_dt.hour();
        let month = local_dt.month();
        let day = local_dt.day();
        let weekday = local_dt.weekday().num_days_from_sunday();

        if !self.minute.contains(minute) {
            return false;
        }
        if !self.hour.contains(hour) {
            return false;
        }
        if !self.month.contains(month) {
            return false;
        }

        let day_of_month_match = self.day_of_month.contains(day);
        let day_of_week_match = self.day_of_week.contains(weekday);
        if self.day_of_month.wildcard && self.day_of_week.wildcard {
            return true;
        }
        if self.day_of_month.wildcard {
            return day_of_week_match;
        }
        if self.day_of_week.wildcard {
            return day_of_month_match;
        }
        day_of_month_match || day_of_week_match
    }
}

fn parse_cron_step<'a>(segment: &'a str, field_name: &str) -> Result<(&'a str, u32), String> {
    if let Some((base, step_raw)) = segment.split_once('/') {
        let step = step_raw
            .parse::<u32>()
            .map_err(|_| format!("invalid cron step in {field_name}: {segment}"))?;
        if step == 0 {
            return Err(format!(
                "cron step must be positive in {field_name}: {segment}"
            ));
        }
        return Ok((base, step));
    }
    Ok((segment, 1))
}

fn parse_cron_base_range(
    base: &str,
    min: u32,
    max: u32,
    field_name: &str,
    allow_sunday_seven: bool,
) -> Result<(u32, u32), String> {
    if base == "*" {
        return Ok((min, max));
    }
    if let Some((start_raw, end_raw)) = base.split_once('-') {
        let start = parse_cron_number(start_raw, min, max, field_name, allow_sunday_seven)?;
        let end = parse_cron_number(end_raw, min, max, field_name, allow_sunday_seven)?;
        if end < start {
            return Err(format!(
                "invalid cron range in {field_name}: start must be <= end"
            ));
        }
        return Ok((start, end));
    }
    let value = parse_cron_number(base, min, max, field_name, allow_sunday_seven)?;
    Ok((value, value))
}

fn parse_cron_number(
    raw: &str,
    min: u32,
    max: u32,
    field_name: &str,
    allow_sunday_seven: bool,
) -> Result<u32, String> {
    let parsed = raw
        .parse::<u32>()
        .map_err(|_| format!("invalid cron value in {field_name}: {raw}"))?;
    let normalized = if allow_sunday_seven && parsed == 7 {
        0
    } else {
        parsed
    };
    if normalized < min || normalized > max {
        return Err(format!(
            "cron value out of range for {field_name}: {raw} (expected {min}-{max})"
        ));
    }
    Ok(normalized)
}

fn floor_to_minute_epoch_ms(epoch_ms: i64) -> i64 {
    epoch_ms - epoch_ms.rem_euclid(60_000)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitSpec {
    pub pool: Option<String>,
    pub limit: u32,
    pub window_ms: i64,
}

pub fn rate_limit_scope_key(project_id: &str, job_id: &str, spec: &RateLimitSpec) -> String {
    match spec.pool.as_deref() {
        Some(pool_name) => format!("pool:{project_id}:{pool_name}"),
        None => format!("job:{job_id}"),
    }
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
    fn set_deploy_status(&self, params: SetDeployStatusParams) -> Result<DeployRecord, String>;
    fn get_lease_state(&self, lease_id: &str) -> Result<Option<LeaseStateRecord>, String>;
    fn list_deploys_by_status(
        &self,
        status: DeployStatus,
        limit: usize,
    ) -> Result<Vec<DeployRecord>, String>;
    fn list_recent_runs(&self, limit: usize) -> Result<Vec<RunRecord>, String>;
    fn list_runners(&self) -> Result<Vec<RunnerStatusRecord>, String>;
    fn list_active_leases(&self) -> Result<Vec<LeaseStatusRecord>, String>;
    fn list_concurrency_status(&self) -> Result<Vec<ConcurrencyStatusRecord>, String>;
    fn list_rate_limit_status(&self) -> Result<Vec<RateLimitStatusRecord>, String>;
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

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::{recurring_due_times_ms, validate_recurring_schedule_expr};

    #[test]
    fn recurring_due_times_supports_interval_expressions() {
        let due = recurring_due_times_ms("20m", 0, None, 3_600_000)
            .expect("interval recurring schedule should parse");
        assert_eq!(due, vec![1_200_000, 2_400_000, 3_600_000]);
    }

    #[test]
    fn recurring_due_times_supports_cron_expressions() {
        let due = recurring_due_times_ms("cron:*/15 * * * *", 0, None, 2_700_000)
            .expect("cron recurring schedule should parse");
        assert_eq!(due, vec![900_000, 1_800_000, 2_700_000]);
    }

    #[test]
    fn recurring_due_times_supports_cron_with_timezone() {
        let created = Utc
            .with_ymd_and_hms(2026, 4, 27, 12, 0, 0)
            .single()
            .expect("created time should be valid")
            .timestamp_millis();
        let now = Utc
            .with_ymd_and_hms(2026, 4, 27, 14, 0, 0)
            .single()
            .expect("now time should be valid")
            .timestamp_millis();
        let expected = Utc
            .with_ymd_and_hms(2026, 4, 27, 13, 0, 0)
            .single()
            .expect("expected due time should be valid")
            .timestamp_millis();

        let due = recurring_due_times_ms("cron:tz=America/New_York;0 9 * * 1", created, None, now)
            .expect("timezone cron recurring schedule should parse");
        assert_eq!(due, vec![expected]);
    }

    #[test]
    fn recurring_schedule_validation_rejects_invalid_cron() {
        let error =
            validate_recurring_schedule_expr("cron:*/0 * * * *").expect_err("cron should fail");
        assert!(error.contains("step"), "unexpected error: {error}");
    }

    #[test]
    fn recurring_schedule_validation_rejects_invalid_timezone() {
        let error = validate_recurring_schedule_expr("cron:tz=Not/A_Zone;0 9 * * 1")
            .expect_err("timezone should fail");
        assert!(error.contains("timezone"), "unexpected error: {error}");
    }
}
