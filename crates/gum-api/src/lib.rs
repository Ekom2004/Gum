pub mod app_state;
pub mod handlers;
pub mod routes;
pub mod secret_store;
pub mod service;

pub const EXTERNAL_ROUTES: &[&str] = &[
    "POST /v1/deploys",
    "POST /v1/jobs/{job_id}/runs",
    "GET /v1/runs/{run_id}",
    "POST /v1/runs/{run_id}/replay",
    "GET /v1/runs/{run_id}/logs",
    "POST /v1/secrets",
    "GET /v1/secrets",
    "DELETE /v1/secrets/{name}",
];

pub const INTERNAL_ROUTES: &[&str] = &[
    "POST /internal/runs/lease",
    "POST /internal/attempts/{attempt_id}/complete",
];
