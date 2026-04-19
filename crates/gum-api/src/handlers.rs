use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;

use crate::app_state::AppState;
use crate::routes::{
    AppendLogRequest, CancelRunRequest, CompleteAttemptRequest, EnqueueRunRequest, LeaseRunRequest,
    RegisterDeployRequest, RegisterRunnerRequest, RunnerHeartbeatRequest,
};
use crate::service;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/deploys", post(register_deploy))
        .route("/v1/jobs/:job_id/runs", post(enqueue_run))
        .route("/v1/runs/:run_id", get(get_run))
        .route("/v1/runs/:run_id/cancel", post(cancel_run))
        .route("/v1/runs/:run_id/replay", post(replay_run))
        .route("/v1/runs/:run_id/logs", get(get_logs))
        .route("/internal/admin/runs", get(list_runs))
        .route("/internal/admin/runners", get(list_runners))
        .route("/internal/admin/leases", get(list_leases))
        .route("/internal/admin/concurrency", get(list_concurrency))
        .route("/internal/admin/providers", get(list_provider_health))
        .route("/internal/runners/register", post(register_runner))
        .route("/internal/runners/heartbeat", post(heartbeat_runner))
        .route("/internal/leases/:lease_id", get(get_lease_state))
        .route("/internal/runs/lease", post(lease_run))
        .route(
            "/internal/runs/:run_id/attempts/:attempt_id/logs",
            post(append_log),
        )
        .route(
            "/internal/attempts/:attempt_id/complete",
            post(complete_attempt),
        )
        .with_state(state)
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn from_message(message: String) -> Self {
        let status = if message.contains("not found") {
            StatusCode::NOT_FOUND
        } else if message.contains("mismatch") || message.contains("disabled") {
            StatusCode::CONFLICT
        } else {
            StatusCode::BAD_REQUEST
        };

        Self { status, message }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }

    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                error: self.message,
            }),
        )
            .into_response()
    }
}

pub async fn register_deploy(
    State(state): State<AppState>,
    Json(payload): Json<RegisterDeployRequest>,
) -> Result<Json<crate::routes::RegisterDeployResponse>, ApiError> {
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || service::register_deploy(&store, payload))
        .await
        .map_err(|error| ApiError::internal(format!("register deploy task failed: {error}")))?;
    result.map(Json).map_err(ApiError::from_message)
}

pub async fn enqueue_run(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
    Json(payload): Json<EnqueueRunRequest>,
) -> Result<Json<crate::routes::EnqueueRunResponse>, ApiError> {
    let store = state.store.clone();
    let project_id = state.project_id.clone();
    let result = tokio::task::spawn_blocking(move || {
        service::enqueue_run(&store, &project_id, &job_id, payload)
    })
    .await
    .map_err(|error| ApiError::internal(format!("enqueue task failed: {error}")))?;
    result.map(Json).map_err(ApiError::from_message)
}

pub async fn get_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<crate::routes::RunResponse>, ApiError> {
    let store = state.store.clone();
    let maybe_run = tokio::task::spawn_blocking(move || service::get_run(&store, &run_id))
        .await
        .map_err(|error| ApiError::internal(format!("get run task failed: {error}")))?
        .map_err(ApiError::from_message)?;
    match maybe_run {
        Some(run) => Ok(Json(run)),
        None => Err(ApiError {
            status: StatusCode::NOT_FOUND,
            message: "run not found".to_string(),
        }),
    }
}

pub async fn replay_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<crate::routes::ReplayRunResponse>, ApiError> {
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || service::replay_run(&store, &run_id))
        .await
        .map_err(|error| ApiError::internal(format!("replay task failed: {error}")))?;
    result.map(Json).map_err(ApiError::from_message)
}

pub async fn cancel_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(payload): Json<CancelRunRequest>,
) -> Result<Json<crate::routes::RunResponse>, ApiError> {
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || service::cancel_run(&store, &run_id, payload))
        .await
        .map_err(|error| ApiError::internal(format!("cancel run task failed: {error}")))?;
    result.map(Json).map_err(ApiError::from_message)
}

pub async fn get_logs(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<Vec<crate::routes::LogLine>>, ApiError> {
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || service::get_logs(&store, &run_id))
        .await
        .map_err(|error| ApiError::internal(format!("get logs task failed: {error}")))?;
    result.map(Json).map_err(ApiError::from_message)
}

pub async fn list_runs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<crate::routes::RunsListResponse>, ApiError> {
    require_admin(&state.admin_key, &headers)?;
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || service::list_runs(&store, 50))
        .await
        .map_err(|error| ApiError::internal(format!("list runs task failed: {error}")))?;
    result.map(Json).map_err(ApiError::from_message)
}

pub async fn list_runners(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<crate::routes::RunnersListResponse>, ApiError> {
    require_admin(&state.admin_key, &headers)?;
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || service::list_runners(&store))
        .await
        .map_err(|error| ApiError::internal(format!("list runners task failed: {error}")))?;
    result.map(Json).map_err(ApiError::from_message)
}

