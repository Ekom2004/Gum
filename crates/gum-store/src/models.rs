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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseRecord {
    pub id: String,
    pub attempt_id: String,
    pub runner_id: String,
    pub expires_at_epoch_ms: i64,
    pub acked_at_epoch_ms: Option<i64>,
    pub released_at_epoch_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogRecord {
    pub id: String,
    pub run_id: String,
    pub attempt_id: String,
    pub stream: String,
    pub message: String,
}
