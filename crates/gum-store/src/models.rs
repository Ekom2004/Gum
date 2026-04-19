use serde::{Deserialize, Serialize};
use serde_json::Value;

use gum_types::{AttemptStatus, DeployStatus, RunStatus, TriggerType};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub api_key_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployRecord {
    pub id: String,
    pub project_id: String,
    pub version: String,
    pub bundle_url: String,
    pub bundle_sha256: String,
    pub sdk_language: String,
    pub entrypoint: String,
    pub status: DeployStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRecord {
    pub id: String,
    pub project_id: String,
    pub deploy_id: String,
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
    pub enabled: bool,
    pub created_at_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: String,
    pub project_id: String,
    pub job_id: String,
    pub deploy_id: String,
    pub trigger_type: TriggerType,
    pub status: RunStatus,
    pub input_json: Value,
    pub attempt_count: u32,
    pub max_attempts: u32,
    pub scheduled_at_epoch_ms: i64,
    pub failure_reason: Option<String>,
    pub failure_class: Option<String>,
    pub retry_after_epoch_ms: Option<i64>,
    pub waiting_for_provider_slug: Option<String>,
    pub replay_of_run_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptRecord {
    pub id: String,
    pub run_id: String,
    pub attempt_number: u32,
    pub status: AttemptStatus,
    pub lease_id: Option<String>,
    pub runner_id: Option<String>,
    pub started_at_epoch_ms: i64,
    pub finished_at_epoch_ms: Option<i64>,
    pub failure_reason: Option<String>,
    pub failure_class: Option<String>,
    pub cancel_requested_at_epoch_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseRecord {
    pub id: String,
    pub attempt_id: String,
    pub runner_id: String,
    pub expires_at_epoch_ms: i64,
    pub acked_at_epoch_ms: Option<i64>,
    pub released_at_epoch_ms: Option<i64>,
    pub revoke_requested_at_epoch_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerRecord {
    pub id: String,
    pub compute_class: String,
    pub max_concurrent_leases: u32,
    pub heartbeat_timeout_secs: u64,
    pub last_heartbeat_at_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlLeaseRecord {
    pub name: String,
    pub holder_id: String,
    pub expires_at_epoch_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderCheckStatus {
    Success,
    Failure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderHealthState {
    Healthy,
    Degraded,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionHealthState {
    Healthy,
    Degraded,
    Down,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderTargetRecord {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub probe_kind: String,
    pub probe_config_json: Value,
    pub enabled: bool,
    pub created_at_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCheckRecord {
    pub id: String,
    pub provider_target_id: String,
    pub status: ProviderCheckStatus,
    pub latency_ms: Option<u32>,
    pub error_class: Option<String>,
    pub status_code: Option<u16>,
    pub checked_at_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderHealthRecord {
    pub provider_target_id: String,
    pub provider_name: String,
    pub provider_slug: String,
    pub state: ProviderHealthState,
    pub reason: Option<String>,
    pub last_changed_at_epoch_ms: i64,
    pub last_success_at_epoch_ms: Option<i64>,
    pub last_failure_at_epoch_ms: Option<i64>,
    pub degraded_score: i32,
    pub down_score: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionHealthRecord {
    pub job_id: String,
    pub state: FunctionHealthState,
    pub consecutive_infra_failures: u32,
    pub reason: Option<String>,
    pub hold_until_epoch_ms: Option<i64>,
    pub last_changed_at_epoch_ms: i64,
    pub last_success_at_epoch_ms: Option<i64>,
    pub last_failure_at_epoch_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseStateRecord {
    pub lease_id: String,
    pub run_id: String,
    pub attempt_id: String,
    pub cancel_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerStatusRecord {
    pub id: String,
    pub compute_class: String,
    pub max_concurrent_leases: u32,
    pub last_heartbeat_at_epoch_ms: i64,
    pub active_lease_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseStatusRecord {
    pub lease_id: String,
    pub run_id: String,
    pub attempt_id: String,
    pub runner_id: String,
    pub expires_at_epoch_ms: i64,
    pub cancel_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConcurrencyStatusRecord {
    pub job_id: String,
    pub job_name: String,
    pub concurrency_limit: u32,
    pub active_run_ids: Vec<String>,
    pub queued_run_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitStatusRecord {
    pub scope_key: String,
    pub scope_kind: String,
    pub project_id: String,
    pub pool_name: Option<String>,
    pub limit: u32,
    pub window_ms: i64,
    pub recent_start_count: u32,
    pub job_ids: Vec<String>,
    pub job_names: Vec<String>,
    pub waiting_run_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunKeyRecord {
    pub project_id: String,
    pub job_id: String,
    pub key_value: String,
    pub run_id: String,
    pub created_at_epoch_ms: i64,
    pub expires_at_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogRecord {
    pub id: String,
    pub run_id: String,
    pub attempt_id: String,
    pub stream: String,
    pub message: String,
}
