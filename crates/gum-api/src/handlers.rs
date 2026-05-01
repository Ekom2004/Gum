use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::app_state::AppState;
use crate::routes::{
    AppendLogRequest, CancelRunRequest, CompleteAttemptRequest, CompleteRuntimePrepareRequest,
    EnqueueRunRequest, LeaseRunRequest, LeaseRuntimePrepareRequest, RegisterDeployRequest,
    RegisterRunnerRequest, RunnerHeartbeatRequest, SecretMetadataResponse, SecretsListResponse,
    SetSecretRequest,
};
use crate::secret_store::{ResolveSecretParams, SetSecretParams};
use crate::service;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/deploys", post(register_deploy))
        .route(
            "/v1/deploys/:deploy_id/prepare-runtime",
            post(prepare_runtime),
        )
        .route("/v1/jobs/:job_id/runs", post(enqueue_run))
        .route("/v1/runs/:run_id", get(get_run))
        .route("/v1/runs/:run_id/cancel", post(cancel_run))
        .route("/v1/runs/:run_id/replay", post(replay_run))
        .route("/v1/runs/:run_id/logs", get(get_logs))
        .route("/v1/secrets", post(set_secret).get(list_secrets))
        .route("/v1/secrets/:name", axum::routing::delete(delete_secret))
        .route("/internal/admin/runs", get(list_runs))
        .route("/internal/admin/runners", get(list_runners))
        .route("/internal/admin/leases", get(list_leases))
        .route("/internal/admin/concurrency", get(list_concurrency))
        .route("/internal/admin/rate-limits", get(list_rate_limits))
        .route("/internal/admin/providers", get(list_provider_health))
        .route("/internal/runners/register", post(register_runner))
        .route("/internal/runners/heartbeat", post(heartbeat_runner))
        .route("/internal/leases/:lease_id", get(get_lease_state))
        .route("/internal/runs/lease", post(lease_run))
        .route(
            "/internal/runtime-prepares/lease",
            post(lease_runtime_prepare),
        )
        .route(
            "/internal/runtime-prepares/:deploy_id/complete",
            post(complete_runtime_prepare),
        )
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

#[derive(Debug, Clone, Deserialize)]
pub struct EnqueueRunPayload {
    input: Value,
    #[serde(default)]
    delay: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecretsQuery {
    #[serde(default)]
    pub environment: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeleteSecretQuery {
    #[serde(default)]
    pub environment: Option<String>,
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
    headers: HeaderMap,
    Json(payload): Json<RegisterDeployRequest>,
) -> Result<Json<crate::routes::RegisterDeployResponse>, ApiError> {
    require_api_or_admin(&state.api_key, &state.admin_key, &headers)?;
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || service::register_deploy(&store, payload))
        .await
        .map_err(|error| ApiError::internal(format!("register deploy task failed: {error}")))?;
    result.map(Json).map_err(ApiError::from_message)
}

pub async fn enqueue_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(job_id): Path<String>,
    Json(payload): Json<EnqueueRunPayload>,
) -> Result<Json<crate::routes::EnqueueRunResponse>, ApiError> {
    require_api_or_admin(&state.api_key, &state.admin_key, &headers)?;
    let store = state.store.clone();
    let project_id = state.project_id.clone();
    let request = EnqueueRunRequest {
        input: payload.input,
    };
    let result = tokio::task::spawn_blocking(move || {
        service::enqueue_run_with_delay(
            &store,
            &project_id,
            &job_id,
            request,
            payload.delay.as_deref(),
        )
    })
    .await
    .map_err(|error| ApiError::internal(format!("enqueue task failed: {error}")))?;
    result.map(Json).map_err(ApiError::from_message)
}

pub async fn prepare_runtime(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(deploy_id): Path<String>,
) -> Result<Json<crate::routes::PrepareDeployRuntimeResponse>, ApiError> {
    require_api_or_admin(&state.api_key, &state.admin_key, &headers)?;
    let store = state.store.clone();
    let result =
        tokio::task::spawn_blocking(move || service::request_runtime_prepare(&store, &deploy_id))
            .await
            .map_err(|error| {
                ApiError::internal(format!("request runtime prepare task failed: {error}"))
            })?;
    result.map(Json).map_err(ApiError::from_message)
}

pub async fn get_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(run_id): Path<String>,
) -> Result<Json<crate::routes::RunResponse>, ApiError> {
    require_api_or_admin(&state.api_key, &state.admin_key, &headers)?;
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
    headers: HeaderMap,
    Path(run_id): Path<String>,
) -> Result<Json<crate::routes::ReplayRunResponse>, ApiError> {
    require_api_or_admin(&state.api_key, &state.admin_key, &headers)?;
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || service::replay_run(&store, &run_id))
        .await
        .map_err(|error| ApiError::internal(format!("replay task failed: {error}")))?;
    result.map(Json).map_err(ApiError::from_message)
}

