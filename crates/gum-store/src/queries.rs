use serde_json::Value;

use crate::models::{
    AttemptRecord, DeployRecord, JobRecord, LeaseRecord, LeaseStateRecord, LogRecord, RunRecord,
    RunnerRecord,
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
    pub compute_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnqueueRunParams {
    pub project_id: String,
    pub job_id: String,
    pub deploy_id: String,
    pub input_json: Value,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelRunParams {
    pub run_id: String,
    pub requested_at_epoch_ms: i64,
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

pub trait GumStore {
    fn register_deploy(
        &self,
        params: RegisterDeployParams,
    ) -> Result<(DeployRecord, Vec<JobRecord>), String>;
    fn register_runner(&self, params: RegisterRunnerParams) -> Result<RunnerRecord, String>;
    fn heartbeat_runner(&self, params: HeartbeatRunnerParams) -> Result<RunnerRecord, String>;
    fn try_acquire_control_lease(&self, params: ControlLeaseParams) -> Result<bool, String>;
    fn get_job(&self, job_id: &str) -> Result<Option<JobRecord>, String>;
    fn get_run(&self, run_id: &str) -> Result<Option<RunRecord>, String>;
    fn get_deploy(&self, deploy_id: &str) -> Result<Option<DeployRecord>, String>;
    fn get_lease_state(&self, lease_id: &str) -> Result<Option<LeaseStateRecord>, String>;
    fn enqueue_run(&self, params: EnqueueRunParams) -> Result<RunRecord, String>;
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
