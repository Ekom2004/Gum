use gum_types::AttemptStatus;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeasedRun {
    pub lease_id: String,
    pub attempt_id: String,
    pub run_id: String,
    pub job_id: String,
    pub key: Option<String>,
    pub replay_of: Option<String>,
    pub deploy_id: String,
    pub bundle_url: String,
    pub entrypoint: String,
    pub handler_ref: String,
    pub timeout_secs: u32,
    pub memory_mb: Option<u32>,
    pub input: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerLoopConfig {
    pub runner_id: String,
    pub poll_interval_ms: u64,
    pub lease_ttl_secs: u64,
    pub heartbeat_timeout_secs: u64,
    pub compute_class: String,
    pub memory_mb: u32,
    pub max_concurrent_leases: u32,
    pub internal_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteAttemptRequest {
    pub runner_id: String,
    pub status: AttemptStatus,
    pub failure_reason: Option<String>,
    pub failure_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppendLogRequest {
    pub stream: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterRunnerRequest {
    pub runner_id: String,
    pub compute_class: String,
    pub memory_mb: u32,
    pub max_concurrent_leases: u32,
    pub heartbeat_timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerHeartbeatRequest {
    pub runner_id: String,
    pub compute_class: String,
    pub memory_mb: u32,
    pub max_concurrent_leases: u32,
    pub heartbeat_timeout_secs: u64,
    pub lease_ttl_secs: u64,
    pub active_lease_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseStateResponse {
    pub lease_id: String,
    pub run_id: String,
    pub attempt_id: String,
    pub cancel_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseRunRequest {
    pub runner_id: String,
    pub lease_ttl_secs: u64,
}