pub async fn cancel_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(run_id): Path<String>,
    Json(payload): Json<CancelRunRequest>,
) -> Result<Json<crate::routes::RunResponse>, ApiError> {
    require_api_or_admin(&state.api_key, &state.admin_key, &headers)?;
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || service::cancel_run(&store, &run_id, payload))
        .await
        .map_err(|error| ApiError::internal(format!("cancel run task failed: {error}")))?;
    result.map(Json).map_err(ApiError::from_message)
}

pub async fn get_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(run_id): Path<String>,
) -> Result<Json<Vec<crate::routes::LogLine>>, ApiError> {
    require_api_or_admin(&state.api_key, &state.admin_key, &headers)?;
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || service::get_logs(&store, &run_id))
        .await
        .map_err(|error| ApiError::internal(format!("get logs task failed: {error}")))?;
    result.map(Json).map_err(ApiError::from_message)
}

pub async fn set_secret(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<SetSecretRequest>,
) -> Result<Json<SecretMetadataResponse>, ApiError> {
    require_api_or_admin(&state.api_key, &state.admin_key, &headers)?;
    let environment = normalize_environment(payload.environment.as_deref());
    if payload.name.trim().is_empty() {
        return Err(ApiError::from_message(
            "secret name cannot be empty".to_string(),
        ));
    }
    if payload.value.is_empty() {
        return Err(ApiError::from_message(
            "secret value cannot be empty".to_string(),
        ));
    }
    let secrets = state.secrets.clone();
    let project_id = state.project_id.clone();
    let name = payload.name.trim().to_string();
    let value = payload.value;
    let metadata = tokio::task::spawn_blocking(move || {
        secrets.set_secret(SetSecretParams {
            project_id,
            environment,
            name,
            value,
        })
    })
    .await
    .map_err(|error| ApiError::internal(format!("set secret task failed: {error}")))?
    .map_err(ApiError::from_message)?;
    Ok(Json(secret_metadata_response(metadata)))
}

pub async fn list_secrets(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<SecretsQuery>,
) -> Result<Json<SecretsListResponse>, ApiError> {
    require_api_or_admin(&state.api_key, &state.admin_key, &headers)?;
    let environment = normalize_environment(query.environment.as_deref());
    let secret_store = state.secrets.clone();
    let project_id = state.project_id.clone();
    let secrets =
        tokio::task::spawn_blocking(move || secret_store.list_secrets(&project_id, &environment))
            .await
            .map_err(|error| ApiError::internal(format!("list secrets task failed: {error}")))?
            .map_err(ApiError::from_message)?
            .into_iter()
            .map(secret_metadata_response)
            .collect();
    Ok(Json(SecretsListResponse { secrets }))
}

pub async fn delete_secret(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
    axum::extract::Query(query): axum::extract::Query<DeleteSecretQuery>,
) -> Result<StatusCode, ApiError> {
    require_api_or_admin(&state.api_key, &state.admin_key, &headers)?;
    if name.trim().is_empty() {
        return Err(ApiError::from_message(
            "secret name cannot be empty".to_string(),
        ));
    }
    let environment = normalize_environment(query.environment.as_deref());
    let secret_store = state.secrets.clone();
    let project_id = state.project_id.clone();
    let name = name.trim().to_string();
    let deleted = tokio::task::spawn_blocking(move || {
        secret_store.delete_secret(&project_id, &environment, &name)
    })
    .await
    .map_err(|error| ApiError::internal(format!("delete secret task failed: {error}")))?
    .map_err(ApiError::from_message)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError {
            status: StatusCode::NOT_FOUND,
            message: "secret not found".to_string(),
        })
    }
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

