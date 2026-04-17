use gum_types::RunEnvelope;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseRequest {
    pub runner_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseGrant {
    pub lease_id: String,
    pub runner_id: String,
    pub run: RunEnvelope,
    pub lease_ttl_secs: u64,
}

pub const QUEUE_RULES: &[&str] = &[
    "only queued runs are eligible",
    "concurrency is enforced before a lease is granted",
    "per-job rate limits are enforced before a lease is granted",
    "expired leases must make work recoverable",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EligibilityDecision {
    Eligible,
    BlockedByConcurrency,
    BlockedByRateLimit,
    BlockedByLease,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuePolicyResult {
    pub run_id: String,
    pub decision: EligibilityDecision,
}
