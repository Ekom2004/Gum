use serde::{Deserialize, Serialize};
use serde_json::Value;
use gum_types::AttemptStatus;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeasedRun {
    pub lease_id: String,
    pub attempt_id: String,
    pub run_id: String,
    pub job_id: String,
    pub deploy_id: String,
    pub bundle_url: String,
    pub entrypoint: String,
    pub handler_ref: String,
    pub timeout_secs: u32,
    pub input: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerLoopConfig {
    pub runner_id: String,
    pub poll_interval_ms: u64,
    pub lease_ttl_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseRunRequest {
    pub runner_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteAttemptRequest {
    pub runner_id: String,
    pub status: AttemptStatus,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppendLogRequest {
    pub stream: String,
    pub message: String,
}
