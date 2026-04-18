pub mod execution;
pub mod runner_loop;

use gum_types::{AttemptStatus, DeployMetadata, RunEnvelope};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerConfig {
    pub runner_id: String,
    pub work_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub deploy: DeployMetadata,
    pub run: RunEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptOutcome {
    pub attempt_id: String,
    pub status: AttemptStatus,
    pub failure_reason: Option<String>,
    pub failure_class: Option<String>,
}

pub const EXECUTION_FLOW: &[&str] = &[
    "poll for a lease",
    "download the deploy bundle",
    "resolve the job handler",
    "execute the attempt",
    "stream logs",
    "enforce timeout",
    "report completion",
];
