use serde::{Deserialize, Serialize};
use serde_json::Value;

use gum_types::{AttemptStatus, RunStatus};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegisterDeployRequest {
    pub project_id: String,
    pub version: String,
    pub bundle_url: String,
    pub bundle_sha256: String,
    pub sdk_language: String,
    pub entrypoint: String,
    pub jobs: Vec<RegisteredJob>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredJob {
    pub id: String,
    pub name: String,
    pub handler_ref: String,
    pub trigger_mode: String,
    pub schedule_expr: Option<String>,
    pub retries: u32,
    pub timeout_secs: u32,
    pub rate_limit_spec: Option<String>,
    pub concurrency_limit: Option<u32>,
    #[serde(default)]
    pub cpu_cores: Option<u32>,
    pub memory_mb: Option<u32>,
    pub key_field: Option<String>,
    pub compute_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterDeployResponse {
    pub id: String,
    pub registered_jobs: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnqueueRunRequest {
    pub input: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnqueueRunResponse {
    pub id: String,
    pub status: RunStatus,
    pub deduped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunResponse {
    pub id: String,
    pub job_id: String,
    pub status: RunStatus,
    pub attempt: u32,
    pub failure_reason: Option<String>,
    pub failure_class: Option<String>,
    pub retry_after_epoch_ms: Option<i64>,
    pub waiting_reason: Option<String>,
    pub waiting_for_provider_slug: Option<String>,
    pub replay_of: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayRunResponse {
    pub id: String,
    pub status: RunStatus,
    pub replay_of: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogLine {
    pub attempt_id: String,
    pub stream: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseRunRequest {
    pub runner_id: String,
    pub lease_ttl_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaseRunResponse {
    pub lease_id: String,
    pub attempt_id: String,
    pub run_id: String,
    pub job_id: String,
    pub key: Option<String>,
    pub replay_of: Option<String>,
    pub deploy_id: String,
    pub input: Value,
    pub bundle_url: String,
    pub entrypoint: String,
    pub handler_ref: String,
    pub timeout_secs: u32,
    pub cpu_cores: Option<u32>,
    pub memory_mb: Option<u32>,
    pub lease_ttl_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterRunnerRequest {
    pub runner_id: String,
    pub compute_class: String,
    #[serde(default = "default_runner_cpu_cores")]
    pub cpu_cores: u32,
    #[serde(default = "default_runner_memory_mb")]
    pub memory_mb: u32,
    pub max_concurrent_leases: u32,
    pub heartbeat_timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerHeartbeatRequest {
    pub runner_id: String,
    pub compute_class: String,
    #[serde(default = "default_runner_cpu_cores")]
    pub cpu_cores: u32,
    #[serde(default = "default_runner_memory_mb")]
    pub memory_mb: u32,
    pub max_concurrent_leases: u32,
    pub heartbeat_timeout_secs: u64,
    pub lease_ttl_secs: u64,
    pub active_lease_ids: Vec<String>,
}

fn default_runner_memory_mb() -> u32 {
    1024
}

fn default_runner_cpu_cores() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseStateResponse {
    pub lease_id: String,
    pub run_id: String,
    pub attempt_id: String,
    pub cancel_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunsListResponse {
    pub runs: Vec<RunResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerStatusResponse {
    pub id: String,
    pub compute_class: String,
    pub cpu_cores: u32,
    pub memory_mb: u32,
    pub active_cpu_cores: u32,
    pub active_memory_mb: u32,
    pub max_concurrent_leases: u32,
    pub last_heartbeat_at_epoch_ms: i64,
    pub active_lease_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnersListResponse {
    pub runners: Vec<RunnerStatusResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseStatusResponse {
    pub lease_id: String,
    pub run_id: String,
    pub attempt_id: String,
    pub runner_id: String,
    pub expires_at_epoch_ms: i64,
    pub cancel_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeasesListResponse {
    pub leases: Vec<LeaseStatusResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConcurrencyStatusResponse {
    pub job_id: String,
    pub job_name: String,
    pub concurrency_limit: u32,
    pub active_count: u32,
    pub queued_count: u32,
    pub active_run_ids: Vec<String>,
    pub queued_run_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConcurrencyListResponse {
    pub concurrency: Vec<ConcurrencyStatusResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitStatusResponse {
    pub scope_key: String,
    pub scope_kind: String,
    pub pool_name: Option<String>,
    pub limit: u32,
    pub window_ms: i64,
    pub recent_start_count: u32,
    pub waiting_count: u32,
    pub job_ids: Vec<String>,
    pub job_names: Vec<String>,
    pub waiting_run_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitListResponse {
    pub rate_limits: Vec<RateLimitStatusResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderHealthResponse {
    pub provider_target_id: String,
    pub provider_name: String,
    pub provider_slug: String,
    pub state: String,
    pub reason: Option<String>,
    pub last_changed_at_epoch_ms: i64,
    pub last_success_at_epoch_ms: Option<i64>,
    pub last_failure_at_epoch_ms: Option<i64>,
    pub degraded_score: i32,
    pub down_score: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderHealthListResponse {
    pub providers: Vec<ProviderHealthResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteAttemptRequest {
    pub runner_id: String,
    pub status: AttemptStatus,
    pub failure_reason: Option<String>,
    pub failure_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelRunRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppendLogRequest {
    pub stream: String,
    pub message: String,
}
