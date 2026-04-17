pub mod memory;
pub mod models;
pub mod pg;
pub mod queries;

use serde::{Deserialize, Serialize};

pub const TABLES: &[&str] = &[
    "projects",
    "deploys",
    "jobs",
    "runs",
    "attempts",
    "leases",
    "runners",
    "control_leases",
    "logs",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaPlan {
    pub tables: Vec<String>,
}

impl Default for SchemaPlan {
    fn default() -> Self {
        Self {
            tables: TABLES.iter().map(|table| table.to_string()).collect(),
        }
    }
}