pub async fn list_rate_limits(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<crate::routes::RateLimitListResponse>, ApiError> {
    require_admin(&state.admin_key, &headers)?;
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || service::list_rate_limits(&store))
        .await
        .map_err(|error| ApiError::internal(format!("list rate limits task failed: {error}")))?;
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
    headers: HeaderMap,
    Json(payload): Json<LeaseRunRequest>,
) -> Result<Response, ApiError> {
    require_internal(&state.internal_key, &headers)?;
    let store = state.store.clone();
    let leased = tokio::task::spawn_blocking(move || service::lease_run(&store, payload))
        .await
        .map_err(|error| ApiError::internal(format!("lease task failed: {error}")))?
        .map_err(ApiError::from_message)?;
    match leased {
        Some(mut run) => {
            let environment = secret_environment();
            let secrets = state.secrets.clone();
            let project_id = state.project_id.clone();
            let required_secret_names = run.required_secret_names.clone();
            let resolved = tokio::task::spawn_blocking(move || {
                let mut resolved = HashMap::new();
                for secret_name in required_secret_names {
                    let value = secrets.resolve_secret(ResolveSecretParams {
                        project_id: project_id.clone(),
                        environment: environment.clone(),
                        name: secret_name.clone(),
                    })?;
                    if let Some(value) = value {
                        resolved.insert(secret_name, value);
                    }
                }
                Ok::<HashMap<String, String>, String>(resolved)
            })
            .await
            .map_err(|error| ApiError::internal(format!("resolve secrets task failed: {error}")))?
            .map_err(ApiError::from_message)?;
            run.resolved_secrets = resolved;
            Ok(Json(run).into_response())
        }
        None => Ok(StatusCode::NO_CONTENT.into_response()),
    }
}

pub async fn lease_runtime_prepare(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LeaseRuntimePrepareRequest>,
) -> Result<Response, ApiError> {
    require_internal(&state.internal_key, &headers)?;
    let store = state.store.clone();
    let leased =
        tokio::task::spawn_blocking(move || service::lease_runtime_prepare(&store, payload))
            .await
            .map_err(|error| {
                ApiError::internal(format!("lease runtime prepare task failed: {error}"))
            })?
            .map_err(ApiError::from_message)?;
    match leased {
        Some(prepare) => Ok(Json(prepare).into_response()),
        None => Ok(StatusCode::NO_CONTENT.into_response()),
    }
}

pub async fn register_runner(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<RegisterRunnerRequest>,
) -> Result<StatusCode, ApiError> {
    require_internal(&state.internal_key, &headers)?;
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
    headers: HeaderMap,
    Json(payload): Json<RunnerHeartbeatRequest>,
) -> Result<StatusCode, ApiError> {
    require_internal(&state.internal_key, &headers)?;
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
    headers: HeaderMap,
    Path(lease_id): Path<String>,
) -> Result<Json<crate::routes::LeaseStateResponse>, ApiError> {
    require_internal(&state.internal_key, &headers)?;
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

pub async fn complete_runtime_prepare(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(deploy_id): Path<String>,
    Json(payload): Json<CompleteRuntimePrepareRequest>,
) -> Result<Json<crate::routes::PrepareDeployRuntimeResponse>, ApiError> {
    require_internal(&state.internal_key, &headers)?;
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _runner_id = payload.runner_id;
        service::complete_runtime_prepare(&store, &deploy_id, payload.success)
    })
    .await
    .map_err(|error| {
        ApiError::internal(format!("complete runtime prepare task failed: {error}"))
    })?;
    result.map(Json).map_err(ApiError::from_message)
}

pub async fn complete_attempt(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(attempt_id): Path<String>,
    Json(payload): Json<CompleteAttemptRequest>,
) -> Result<Json<crate::routes::RunResponse>, ApiError> {
    require_internal(&state.internal_key, &headers)?;
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || {
        service::complete_attempt(&store, &attempt_id, payload)
    })
    .await
    .map_err(|error| ApiError::internal(format!("complete attempt task failed: {error}")))?;
    result.map(Json).map_err(ApiError::from_message)
}

fn require_admin(admin_key: &str, headers: &HeaderMap) -> Result<(), ApiError> {
    require_single_bearer(
        admin_key,
        headers,
        "admin auth misconfigured",
        "missing admin authorization",
        "invalid admin authorization",
    )
}

fn normalize_environment(environment: Option<&str>) -> String {
    let normalized = environment.unwrap_or("prod").trim();
    if normalized.is_empty() {
        "prod".to_string()
    } else {
        normalized.to_string()
    }
}

fn secret_environment() -> String {
    normalize_environment(std::env::var("GUM_SECRET_ENV").ok().as_deref())
}

fn secret_metadata_response(
    metadata: crate::secret_store::SecretMetadata,
) -> SecretMetadataResponse {
    SecretMetadataResponse {
        project_id: metadata.project_id,
        environment: metadata.environment,
        name: metadata.name,
        backend: metadata.backend,
        updated_at_epoch_ms: metadata.updated_at_epoch_ms,
        last_used_at_epoch_ms: metadata.last_used_at_epoch_ms,
    }
}

