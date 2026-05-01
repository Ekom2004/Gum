use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerType {
    Enqueue,
    Schedule,
    Replay,
    Backfill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    TimedOut,
    Canceled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttemptStatus {
    Queued,
    Leased,
    Running,
    Succeeded,
    Failed,
    TimedOut,
    Canceled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeployStatus {
    Registering,
    Warming,
    WarmupFailed,
    Ready,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct JobPolicy {
    pub every: Option<String>,
    pub retries: u32,
    pub timeout_secs: u32,
    pub rate_limit_spec: Option<String>,
    pub concurrency_limit: Option<u32>,
    pub cpu_cores: Option<u32>,
    pub memory_mb: Option<u32>,
    pub key_field: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployMetadata {
    pub deploy_id: String,
    pub bundle_url: String,
    pub bundle_sha256: String,
    pub entrypoint: String,
    pub sdk_language: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunEnvelope {
    pub run_id: String,
    pub job_id: String,
    pub deploy_id: String,
    pub trigger: TriggerType,
    pub input_json: String,
    pub attempt_count: u32,
    pub max_attempts: u32,
}
