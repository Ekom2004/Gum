pub mod app_state;
pub mod handlers;
pub mod routes;
pub mod service;

pub const EXTERNAL_ROUTES: &[&str] = &[
    "POST /v1/deploys",
    "POST /v1/jobs/{job_id}/runs",
    "GET /v1/runs/{run_id}",
    "POST /v1/runs/{run_id}/replay",
    "GET /v1/runs/{run_id}/logs",
];

pub const INTERNAL_ROUTES: &[&str] = &[
    "POST /internal/runs/lease",
    "POST /internal/attempts/{attempt_id}/complete",
];