fn require_api_or_admin(
    api_key: &str,
    admin_key: &str,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    if api_key.trim().is_empty() && admin_key.trim().is_empty() {
        return Err(ApiError::internal("api auth misconfigured"));
    }
    let token = extract_bearer(
        headers,
        "missing api authorization",
        "invalid api authorization",
    )?;
    if (!api_key.trim().is_empty() && constant_time_eq(token, api_key))
        || (!admin_key.trim().is_empty() && constant_time_eq(token, admin_key))
    {
        return Ok(());
    }
    Err(ApiError::unauthorized("invalid api authorization"))
}

fn require_internal(internal_key: &str, headers: &HeaderMap) -> Result<(), ApiError> {
    require_single_bearer(
        internal_key,
        headers,
        "internal auth misconfigured",
        "missing internal authorization",
        "invalid internal authorization",
    )
}

fn require_single_bearer(
    expected_token: &str,
    headers: &HeaderMap,
    misconfigured_message: &str,
    missing_message: &str,
    invalid_message: &str,
) -> Result<(), ApiError> {
    if expected_token.trim().is_empty() {
        return Err(ApiError::internal(misconfigured_message));
    }
    let token = extract_bearer(headers, missing_message, invalid_message)?;
    if !constant_time_eq(token, expected_token) {
        return Err(ApiError::unauthorized(invalid_message));
    }
    Ok(())
}

fn extract_bearer<'a>(
    headers: &'a HeaderMap,
    missing_message: &str,
    invalid_message: &str,
) -> Result<&'a str, ApiError> {
    let Some(value) = headers.get(axum::http::header::AUTHORIZATION) else {
        return Err(ApiError::unauthorized(missing_message));
    };
    let Ok(value) = value.to_str() else {
        return Err(ApiError::unauthorized(invalid_message));
    };
    let Some(token) = value.strip_prefix("Bearer ") else {
        return Err(ApiError::unauthorized(invalid_message));
    };
    if token.is_empty() {
        return Err(ApiError::unauthorized(invalid_message));
    }
    Ok(token)
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    let max_len = left_bytes.len().max(right_bytes.len());

    let mut diff = left_bytes.len() ^ right_bytes.len();
    for index in 0..max_len {
        let left_byte = left_bytes.get(index).copied().unwrap_or(0);
        let right_byte = right_bytes.get(index).copied().unwrap_or(0);
        diff |= usize::from(left_byte ^ right_byte);
    }

    diff == 0
}

#[cfg(test)]
mod tests {
    use super::{constant_time_eq, require_admin, require_api_or_admin, require_internal};
    use axum::http::{header, HeaderMap, HeaderValue, StatusCode};

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

    #[test]
    fn admin_auth_rejects_empty_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer "));
        let error =
            require_admin("admin-secret", &headers).expect_err("empty bearer token should fail");
        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn admin_auth_rejects_non_bearer_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Token admin-secret"),
        );
        let error =
            require_admin("admin-secret", &headers).expect_err("non-bearer token should fail");
        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn admin_auth_rejects_empty_admin_key_configuration() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer admin-secret"),
        );
        let error =
            require_admin("", &headers).expect_err("empty admin key should be misconfigured");
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn api_auth_accepts_api_key() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer api-secret"),
        );
        require_api_or_admin("api-secret", "admin-secret", &headers)
            .expect("matching api key should pass");
    }

    #[test]
    fn api_auth_accepts_admin_key() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer admin-secret"),
        );
        require_api_or_admin("api-secret", "admin-secret", &headers)
            .expect("matching admin key should pass api auth");
    }

    #[test]
    fn api_auth_rejects_invalid_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer wrong"),
        );
        let error = require_api_or_admin("api-secret", "admin-secret", &headers)
            .expect_err("invalid token should fail api auth");
        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn internal_auth_rejects_missing_header() {
        let headers = HeaderMap::new();
        let error = require_internal("internal-secret", &headers)
            .expect_err("missing header should fail internal auth");
        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn internal_auth_accepts_matching_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer internal-secret"),
        );
        require_internal("internal-secret", &headers)
            .expect("matching token should pass internal auth");
    }

    #[test]
    fn constant_time_compare_matches_expected_results() {
        assert!(constant_time_eq("admin-secret", "admin-secret"));
        assert!(!constant_time_eq("admin-secret", "admin-secrex"));
        assert!(!constant_time_eq("admin-secret", "admin-secret-extra"));
        assert!(!constant_time_eq("admin-secret", ""));
    }
}

pub async fn append_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((run_id, attempt_id)): Path<(String, String)>,
    Json(payload): Json<AppendLogRequest>,
) -> Result<StatusCode, ApiError> {
    require_internal(&state.internal_key, &headers)?;
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