pub async fn list_leases(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<crate::routes::LeasesListResponse>, ApiError> {
    require_admin(&state.admin_key, &headers)?;
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || service::list_leases(&store))
        .await
        .map_err(|error| ApiError::internal(format!("list leases task failed: {error}")))?;
    result.map(Json).map_err(ApiError::from_message)
}

pub async fn list_concurrency(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<crate::routes::ConcurrencyListResponse>, ApiError> {
    require_admin(&state.admin_key, &headers)?;
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || service::list_concurrency(&store))
        .await
        .map_err(|error| ApiError::internal(format!("list concurrency task failed: {error}")))?;
    result.map(Json).map_err(ApiError::from_message)
}

pub async fn list_provider_health(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<crate::routes::ProviderHealthListResponse>, ApiError> {
    require_admin(&state.admin_key, &headers)?;
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || service::list_provider_health(&store))
        .await
        .map_err(|error| {
            ApiError::internal(format!("list provider health task failed: {error}"))
        })?;
    result.map(Json).map_err(ApiError::from_message)
}

pub async fn lease_run(
    State(state): State<AppState>,
    Json(payload): Json<LeaseRunRequest>,
) -> Result<Response, ApiError> {
    let store = state.store.clone();
    let leased = tokio::task::spawn_blocking(move || service::lease_run(&store, payload))
        .await
        .map_err(|error| ApiError::internal(format!("lease task failed: {error}")))?
        .map_err(ApiError::from_message)?;
    match leased {
        Some(run) => Ok(Json(run).into_response()),
        None => Ok(StatusCode::NO_CONTENT.into_response()),
    }
}

pub async fn register_runner(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRunnerRequest>,
) -> Result<StatusCode, ApiError> {
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || service::register_runner(&store, payload))
        .await
        .map_err(|error| ApiError::internal(format!("register runner task failed: {error}")))?;
    result
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(ApiError::from_message)
}

pub async fn heartbeat_runner(
    State(state): State<AppState>,
    Json(payload): Json<RunnerHeartbeatRequest>,
) -> Result<StatusCode, ApiError> {
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || service::heartbeat_runner(&store, payload))
        .await
        .map_err(|error| ApiError::internal(format!("runner heartbeat task failed: {error}")))?;
    result
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(ApiError::from_message)
}

pub async fn get_lease_state(
    State(state): State<AppState>,
    Path(lease_id): Path<String>,
) -> Result<Json<crate::routes::LeaseStateResponse>, ApiError> {
    let store = state.store.clone();
    let maybe_state =
        tokio::task::spawn_blocking(move || service::get_lease_state(&store, &lease_id))
            .await
            .map_err(|error| ApiError::internal(format!("get lease state task failed: {error}")))?
            .map_err(ApiError::from_message)?;
    match maybe_state {
        Some(state) => Ok(Json(state)),
        None => Err(ApiError {
            status: StatusCode::NOT_FOUND,
            message: "lease not found".to_string(),
        }),
    }
}

pub async fn complete_attempt(
    State(state): State<AppState>,
    Path(attempt_id): Path<String>,
    Json(payload): Json<CompleteAttemptRequest>,
) -> Result<Json<crate::routes::RunResponse>, ApiError> {
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || {
        service::complete_attempt(&store, &attempt_id, payload)
    })
    .await
    .map_err(|error| ApiError::internal(format!("complete attempt task failed: {error}")))?;
    result.map(Json).map_err(ApiError::from_message)
}

fn require_admin(admin_key: &str, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(value) = headers.get(axum::http::header::AUTHORIZATION) else {
        return Err(ApiError::unauthorized("missing admin authorization"));
    };
    let Ok(value) = value.to_str() else {
        return Err(ApiError::unauthorized("invalid admin authorization"));
    };
    let Some(token) = value.strip_prefix("Bearer ") else {
        return Err(ApiError::unauthorized("invalid admin authorization"));
    };
    if token != admin_key {
        return Err(ApiError::unauthorized("invalid admin authorization"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::require_admin;
    use axum::http::{header, HeaderMap, HeaderValue};

    #[test]
    fn admin_auth_rejects_missing_header() {
        let headers = HeaderMap::new();
        let error =
            require_admin("admin-secret", &headers).expect_err("missing header should fail");
        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn admin_auth_rejects_wrong_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer wrong-token"),
        );
        let error = require_admin("admin-secret", &headers).expect_err("wrong token should fail");
        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn admin_auth_accepts_matching_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer admin-secret"),
        );
        require_admin("admin-secret", &headers).expect("matching token should pass");
    }
}

pub async fn append_log(
    State(state): State<AppState>,
    Path((run_id, attempt_id)): Path<(String, String)>,
    Json(payload): Json<AppendLogRequest>,
) -> Result<StatusCode, ApiError> {
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || {
        service::append_log(&store, &run_id, &attempt_id, payload)
    })
    .await
    .map_err(|error| ApiError::internal(format!("append log task failed: {error}")))?;
    result
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(ApiError::from_message)
}

pub fn startup_error(message: impl Into<String>) -> ApiError {
    ApiError::internal(message)
}
