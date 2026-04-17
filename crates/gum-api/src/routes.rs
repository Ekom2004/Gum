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
pub struct RunResponse {
    pub id: String,
    pub job_id: String,
    pub status: RunStatus,
    pub attempt: u32,
    pub failure_reason: Option<String>,
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
    pub deploy_id: String,
    pub input: Value,
    pub bundle_url: String,
    pub entrypoint: String,
    pub handler_ref: String,
    pub timeout_secs: u32,
    pub lease_ttl_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterRunnerRequest {
    pub runner_id: String,
    pub compute_class: String,
    pub max_concurrent_leases: u32,
    pub heartbeat_timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerHeartbeatRequest {
    pub runner_id: String,
    pub compute_class: String,
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
pub struct CompleteAttemptRequest {
    pub runner_id: String,
    pub status: AttemptStatus,
    pub failure_reason: Option<String>,
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
