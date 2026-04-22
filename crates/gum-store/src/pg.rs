use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use gum_types::{AttemptStatus, DeployStatus, RunStatus, TriggerType};
use postgres::{Client, NoTls, Row};

use crate::models::{
    AttemptRecord, ConcurrencyStatusRecord, DeployRecord, FunctionHealthRecord,
    FunctionHealthState, JobRecord, LeaseRecord, LeaseStateRecord, LeaseStatusRecord, LogRecord,
    ProjectRecord, ProviderCheckRecord, ProviderCheckStatus, ProviderHealthRecord,
    ProviderHealthState, ProviderTargetRecord, RateLimitStatusRecord, RunRecord, RunnerRecord,
    RunnerStatusRecord,
};
use crate::queries::{
    compute_retry_disposition, function_health_hold_delay_ms, is_infrastructure_failure_class,
    is_provider_failure_class, key_retention_ms, parse_rate_limit_spec, parse_schedule_interval_ms,
    provider_slug_from_job, rate_limit_scope_key, CancelRunParams, CompleteAttemptParams,
    ControlLeaseParams, EnqueueRunParams, EnqueueRunResult, GumStore, HeartbeatRunnerParams,
    LeaseNextAttemptParams, RecordProviderCheckParams, RegisterDeployParams, RegisterRunnerParams,
    ReplayRunParams, SetFunctionHealthParams, SetProviderHealthParams, UpsertProviderTargetParams,
};

const MIGRATION_0001: &str = include_str!("../migrations/0001_slice1.sql");
const MIGRATION_0002: &str = include_str!("../migrations/0002_scheduler.sql");
const MIGRATION_0003: &str = include_str!("../migrations/0003_runner_liveness.sql");
const MIGRATION_0004: &str =
    include_str!("../migrations/0004_control_leases_and_compute_classes.sql");
const MIGRATION_0005: &str = include_str!("../migrations/0005_cancel_revoke.sql");
const MIGRATION_0006: &str = include_str!("../migrations/0006_provider_health.sql");
const MIGRATION_0007: &str = include_str!("../migrations/0007_retry_policy.sql");
const MIGRATION_0008: &str = include_str!("../migrations/0008_function_health.sql");
const MIGRATION_0009: &str = include_str!("../migrations/0009_run_keys.sql");
const MIGRATION_0010: &str = include_str!("../migrations/0010_memory_resources.sql");

#[derive(Clone)]
pub struct PostgresStore {
    database_url: Arc<String>,
    ids: Arc<AtomicU64>,
}

impl PostgresStore {
    pub fn connect(database_url: &str) -> Result<Self, String> {
        let _ = Client::connect(database_url, NoTls)
            .map_err(|error| format!("failed to connect to postgres: {error}"))?;

        Ok(Self {
            database_url: Arc::new(database_url.to_string()),
            ids: Arc::new(AtomicU64::new(0)),
        })
    }

    pub fn prepare_dev_database(&self, project: &ProjectRecord) -> Result<(), String> {
        let mut client = self.connect_client()?;
        client
            .batch_execute(MIGRATION_0001)
            .map_err(|error| format!("failed to apply migrations: {error}"))?;
        client
            .batch_execute(MIGRATION_0002)
            .map_err(|error| format!("failed to apply migrations: {error}"))?;
        client
            .batch_execute(MIGRATION_0003)
            .map_err(|error| format!("failed to apply migrations: {error}"))?;
        client
            .batch_execute(MIGRATION_0004)
            .map_err(|error| format!("failed to apply migrations: {error}"))?;
        client
            .batch_execute(MIGRATION_0005)
            .map_err(|error| format!("failed to apply migrations: {error}"))?;
        client
            .batch_execute(MIGRATION_0006)
            .map_err(|error| format!("failed to apply migrations: {error}"))?;
        client
            .batch_execute(MIGRATION_0007)
            .map_err(|error| format!("failed to apply migrations: {error}"))?;
        client
            .batch_execute(MIGRATION_0008)
            .map_err(|error| format!("failed to apply migrations: {error}"))?;
        client
            .batch_execute(MIGRATION_0009)
            .map_err(|error| format!("failed to apply migrations: {error}"))?;
        client
            .batch_execute(MIGRATION_0010)
            .map_err(|error| format!("failed to apply migrations: {error}"))?;
        client
            .execute(
                "INSERT INTO projects (id, name, slug, api_key_hash)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (id) DO UPDATE
                 SET name = EXCLUDED.name,
                     slug = EXCLUDED.slug,
                     api_key_hash = EXCLUDED.api_key_hash,
                     updated_at = NOW()",
                &[
                    &project.id,
                    &project.name,
                    &project.slug,
                    &project.api_key_hash,
                ],
            )
            .map_err(|error| format!("failed to seed dev project: {error}"))?;
        Ok(())
    }

    fn connect_client(&self) -> Result<Client, String> {
        Client::connect(self.database_url.as_str(), NoTls)
            .map_err(|error| format!("failed to connect to postgres: {error}"))
    }

    fn next_id(&self, prefix: &str) -> String {
        let counter = self.ids.fetch_add(1, Ordering::Relaxed) + 1;
        format!("{prefix}_{}_{}", now_epoch_ms(), counter)
    }

    fn run_key_lock_id(project_id: &str, job_id: &str, key_value: &str) -> i64 {
        let mut hash = 1469598103934665603_u64;
        for byte in project_id.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(1099511628211);
        }
        for byte in job_id.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(1099511628211);
        }
        for byte in key_value.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(1099511628211);
        }
        (hash & 0x7fff_ffff_ffff_ffff) as i64
    }

    fn provider_health_by_slug_tx(
        tx: &mut postgres::Transaction<'_>,
        provider_slug: &str,
    ) -> Result<Option<ProviderHealthRecord>, String> {
        let row = tx
            .query_opt(
                "SELECT provider_health.provider_target_id,
                        provider_targets.name AS provider_name,
                        provider_targets.slug AS provider_slug,
                        provider_health.state,
                        provider_health.reason,
                        (EXTRACT(EPOCH FROM provider_health.last_changed_at) * 1000)::bigint AS last_changed_at_epoch_ms,
                        CASE
                            WHEN provider_health.last_success_at IS NULL THEN NULL
                            ELSE (EXTRACT(EPOCH FROM provider_health.last_success_at) * 1000)::bigint
                        END AS last_success_at_epoch_ms,
                        CASE
                            WHEN provider_health.last_failure_at IS NULL THEN NULL
                            ELSE (EXTRACT(EPOCH FROM provider_health.last_failure_at) * 1000)::bigint
                        END AS last_failure_at_epoch_ms,
                        provider_health.degraded_score,
                        provider_health.down_score
                 FROM provider_health
                 JOIN provider_targets ON provider_targets.id = provider_health.provider_target_id
                 WHERE provider_targets.slug = $1",
                &[&provider_slug],
            )
            .map_err(|error| format!("failed to load provider health by slug: {error}"))?;
        row.map(provider_health_from_row).transpose()
    }

    fn apply_provider_signal_tx(
        &self,
        tx: &mut postgres::Transaction<'_>,
        provider_slug: &str,
        signal_status: ProviderCheckStatus,
        error_class: Option<&str>,
        now_epoch_ms: i64,
    ) -> Result<(), String> {
        let target_row = tx
            .query_opt(
                "SELECT *,
                        (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_at_epoch_ms
                 FROM provider_targets
                 WHERE slug = $1",
                &[&provider_slug],
            )
            .map_err(|error| format!("failed to load provider target by slug: {error}"))?;
        let Some(target_row) = target_row else {
            return Ok(());
        };
        let target = provider_target_from_row(target_row)?;
        let previous = Self::provider_health_by_slug_tx(tx, provider_slug)?;

        tx.execute(
            "INSERT INTO provider_checks (
                id, provider_target_id, status, latency_ms, error_class, status_code, checked_at
             ) VALUES ($1, $2, $3, NULL, $4, NULL, TO_TIMESTAMP($5::double precision / 1000.0))",
            &[
                &self.next_id("pcheck"),
                &target.id,
                &provider_check_status_to_str(signal_status),
                &error_class.map(str::to_string),
                &(now_epoch_ms as f64),
            ],
        )
        .map_err(|error| format!("failed to record provider request signal: {error}"))?;

        let (
            state,
            reason,
            last_success_at_epoch_ms,
            last_failure_at_epoch_ms,
            degraded_score,
            down_score,
        ) = match signal_status {
            ProviderCheckStatus::Success => (
                ProviderHealthState::Healthy,
                None,
                Some(now_epoch_ms),
                previous
                    .as_ref()
                    .and_then(|record| record.last_failure_at_epoch_ms),
                previous
                    .as_ref()
                    .map(|record| record.degraded_score.saturating_sub(2))
                    .unwrap_or(0),
                0,
            ),
            ProviderCheckStatus::Failure => {
                let next_down = (previous
                    .as_ref()
                    .map(|record| record.down_score)
                    .unwrap_or(0)
                    + 1)
                .clamp(0, 10);
                (
                    if next_down >= 3 {
                        ProviderHealthState::Down
                    } else {
                        ProviderHealthState::Degraded
                    },
                    Some(
                        error_class
                            .map(str::to_string)
                            .unwrap_or_else(|| "provider request failed".to_string()),
                    ),
                    previous
                        .as_ref()
                        .and_then(|record| record.last_success_at_epoch_ms),
                    Some(now_epoch_ms),
                    (previous
                        .as_ref()
                        .map(|record| record.degraded_score)
                        .unwrap_or(0)
                        + 1)
                    .clamp(0, 10),
                    next_down,
                )
            }
        };

        let last_changed_at_epoch_ms = if previous
            .as_ref()
            .map(|record| record.state == state)
            .unwrap_or(false)
        {
            previous
                .as_ref()
                .map(|record| record.last_changed_at_epoch_ms)
                .unwrap_or(now_epoch_ms)
        } else {
            now_epoch_ms
        };

        tx.execute(
            "INSERT INTO provider_health (
                provider_target_id, state, reason, last_changed_at, last_success_at,
                last_failure_at, degraded_score, down_score, updated_at
             ) VALUES (
                $1,
                $2,
                $3,
                TO_TIMESTAMP($4::double precision / 1000.0),
                CASE WHEN $5::double precision IS NULL THEN NULL ELSE TO_TIMESTAMP($5::double precision / 1000.0) END,
                CASE WHEN $6::double precision IS NULL THEN NULL ELSE TO_TIMESTAMP($6::double precision / 1000.0) END,
                $7,
                $8,
                NOW()
             )
             ON CONFLICT (provider_target_id) DO UPDATE
             SET state = EXCLUDED.state,
                 reason = EXCLUDED.reason,
                 last_changed_at = EXCLUDED.last_changed_at,
                 last_success_at = EXCLUDED.last_success_at,
                 last_failure_at = EXCLUDED.last_failure_at,
                 degraded_score = EXCLUDED.degraded_score,
                 down_score = EXCLUDED.down_score,
                 updated_at = NOW()",
            &[
                &target.id,
                &provider_health_state_to_str(state),
                &reason,
                &(last_changed_at_epoch_ms as f64),
                &last_success_at_epoch_ms.map(|value| value as f64),
                &last_failure_at_epoch_ms.map(|value| value as f64),
                &degraded_score,
                &down_score,
            ],
        )
        .map_err(|error| format!("failed to update provider health from request signal: {error}"))?;

        Ok(())
    }

    fn function_health_for_job_tx(
        tx: &mut postgres::Transaction<'_>,
        job_id: &str,
    ) -> Result<Option<FunctionHealthRecord>, String> {
        let row = tx
            .query_opt(
                "SELECT job_id,
                        state,
                        consecutive_infra_failures,
                        reason,
                        CASE
                            WHEN hold_until IS NULL THEN NULL
                            ELSE (EXTRACT(EPOCH FROM hold_until) * 1000)::bigint
                        END AS hold_until_epoch_ms,
                        (EXTRACT(EPOCH FROM last_changed_at) * 1000)::bigint AS last_changed_at_epoch_ms,
                        CASE
                            WHEN last_success_at IS NULL THEN NULL
                            ELSE (EXTRACT(EPOCH FROM last_success_at) * 1000)::bigint
                        END AS last_success_at_epoch_ms,
                        CASE
                            WHEN last_failure_at IS NULL THEN NULL
                            ELSE (EXTRACT(EPOCH FROM last_failure_at) * 1000)::bigint
                        END AS last_failure_at_epoch_ms
                 FROM function_health
                 WHERE job_id = $1",
                &[&job_id],
            )
            .map_err(|error| format!("failed to load function health: {error}"))?;
        row.map(function_health_from_row).transpose()
    }

    fn apply_function_signal_tx(
        tx: &mut postgres::Transaction<'_>,
        job_id: &str,
        failure_class: Option<&str>,
        attempt_status: AttemptStatus,
        now_epoch_ms: i64,
    ) -> Result<(), String> {
        let previous = Self::function_health_for_job_tx(tx, job_id)?;
        let is_success = attempt_status == AttemptStatus::Succeeded;
        let is_infra_failure = !is_success && is_infrastructure_failure_class(failure_class);

        let next = if is_success {
            FunctionHealthRecord {
                job_id: job_id.to_string(),
                state: FunctionHealthState::Healthy,
                consecutive_infra_failures: 0,
                reason: None,
                hold_until_epoch_ms: None,
                last_changed_at_epoch_ms: if previous
                    .as_ref()
                    .map(|record| record.state == FunctionHealthState::Healthy)
                    .unwrap_or(false)
                {
                    previous
                        .as_ref()
                        .map(|record| record.last_changed_at_epoch_ms)
                        .unwrap_or(now_epoch_ms)
                } else {
                    now_epoch_ms
                },
                last_success_at_epoch_ms: Some(now_epoch_ms),
                last_failure_at_epoch_ms: previous
                    .as_ref()
                    .and_then(|record| record.last_failure_at_epoch_ms),
            }
        } else if is_infra_failure {
            let consecutive_infra_failures = previous
                .as_ref()
                .map(|record| record.consecutive_infra_failures)
                .unwrap_or(0)
                .saturating_add(1);
            let state = if consecutive_infra_failures >= 5 {
                FunctionHealthState::Down
            } else if consecutive_infra_failures >= 3 {
                FunctionHealthState::Degraded
            } else {
                FunctionHealthState::Healthy
            };
            FunctionHealthRecord {
                job_id: job_id.to_string(),
                state,
                consecutive_infra_failures,
                reason: Some(
                    failure_class
                        .map(str::to_string)
                        .unwrap_or_else(|| "infrastructure failure".to_string()),
                ),
                hold_until_epoch_ms: if matches!(
                    state,
                    FunctionHealthState::Degraded | FunctionHealthState::Down
                ) {
                    Some(now_epoch_ms + function_health_hold_delay_ms())
                } else {
                    None
                },
                last_changed_at_epoch_ms: if previous
                    .as_ref()
                    .map(|record| record.state == state)
                    .unwrap_or(false)
                {
                    previous
                        .as_ref()
                        .map(|record| record.last_changed_at_epoch_ms)
                        .unwrap_or(now_epoch_ms)
                } else {
                    now_epoch_ms
                },
                last_success_at_epoch_ms: previous
                    .as_ref()
                    .and_then(|record| record.last_success_at_epoch_ms),
                last_failure_at_epoch_ms: Some(now_epoch_ms),
            }
        } else {
            previous.unwrap_or(FunctionHealthRecord {
                job_id: job_id.to_string(),
                state: FunctionHealthState::Healthy,
                consecutive_infra_failures: 0,
                reason: None,
                hold_until_epoch_ms: None,
                last_changed_at_epoch_ms: now_epoch_ms,
                last_success_at_epoch_ms: None,
                last_failure_at_epoch_ms: None,
            })
        };

        tx.execute(
            "INSERT INTO function_health (
                job_id, state, consecutive_infra_failures, reason, hold_until, last_changed_at,
                last_success_at, last_failure_at, updated_at
             ) VALUES (
                $1,
                $2,
                $3,
                $4,
                CASE WHEN $5::double precision IS NULL THEN NULL ELSE TO_TIMESTAMP($5::double precision / 1000.0) END,
                TO_TIMESTAMP($6::double precision / 1000.0),
                CASE WHEN $7::double precision IS NULL THEN NULL ELSE TO_TIMESTAMP($7::double precision / 1000.0) END,
                CASE WHEN $8::double precision IS NULL THEN NULL ELSE TO_TIMESTAMP($8::double precision / 1000.0) END,
                NOW()
             )
             ON CONFLICT (job_id) DO UPDATE
             SET state = EXCLUDED.state,
                 consecutive_infra_failures = EXCLUDED.consecutive_infra_failures,
                 reason = EXCLUDED.reason,
                 hold_until = EXCLUDED.hold_until,
                 last_changed_at = EXCLUDED.last_changed_at,
                 last_success_at = EXCLUDED.last_success_at,
                 last_failure_at = EXCLUDED.last_failure_at,
                 updated_at = NOW()",
            &[
                &next.job_id,
                &function_health_state_to_str(next.state),
                &(next.consecutive_infra_failures as i32),
                &next.reason,
                &next.hold_until_epoch_ms.map(|value| value as f64),
                &(next.last_changed_at_epoch_ms as f64),
                &next.last_success_at_epoch_ms.map(|value| value as f64),
                &next.last_failure_at_epoch_ms.map(|value| value as f64),
            ],
        )
        .map_err(|error| format!("failed to update function health: {error}"))?;

        Ok(())
    }
}

impl GumStore for PostgresStore {
    fn register_deploy(
        &self,
        params: RegisterDeployParams,
    ) -> Result<(DeployRecord, Vec<JobRecord>), String> {
        let mut client = self.connect_client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| format!("failed to start deploy transaction: {error}"))?;

        let project_exists = tx
            .query_opt(
                "SELECT id FROM projects WHERE id = $1",
                &[&params.project_id],
            )
            .map_err(|error| format!("failed to check project: {error}"))?;
        if project_exists.is_none() {
            return Err("project not found".to_string());
        }

        let deploy = DeployRecord {
            id: self.next_id("dep"),
            project_id: params.project_id.clone(),
            version: params.version,
            bundle_url: params.bundle_url,
            bundle_sha256: params.bundle_sha256,
            sdk_language: params.sdk_language,
            entrypoint: params.entrypoint,
            status: DeployStatus::Ready,
        };
        let created_at_epoch_ms = now_epoch_ms();

        tx.execute(
            "INSERT INTO deploys (
                id, project_id, version, bundle_url, bundle_sha256, sdk_language, entrypoint, status
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            &[
                &deploy.id,
                &deploy.project_id,
                &deploy.version,
                &deploy.bundle_url,
                &deploy.bundle_sha256,
                &deploy.sdk_language,
                &deploy.entrypoint,
                &deploy_status_to_str(deploy.status),
            ],
        )
        .map_err(|error| format!("failed to insert deploy: {error}"))?;

        let mut jobs = Vec::with_capacity(params.jobs.len());
        for job in params.jobs {
            let record = JobRecord {
                id: job.id,
                project_id: params.project_id.clone(),
                deploy_id: deploy.id.clone(),
                name: job.name,
                handler_ref: job.handler_ref,
                trigger_mode: job.trigger_mode,
                schedule_expr: job.schedule_expr,
                retries: job.retries,
                timeout_secs: job.timeout_secs,
                rate_limit_spec: job.rate_limit_spec,
                concurrency_limit: job.concurrency_limit,
                memory_mb: job.memory_mb,
                key_field: job.key_field,
                compute_class: job.compute_class,
                enabled: true,
                created_at_epoch_ms,
            };

            tx.execute(
                "INSERT INTO jobs (
                    id, project_id, deploy_id, name, handler_ref, trigger_mode, schedule_expr,
                    retries, timeout_secs, rate_limit_spec, concurrency_limit, memory_mb, key_field, compute_class, enabled
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
                 ON CONFLICT (id) DO UPDATE
                 SET project_id = EXCLUDED.project_id,
                     deploy_id = EXCLUDED.deploy_id,
                     name = EXCLUDED.name,
                     handler_ref = EXCLUDED.handler_ref,
                     trigger_mode = EXCLUDED.trigger_mode,
                     schedule_expr = EXCLUDED.schedule_expr,
                     retries = EXCLUDED.retries,
                     timeout_secs = EXCLUDED.timeout_secs,
                     rate_limit_spec = EXCLUDED.rate_limit_spec,
                     concurrency_limit = EXCLUDED.concurrency_limit,
                     memory_mb = EXCLUDED.memory_mb,
                     key_field = EXCLUDED.key_field,
                     compute_class = EXCLUDED.compute_class,
                     enabled = EXCLUDED.enabled,
                     updated_at = NOW()",
                &[
                    &record.id,
                    &record.project_id,
                    &record.deploy_id,
                    &record.name,
                    &record.handler_ref,
                    &record.trigger_mode,
                    &record.schedule_expr,
                    &(record.retries as i32),
                    &(record.timeout_secs as i32),
                    &record.rate_limit_spec,
                    &record.concurrency_limit.map(|value| value as i32),
                    &record.memory_mb.map(|value| value as i32),
                    &record.key_field,
                    &record.compute_class,
                    &record.enabled,
                ],
            )
            .map_err(|error| format!("failed to insert job {}: {error}", record.id))?;
            jobs.push(record);
        }

        tx.commit()
            .map_err(|error| format!("failed to commit deploy transaction: {error}"))?;
        Ok((deploy, jobs))
    }

    fn upsert_provider_target(
        &self,
        params: UpsertProviderTargetParams,
    ) -> Result<ProviderTargetRecord, String> {
        let mut client = self.connect_client()?;
        let row = client
            .query_one(
                "INSERT INTO provider_targets (
                    id, name, slug, probe_kind, probe_config_json, enabled, updated_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, NOW())
                 ON CONFLICT (id) DO UPDATE
                 SET name = EXCLUDED.name,
                     slug = EXCLUDED.slug,
                     probe_kind = EXCLUDED.probe_kind,
                     probe_config_json = EXCLUDED.probe_config_json,
                     enabled = EXCLUDED.enabled,
                     updated_at = NOW()
                 RETURNING *,
                           (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_at_epoch_ms",
                &[
                    &params.id,
                    &params.name,
                    &params.slug,
                    &params.probe_kind,
                    &params.probe_config_json,
                    &params.enabled,
                ],
            )
            .map_err(|error| format!("failed to upsert provider target: {error}"))?;
        provider_target_from_row(row)
    }

    fn record_provider_check(
        &self,
        params: RecordProviderCheckParams,
    ) -> Result<ProviderCheckRecord, String> {
        let mut client = self.connect_client()?;
        let row = client
            .query_one(
                "INSERT INTO provider_checks (
                    id, provider_target_id, status, latency_ms, error_class, status_code, checked_at
                 ) VALUES (
                    $1,
                    $2,
                    $3,
                    $4,
                    $5,
                    $6,
                    TO_TIMESTAMP($7::double precision / 1000.0)
                 )
                 RETURNING *,
                           (EXTRACT(EPOCH FROM checked_at) * 1000)::bigint AS checked_at_epoch_ms",
                &[
                    &self.next_id("pch"),
                    &params.provider_target_id,
                    &provider_check_status_to_str(params.status),
                    &params.latency_ms.map(|value| value as i32),
                    &params.error_class,
                    &params.status_code.map(|value| value as i32),
                    &(params.checked_at_epoch_ms as f64),
                ],
            )
            .map_err(|error| format!("failed to record provider check: {error}"))?;
        provider_check_from_row(row)
    }

    fn set_provider_health(
        &self,
        params: SetProviderHealthParams,
    ) -> Result<ProviderHealthRecord, String> {
        let mut client = self.connect_client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| format!("failed to start provider health transaction: {error}"))?;
        tx.execute(
                "INSERT INTO provider_health (
                    provider_target_id, state, reason, last_changed_at, last_success_at,
                    last_failure_at, degraded_score, down_score, updated_at
                 ) VALUES (
                    $1,
                    $2,
                    $3,
                    TO_TIMESTAMP($4::double precision / 1000.0),
                    CASE WHEN $5::double precision IS NULL THEN NULL ELSE TO_TIMESTAMP($5::double precision / 1000.0) END,
                    CASE WHEN $6::double precision IS NULL THEN NULL ELSE TO_TIMESTAMP($6::double precision / 1000.0) END,
                    $7,
                    $8,
                    NOW()
                 )
                 ON CONFLICT (provider_target_id) DO UPDATE
                 SET state = EXCLUDED.state,
                     reason = EXCLUDED.reason,
                     last_changed_at = EXCLUDED.last_changed_at,
                     last_success_at = EXCLUDED.last_success_at,
                     last_failure_at = EXCLUDED.last_failure_at,
                     degraded_score = EXCLUDED.degraded_score,
                     down_score = EXCLUDED.down_score,
                     updated_at = NOW()",
                &[
                    &params.provider_target_id,
                    &provider_health_state_to_str(params.state),
                    &params.reason,
                    &(params.last_changed_at_epoch_ms as f64),
                    &params.last_success_at_epoch_ms.map(|value| value as f64),
                    &params.last_failure_at_epoch_ms.map(|value| value as f64),
                    &params.degraded_score,
                    &params.down_score,
                ],
            )
            .map_err(|error| format!("failed to set provider health: {error}"))?;
        let row = tx
            .query_one(
                "SELECT provider_health.provider_target_id,
                        provider_targets.name AS provider_name,
                        provider_targets.slug AS provider_slug,
                        provider_health.state,
                        provider_health.reason,
                        (EXTRACT(EPOCH FROM provider_health.last_changed_at) * 1000)::bigint AS last_changed_at_epoch_ms,
                        CASE
                            WHEN provider_health.last_success_at IS NULL THEN NULL
                            ELSE (EXTRACT(EPOCH FROM provider_health.last_success_at) * 1000)::bigint
                        END AS last_success_at_epoch_ms,
                        CASE
                            WHEN provider_health.last_failure_at IS NULL THEN NULL
                            ELSE (EXTRACT(EPOCH FROM provider_health.last_failure_at) * 1000)::bigint
                        END AS last_failure_at_epoch_ms,
                        provider_health.degraded_score,
                        provider_health.down_score
                 FROM provider_health
                 JOIN provider_targets ON provider_targets.id = provider_health.provider_target_id
                 WHERE provider_health.provider_target_id = $1",
                &[&params.provider_target_id],
            )
            .map_err(|error| format!("failed to reload provider health: {error}"))?;
        tx.commit()
            .map_err(|error| format!("failed to commit provider health transaction: {error}"))?;
        provider_health_from_row(row)
    }

    fn set_function_health(
        &self,
        params: SetFunctionHealthParams,
    ) -> Result<FunctionHealthRecord, String> {
        let mut client = self.connect_client()?;
        let row = client
            .query_one(
                "INSERT INTO function_health (
                    job_id, state, consecutive_infra_failures, reason, hold_until, last_changed_at,
                    last_success_at, last_failure_at, updated_at
                 ) VALUES (
                    $1,
                    $2,
                    $3,
                    $4,
                    CASE WHEN $5::double precision IS NULL THEN NULL ELSE TO_TIMESTAMP($5::double precision / 1000.0) END,
                    TO_TIMESTAMP($6::double precision / 1000.0),
                    CASE WHEN $7::double precision IS NULL THEN NULL ELSE TO_TIMESTAMP($7::double precision / 1000.0) END,
                    CASE WHEN $8::double precision IS NULL THEN NULL ELSE TO_TIMESTAMP($8::double precision / 1000.0) END,
                    NOW()
                 )
                 ON CONFLICT (job_id) DO UPDATE
                 SET state = EXCLUDED.state,
                     consecutive_infra_failures = EXCLUDED.consecutive_infra_failures,
                     reason = EXCLUDED.reason,
                     hold_until = EXCLUDED.hold_until,
                     last_changed_at = EXCLUDED.last_changed_at,
                     last_success_at = EXCLUDED.last_success_at,
                     last_failure_at = EXCLUDED.last_failure_at,
                     updated_at = NOW()
                 RETURNING job_id,
                           state,
                           consecutive_infra_failures,
                           reason,
                           CASE
                               WHEN hold_until IS NULL THEN NULL
                               ELSE (EXTRACT(EPOCH FROM hold_until) * 1000)::bigint
                           END AS hold_until_epoch_ms,
                           (EXTRACT(EPOCH FROM last_changed_at) * 1000)::bigint AS last_changed_at_epoch_ms,
                           CASE
                               WHEN last_success_at IS NULL THEN NULL
                               ELSE (EXTRACT(EPOCH FROM last_success_at) * 1000)::bigint
                           END AS last_success_at_epoch_ms,
                           CASE
                               WHEN last_failure_at IS NULL THEN NULL
                               ELSE (EXTRACT(EPOCH FROM last_failure_at) * 1000)::bigint
                           END AS last_failure_at_epoch_ms",
                &[
                    &params.job_id,
                    &function_health_state_to_str(params.state),
                    &(params.consecutive_infra_failures as i32),
                    &params.reason,
                    &params.hold_until_epoch_ms.map(|value| value as f64),
                    &(params.last_changed_at_epoch_ms as f64),
                    &params.last_success_at_epoch_ms.map(|value| value as f64),
                    &params.last_failure_at_epoch_ms.map(|value| value as f64),
                ],
            )
            .map_err(|error| format!("failed to set function health: {error}"))?;
        function_health_from_row(row)
    }

    fn get_function_health(&self, job_id: &str) -> Result<Option<FunctionHealthRecord>, String> {
        let mut client = self.connect_client()?;
        let row = client
            .query_opt(
                "SELECT job_id,
                        state,
                        consecutive_infra_failures,
                        reason,
                        CASE
                            WHEN hold_until IS NULL THEN NULL
                            ELSE (EXTRACT(EPOCH FROM hold_until) * 1000)::bigint
                        END AS hold_until_epoch_ms,
                        (EXTRACT(EPOCH FROM last_changed_at) * 1000)::bigint AS last_changed_at_epoch_ms,
                        CASE
                            WHEN last_success_at IS NULL THEN NULL
                            ELSE (EXTRACT(EPOCH FROM last_success_at) * 1000)::bigint
                        END AS last_success_at_epoch_ms,
                        CASE
                            WHEN last_failure_at IS NULL THEN NULL
                            ELSE (EXTRACT(EPOCH FROM last_failure_at) * 1000)::bigint
                        END AS last_failure_at_epoch_ms
                 FROM function_health
                 WHERE job_id = $1",
                &[&job_id],
            )
            .map_err(|error| format!("failed to get function health: {error}"))?;
        row.map(function_health_from_row).transpose()
    }

    fn list_provider_targets(&self) -> Result<Vec<ProviderTargetRecord>, String> {
        let mut client = self.connect_client()?;
        let rows = client
            .query(
                "SELECT *,
                        (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_at_epoch_ms
                 FROM provider_targets
                 ORDER BY slug ASC",
                &[],
            )
            .map_err(|error| format!("failed to list provider targets: {error}"))?;
        rows.into_iter().map(provider_target_from_row).collect()
    }

    fn list_provider_health(&self) -> Result<Vec<ProviderHealthRecord>, String> {
        let mut client = self.connect_client()?;
        let rows = client
            .query(
                "SELECT provider_health.provider_target_id,
                        provider_targets.name AS provider_name,
                        provider_targets.slug AS provider_slug,
                        provider_health.state,
                        provider_health.reason,
                        (EXTRACT(EPOCH FROM provider_health.last_changed_at) * 1000)::bigint AS last_changed_at_epoch_ms,
                        CASE
                            WHEN provider_health.last_success_at IS NULL THEN NULL
                            ELSE (EXTRACT(EPOCH FROM provider_health.last_success_at) * 1000)::bigint
                        END AS last_success_at_epoch_ms,
                        CASE
                            WHEN provider_health.last_failure_at IS NULL THEN NULL
                            ELSE (EXTRACT(EPOCH FROM provider_health.last_failure_at) * 1000)::bigint
                        END AS last_failure_at_epoch_ms,
                        provider_health.degraded_score,
                        provider_health.down_score
                 FROM provider_health
                 JOIN provider_targets ON provider_targets.id = provider_health.provider_target_id
                 ORDER BY provider_targets.slug ASC",
                &[],
            )
            .map_err(|error| format!("failed to list provider health: {error}"))?;
        rows.into_iter().map(provider_health_from_row).collect()
    }

    fn register_runner(&self, params: RegisterRunnerParams) -> Result<RunnerRecord, String> {
        let mut client = self.connect_client()?;
        let row = client
            .query_one(
                "INSERT INTO runners (
                    id, compute_class, memory_mb, max_concurrent_leases, heartbeat_timeout_secs, last_heartbeat_at
                 ) VALUES ($1, $2, $3, $4, $5, NOW())
                 ON CONFLICT (id) DO UPDATE
                 SET compute_class = EXCLUDED.compute_class,
                     memory_mb = EXCLUDED.memory_mb,
                     max_concurrent_leases = EXCLUDED.max_concurrent_leases,
                     heartbeat_timeout_secs = EXCLUDED.heartbeat_timeout_secs,
                     last_heartbeat_at = EXCLUDED.last_heartbeat_at,
                     updated_at = NOW()
                 RETURNING id,
                           compute_class,
                           memory_mb,
                           max_concurrent_leases,
                           heartbeat_timeout_secs,
                           (EXTRACT(EPOCH FROM last_heartbeat_at) * 1000)::bigint AS last_heartbeat_at_epoch_ms",
                &[
                    &params.runner_id,
                    &params.compute_class,
                    &(params.memory_mb as i32),
                    &(params.max_concurrent_leases as i32),
                    &(params.heartbeat_timeout_secs as i32),
                ],
            )
            .map_err(|error| format!("failed to register runner: {error}"))?;
        runner_from_row(row)
    }

    fn heartbeat_runner(&self, params: HeartbeatRunnerParams) -> Result<RunnerRecord, String> {
        let mut client = self.connect_client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| format!("failed to start heartbeat transaction: {error}"))?;

        let runner_row = tx
            .query_one(
                "INSERT INTO runners (
                    id, compute_class, memory_mb, max_concurrent_leases, heartbeat_timeout_secs, last_heartbeat_at
                 ) VALUES ($1, $2, $3, $4, $5, NOW())
                 ON CONFLICT (id) DO UPDATE
                 SET compute_class = EXCLUDED.compute_class,
                     memory_mb = EXCLUDED.memory_mb,
                     max_concurrent_leases = EXCLUDED.max_concurrent_leases,
                     heartbeat_timeout_secs = EXCLUDED.heartbeat_timeout_secs,
                     last_heartbeat_at = EXCLUDED.last_heartbeat_at,
                     updated_at = NOW()
                 RETURNING id,
                           compute_class,
                           memory_mb,
                           max_concurrent_leases,
                           heartbeat_timeout_secs,
                           (EXTRACT(EPOCH FROM last_heartbeat_at) * 1000)::bigint AS last_heartbeat_at_epoch_ms",
                &[
                    &params.runner_id,
                    &params.compute_class,
                    &(params.memory_mb as i32),
                    &(params.max_concurrent_leases as i32),
                    &(params.heartbeat_timeout_secs as i32),
                ],
            )
            .map_err(|error| format!("failed to upsert runner heartbeat: {error}"))?;

        for lease_id in &params.active_lease_ids {
            let ownership = tx
                .query_opt(
                    "SELECT runner_id, acked_at, released_at
                     FROM leases
                     WHERE id = $1
                     FOR UPDATE",
                    &[lease_id],
                )
                .map_err(|error| format!("failed to load lease for heartbeat: {error}"))?
                .ok_or_else(|| format!("lease not found: {lease_id}"))?;

            let runner_id: String = ownership.get("runner_id");
            if runner_id != params.runner_id {
                return Err(format!("runner does not own lease: {lease_id}"));
            }

            let acked_at: Option<std::time::SystemTime> = ownership.get("acked_at");
            let released_at: Option<std::time::SystemTime> = ownership.get("released_at");
            if acked_at.is_some() || released_at.is_some() {
                continue;
            }

            tx.execute(
                "UPDATE leases
                 SET expires_at = NOW() + ($1::bigint * INTERVAL '1 second')
                 WHERE id = $2",
                &[&(params.lease_ttl_secs as i64), lease_id],
            )
            .map_err(|error| format!("failed to renew lease {lease_id}: {error}"))?;
        }

        tx.commit()
            .map_err(|error| format!("failed to commit heartbeat transaction: {error}"))?;
        runner_from_row(runner_row)
    }

    fn try_acquire_control_lease(&self, params: ControlLeaseParams) -> Result<bool, String> {
        let mut client = self.connect_client()?;
        let row = client
            .query_opt(
                "INSERT INTO control_leases (name, holder_id, expires_at, updated_at)
                 VALUES (
                    $1,
                    $2,
                    TO_TIMESTAMP($3::double precision / 1000.0) + ($4::bigint * INTERVAL '1 second'),
                    TO_TIMESTAMP($3::double precision / 1000.0)
                 )
                 ON CONFLICT (name) DO UPDATE
                 SET holder_id = EXCLUDED.holder_id,
                     expires_at = EXCLUDED.expires_at,
                     updated_at = EXCLUDED.updated_at
                 WHERE control_leases.holder_id = EXCLUDED.holder_id
                    OR control_leases.expires_at <= TO_TIMESTAMP($3::double precision / 1000.0)
                 RETURNING name",
                &[
                    &params.lease_name,
                    &params.holder_id,
                    &(params.now_epoch_ms as f64),
                    &(params.ttl_secs as i64),
                ],
            )
            .map_err(|error| format!("failed to acquire control lease: {error}"))?;
        Ok(row.is_some())
    }

    fn get_job(&self, job_id: &str) -> Result<Option<JobRecord>, String> {
        let mut client = self.connect_client()?;
        let row = client
            .query_opt(
                "SELECT *, (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_at_epoch_ms
                 FROM jobs
                 WHERE id = $1",
                &[&job_id],
            )
            .map_err(|error| format!("failed to load job: {error}"))?;
        row.map(job_from_row).transpose()
    }

    fn get_run(&self, run_id: &str) -> Result<Option<RunRecord>, String> {
        let mut client = self.connect_client()?;
        let row = client
            .query_opt(
                "SELECT *, (EXTRACT(EPOCH FROM scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms
                 FROM runs
                 WHERE id = $1",
                &[&run_id],
            )
            .map_err(|error| format!("failed to load run: {error}"))?;
        row.map(run_from_row).transpose()
    }

    fn get_deploy(&self, deploy_id: &str) -> Result<Option<DeployRecord>, String> {
        let mut client = self.connect_client()?;
        let row = client
            .query_opt("SELECT * FROM deploys WHERE id = $1", &[&deploy_id])
            .map_err(|error| format!("failed to load deploy: {error}"))?;
        row.map(deploy_from_row).transpose()
    }

    fn get_lease_state(&self, lease_id: &str) -> Result<Option<LeaseStateRecord>, String> {
        let mut client = self.connect_client()?;
        let row = client
            .query_opt(
                "SELECT leases.id AS lease_id,
                        attempts.id AS attempt_id,
                        attempts.run_id AS run_id,
                        (attempts.cancel_requested_at IS NOT NULL OR leases.revoke_requested_at IS NOT NULL) AS cancel_requested
                 FROM leases
                 JOIN attempts ON attempts.id = leases.attempt_id
                 WHERE leases.id = $1",
                &[&lease_id],
            )
            .map_err(|error| format!("failed to load lease state: {error}"))?;
        Ok(row.map(|row| LeaseStateRecord {
            lease_id: row.get("lease_id"),
            run_id: row.get("run_id"),
            attempt_id: row.get("attempt_id"),
            cancel_requested: row.get("cancel_requested"),
        }))
    }

    fn list_recent_runs(&self, limit: usize) -> Result<Vec<RunRecord>, String> {
        let mut client = self.connect_client()?;
        let rows = client
            .query(
                "SELECT *,
                        (EXTRACT(EPOCH FROM scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms
                 FROM runs
                 ORDER BY scheduled_at DESC, id DESC
                 LIMIT $1",
                &[&(limit as i64)],
            )
            .map_err(|error| format!("failed to list recent runs: {error}"))?;
        rows.into_iter().map(run_from_row).collect()
    }

    fn list_runners(&self) -> Result<Vec<RunnerStatusRecord>, String> {
        let mut client = self.connect_client()?;
        let rows = client
            .query(
                "SELECT runners.id,
                        runners.compute_class,
                        runners.memory_mb,
                        runners.max_concurrent_leases,
                        (EXTRACT(EPOCH FROM runners.last_heartbeat_at) * 1000)::bigint AS last_heartbeat_at_epoch_ms,
                        COUNT(attempts.id)::bigint AS active_lease_count,
                        COALESCE(SUM(COALESCE(jobs.memory_mb, 512)), 0)::bigint AS active_memory_mb
                 FROM runners
                 LEFT JOIN attempts
                   ON attempts.runner_id = runners.id
                  AND attempts.status = 'running'
                 LEFT JOIN runs ON runs.id = attempts.run_id
                 LEFT JOIN jobs ON jobs.id = runs.job_id
                 GROUP BY runners.id, runners.compute_class, runners.memory_mb, runners.max_concurrent_leases, runners.last_heartbeat_at
                 ORDER BY runners.id ASC",
                &[],
            )
            .map_err(|error| format!("failed to list runners: {error}"))?;
        rows.into_iter()
            .map(|row| {
                let active_lease_count: i64 = row.get("active_lease_count");
                let active_memory_mb: i64 = row.get("active_memory_mb");
                Ok(RunnerStatusRecord {
                    id: row.get("id"),
                    compute_class: row.get("compute_class"),
                    memory_mb: row.get::<_, i32>("memory_mb") as u32,
                    active_memory_mb: active_memory_mb as u32,
                    max_concurrent_leases: row.get::<_, i32>("max_concurrent_leases") as u32,
                    last_heartbeat_at_epoch_ms: row.get("last_heartbeat_at_epoch_ms"),
                    active_lease_count: active_lease_count as u32,
                })
            })
            .collect()
    }

    fn list_active_leases(&self) -> Result<Vec<LeaseStatusRecord>, String> {
        let mut client = self.connect_client()?;
        let rows = client
            .query(
                "SELECT leases.id AS lease_id,
                        attempts.run_id AS run_id,
                        attempts.id AS attempt_id,
                        leases.runner_id AS runner_id,
                        (EXTRACT(EPOCH FROM leases.expires_at) * 1000)::bigint AS expires_at_epoch_ms,
                        (attempts.cancel_requested_at IS NOT NULL OR leases.revoke_requested_at IS NOT NULL) AS cancel_requested
                 FROM leases
                 JOIN attempts ON attempts.id = leases.attempt_id
                 WHERE leases.acked_at IS NULL
                   AND leases.released_at IS NULL
                 ORDER BY leases.expires_at ASC, leases.id ASC",
                &[],
            )
            .map_err(|error| format!("failed to list active leases: {error}"))?;
        rows.into_iter()
            .map(|row| {
                Ok(LeaseStatusRecord {
                    lease_id: row.get("lease_id"),
                    run_id: row.get("run_id"),
                    attempt_id: row.get("attempt_id"),
                    runner_id: row.get("runner_id"),
                    expires_at_epoch_ms: row.get("expires_at_epoch_ms"),
                    cancel_requested: row.get("cancel_requested"),
                })
            })
            .collect()
    }

    fn list_concurrency_status(&self) -> Result<Vec<ConcurrencyStatusRecord>, String> {
        let mut client = self.connect_client()?;
        let rows = client
            .query(
                "SELECT jobs.id AS job_id,
                        jobs.name AS job_name,
                        jobs.concurrency_limit AS concurrency_limit,
                        COALESCE(active.active_run_ids, ARRAY[]::text[]) AS active_run_ids,
                        COALESCE(queued.queued_run_ids, ARRAY[]::text[]) AS queued_run_ids
                 FROM jobs
                 LEFT JOIN LATERAL (
                     SELECT ARRAY_AGG(DISTINCT runs.id ORDER BY runs.id) AS active_run_ids
                     FROM runs
                     JOIN attempts ON attempts.run_id = runs.id
                     WHERE runs.job_id = jobs.id
                       AND attempts.status = 'running'
                 ) active ON TRUE
                 LEFT JOIN LATERAL (
                     SELECT ARRAY_AGG(runs.id ORDER BY runs.scheduled_at ASC, runs.id ASC) AS queued_run_ids
                     FROM runs
                     WHERE runs.job_id = jobs.id
                       AND runs.status = 'queued'
                 ) queued ON TRUE
                 WHERE jobs.enabled = TRUE
                   AND jobs.concurrency_limit IS NOT NULL
                 ORDER BY jobs.name ASC, jobs.id ASC",
                &[],
            )
            .map_err(|error| format!("failed to list concurrency status: {error}"))?;
        rows.into_iter()
            .map(|row| {
                let concurrency_limit: i32 = row.get("concurrency_limit");
                Ok(ConcurrencyStatusRecord {
                    job_id: row.get("job_id"),
                    job_name: row.get("job_name"),
                    concurrency_limit: concurrency_limit as u32,
                    active_run_ids: row.get("active_run_ids"),
                    queued_run_ids: row.get("queued_run_ids"),
                })
            })
            .collect()
    }

    fn list_rate_limit_status(&self) -> Result<Vec<RateLimitStatusRecord>, String> {
        let mut client = self.connect_client()?;
        let job_rows = client
            .query(
                "SELECT id, project_id, name, rate_limit_spec
                 FROM jobs
                 WHERE enabled = TRUE
                   AND rate_limit_spec IS NOT NULL
                 ORDER BY name ASC, id ASC",
                &[],
            )
            .map_err(|error| format!("failed to load jobs for rate-limit status: {error}"))?;

        #[derive(Clone)]
        struct ScopeSeed {
            project_id: String,
            job_id: String,
            scope_key: String,
            scope_kind: String,
            pool_name: Option<String>,
            limit: u32,
            window_ms: i64,
            job_ids: Vec<String>,
            job_names: Vec<String>,
        }

        let mut scopes = HashMap::<String, ScopeSeed>::new();
        for row in job_rows {
            let job_id: String = row.get("id");
            let project_id: String = row.get("project_id");
            let job_name: String = row.get("name");
            let rate_limit_spec: String = row.get("rate_limit_spec");
            let spec = parse_rate_limit_spec(&rate_limit_spec)?;
            let scope_key = rate_limit_scope_key(&project_id, &job_id, &spec);
            let entry = scopes
                .entry(scope_key.clone())
                .or_insert_with(|| ScopeSeed {
                    project_id: project_id.clone(),
                    job_id: job_id.clone(),
                    scope_key: scope_key.clone(),
                    scope_kind: if spec.pool.is_some() {
                        "pool".to_string()
                    } else {
                        "job".to_string()
                    },
                    pool_name: spec.pool.clone(),
                    limit: spec.limit,
                    window_ms: spec.window_ms,
                    job_ids: Vec::new(),
                    job_names: Vec::new(),
                });
            entry.job_ids.push(job_id);
            entry.job_names.push(job_name);
        }

        let now_ms = now_epoch_ms();
        let mut statuses = Vec::new();
        for mut scope in scopes.into_values() {
            scope.job_ids.sort();
            scope.job_names.sort();

            let window_start_ms = now_ms.saturating_sub(scope.window_ms);
            let recent_count: i64 = if let Some(pool_name) = scope.pool_name.as_deref() {
                client
                    .query_one(
                        "SELECT COUNT(*) AS count
                         FROM attempts
                         JOIN runs ON runs.id = attempts.run_id
                         JOIN jobs ON jobs.id = runs.job_id
                         WHERE attempts.started_at >= TO_TIMESTAMP($1::double precision / 1000.0)
                           AND jobs.project_id = $2
                           AND jobs.rate_limit_spec IS NOT NULL
                           AND POSITION(':' IN jobs.rate_limit_spec) > 0
                           AND split_part(jobs.rate_limit_spec, ':', 1) = $3",
                        &[&(window_start_ms as f64), &scope.project_id, &pool_name],
                    )
                    .map_err(|error| format!("failed to count pooled rate-limit usage: {error}"))?
                    .get("count")
            } else {
                client
                    .query_one(
                        "SELECT COUNT(*) AS count
                         FROM attempts
                         JOIN runs ON runs.id = attempts.run_id
                         WHERE attempts.started_at >= TO_TIMESTAMP($1::double precision / 1000.0)
                           AND runs.job_id = $2",
                        &[&(window_start_ms as f64), &scope.job_id],
                    )
                    .map_err(|error| format!("failed to count job rate-limit usage: {error}"))?
                    .get("count")
            };

            let waiting_run_ids = if recent_count >= i64::from(scope.limit) {
                let rows = if let Some(pool_name) = scope.pool_name.as_deref() {
                    client
                        .query(
                            "SELECT runs.id,
                                    runs.scheduled_at,
                                    jobs.concurrency_limit,
                                    COALESCE(active.active_count, 0) AS active_count
                             FROM runs
                             JOIN jobs ON jobs.id = runs.job_id
                             LEFT JOIN LATERAL (
                                 SELECT COUNT(DISTINCT attempts.run_id) AS active_count
                                 FROM attempts
                                 JOIN runs active_runs ON active_runs.id = attempts.run_id
                                 WHERE attempts.status = 'running'
                                   AND active_runs.job_id = jobs.id
                             ) active ON TRUE
                             WHERE runs.status = 'queued'
                               AND (runs.retry_after_epoch_ms IS NULL OR runs.retry_after_epoch_ms <= $1)
                               AND COALESCE(runs.failure_class, '') <> 'blocked_by_downstream'
                               AND jobs.project_id = $2
                               AND jobs.rate_limit_spec IS NOT NULL
                               AND POSITION(':' IN jobs.rate_limit_spec) > 0
                               AND split_part(jobs.rate_limit_spec, ':', 1) = $3
                             ORDER BY runs.scheduled_at ASC, runs.id ASC",
                            &[&now_ms, &scope.project_id, &pool_name],
                        )
                        .map_err(|error| {
                            format!("failed to list pooled rate-limit waiting runs: {error}")
                        })?
                } else {
                    client
                        .query(
                            "SELECT runs.id,
                                    runs.scheduled_at,
                                    jobs.concurrency_limit,
                                    COALESCE(active.active_count, 0) AS active_count
                             FROM runs
                             JOIN jobs ON jobs.id = runs.job_id
                             LEFT JOIN LATERAL (
                                 SELECT COUNT(DISTINCT attempts.run_id) AS active_count
                                 FROM attempts
                                 JOIN runs active_runs ON active_runs.id = attempts.run_id
                                 WHERE attempts.status = 'running'
                                   AND active_runs.job_id = jobs.id
                             ) active ON TRUE
                             WHERE runs.status = 'queued'
                               AND (runs.retry_after_epoch_ms IS NULL OR runs.retry_after_epoch_ms <= $1)
                               AND COALESCE(runs.failure_class, '') <> 'blocked_by_downstream'
                               AND runs.job_id = $2
                             ORDER BY runs.scheduled_at ASC, runs.id ASC",
                            &[&now_ms, &scope.job_id],
                        )
                        .map_err(|error| {
                            format!("failed to list job rate-limit waiting runs: {error}")
                        })?
                };

                rows.into_iter()
                    .filter(|row| {
                        let concurrency_limit: Option<i32> = row.get("concurrency_limit");
                        let active_count: i64 = row.get("active_count");
                        concurrency_limit.map_or(true, |limit| active_count < i64::from(limit))
                    })
                    .map(|row| row.get("id"))
                    .collect()
            } else {
                Vec::new()
            };

            statuses.push(RateLimitStatusRecord {
                scope_key: scope.scope_key,
                scope_kind: scope.scope_kind,
                project_id: scope.project_id,
                pool_name: scope.pool_name,
                limit: scope.limit,
                window_ms: scope.window_ms,
                recent_start_count: recent_count as u32,
                job_ids: scope.job_ids,
                job_names: scope.job_names,
                waiting_run_ids,
            });
        }

        statuses.sort_by(|left, right| {
            left.scope_kind
                .cmp(&right.scope_kind)
                .then_with(|| left.scope_key.cmp(&right.scope_key))
        });
        Ok(statuses)
    }

    fn enqueue_run(&self, params: EnqueueRunParams) -> Result<EnqueueRunResult, String> {
        let mut client = self.connect_client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| format!("failed to start enqueue transaction: {error}"))?;
        let row = tx
            .query_opt(
                "SELECT enabled, project_id, deploy_id, retries FROM jobs WHERE id = $1",
                &[&params.job_id],
            )
            .map_err(|error| format!("failed to validate job before enqueue: {error}"))?
            .ok_or_else(|| "job not found".to_string())?;

        let enabled: bool = row.get("enabled");
        if !enabled {
            return Err("job disabled".to_string());
        }
        let project_id: String = row.get("project_id");
        let deploy_id: String = row.get("deploy_id");
        if project_id != params.project_id || deploy_id != params.deploy_id {
            return Err("job/project/deploy mismatch".to_string());
        }
        let retries: i32 = row.get("retries");

        if let Some(key_value) = params.dedupe_key_value.as_deref() {
            let lock_id = Self::run_key_lock_id(&params.project_id, &params.job_id, key_value);
            tx.query_one("SELECT pg_advisory_xact_lock($1)", &[&lock_id])
                .map_err(|error| format!("failed to acquire run key lock: {error}"))?;
            tx.execute(
                "DELETE FROM run_keys
                 WHERE project_id = $1
                   AND job_id = $2
                   AND key_value = $3
                   AND expires_at <= NOW()",
                &[&params.project_id, &params.job_id, &key_value],
            )
            .map_err(|error| format!("failed to clear expired run key: {error}"))?;

            if let Some(existing) = tx
                .query_opt(
                    "SELECT runs.*,
                            (EXTRACT(EPOCH FROM runs.scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms
                     FROM run_keys
                     JOIN runs ON runs.id = run_keys.run_id
                     WHERE run_keys.project_id = $1
                       AND run_keys.job_id = $2
                       AND run_keys.key_value = $3
                       AND run_keys.expires_at > NOW()",
                    &[&params.project_id, &params.job_id, &key_value],
                )
                .map_err(|error| format!("failed to load existing keyed run: {error}"))?
            {
                tx.commit()
                    .map_err(|error| format!("failed to commit keyed enqueue transaction: {error}"))?;
                return Ok(EnqueueRunResult {
                    run: run_from_row(existing)?,
                    deduped: true,
                });
            }
        }

        let run_id = self.next_id("run");
        let max_attempts = retries + 1;
        let inserted = tx
            .query_one(
                "INSERT INTO runs (
                    id, project_id, job_id, deploy_id, trigger_type, status,
                    input_json, attempt_count, max_attempts, scheduled_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, 0, $8, NOW())
                 RETURNING *, (EXTRACT(EPOCH FROM scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms",
                &[
                    &run_id,
                    &params.project_id,
                    &params.job_id,
                    &params.deploy_id,
                    &trigger_type_to_str(TriggerType::Enqueue),
                    &run_status_to_str(RunStatus::Queued),
                    &params.input_json,
                    &max_attempts,
                ],
            )
            .map_err(|error| format!("failed to enqueue run: {error}"))?;
        let run = run_from_row(inserted)?;
        if let Some(key_value) = params.dedupe_key_value {
            let expires_at_epoch_ms = now_epoch_ms() + key_retention_ms();
            tx.execute(
                "INSERT INTO run_keys (
                    project_id, job_id, key_value, run_id, expires_at
                 ) VALUES ($1, $2, $3, $4, TO_TIMESTAMP($5 / 1000.0))",
                &[
                    &run.project_id,
                    &run.job_id,
                    &key_value,
                    &run.id,
                    &(expires_at_epoch_ms as f64),
                ],
            )
            .map_err(|error| format!("failed to insert run key: {error}"))?;
        }
        tx.commit()
            .map_err(|error| format!("failed to commit enqueue transaction: {error}"))?;
        Ok(EnqueueRunResult {
            run,
            deduped: false,
        })
    }

    fn replay_run(&self, params: ReplayRunParams) -> Result<RunRecord, String> {
        let mut client = self.connect_client()?;
        let source = client
            .query_one(
                "SELECT *, (EXTRACT(EPOCH FROM scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms
                 FROM runs
                 WHERE id = $1",
                &[&params.source_run_id],
            )
            .map_err(|error| format!("failed to load source run: {error}"))?;
        let source_run = run_from_row(source)?;

        let replay_id = self.next_id("run");
        let inserted = client
            .query_one(
                "INSERT INTO runs (
                    id, project_id, job_id, deploy_id, trigger_type, status,
                    input_json, attempt_count, max_attempts, replay_of_run_id, scheduled_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, 0, $8, $9, NOW())
                 RETURNING *, (EXTRACT(EPOCH FROM scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms",
                &[
                    &replay_id,
                    &source_run.project_id,
                    &source_run.job_id,
                    &source_run.deploy_id,
                    &trigger_type_to_str(TriggerType::Replay),
                    &run_status_to_str(RunStatus::Queued),
                    &source_run.input_json,
                    &(source_run.max_attempts as i32),
                    &source_run.id,
                ],
            )
            .map_err(|error| format!("failed to create replay run: {error}"))?;

        run_from_row(inserted)
    }

    fn lease_next_attempt(
        &self,
        params: LeaseNextAttemptParams,
    ) -> Result<Option<(RunRecord, AttemptRecord, LeaseRecord)>, String> {
        self.recover_lost_attempts(now_epoch_ms())?;

        let mut client = self.connect_client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| format!("failed to start lease transaction: {error}"))?;

        let runner_row = tx
            .query_opt(
                "SELECT id,
                        compute_class,
                        memory_mb,
                        max_concurrent_leases,
                        heartbeat_timeout_secs,
                        (EXTRACT(EPOCH FROM last_heartbeat_at) * 1000)::bigint AS last_heartbeat_at_epoch_ms
                 FROM runners
                 WHERE id = $1
                 FOR UPDATE",
                &[&params.runner_id],
            )
            .map_err(|error| format!("failed to load runner for lease: {error}"))?
            .ok_or_else(|| "runner not registered".to_string())?;
        let runner = runner_from_row(runner_row)?;

        let active_runner_leases: i64 = tx
            .query_one(
                "SELECT COUNT(*) AS count
                 FROM attempts
                 WHERE status = 'running'
                   AND runner_id = $1",
                &[&params.runner_id],
            )
            .map_err(|error| format!("failed to count active runner leases: {error}"))?
            .get("count");
        if active_runner_leases >= i64::from(runner.max_concurrent_leases) {
            tx.commit().map_err(|error| {
                format!("failed to close full-runner lease transaction: {error}")
            })?;
            return Ok(None);
        }

        let candidate_rows = tx
            .query(
                "SELECT runs.*, (EXTRACT(EPOCH FROM runs.scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms
                 FROM runs
                 JOIN jobs ON jobs.id = runs.job_id
                 LEFT JOIN function_health ON function_health.job_id = jobs.id
                 WHERE runs.status = 'queued'
                   AND (runs.retry_after_epoch_ms IS NULL OR runs.retry_after_epoch_ms <= $1)
                   AND (
                     function_health.hold_until IS NULL
                     OR function_health.hold_until <= TO_TIMESTAMP($1::double precision / 1000.0)
                   )
                   AND jobs.enabled = TRUE
                   AND (
                     jobs.concurrency_limit IS NULL OR (
                       SELECT COUNT(*)
                       FROM attempts
                       JOIN runs AS active_runs ON active_runs.id = attempts.run_id
                       WHERE attempts.status = 'running'
                         AND active_runs.job_id = jobs.id
                     ) < jobs.concurrency_limit
                   )
                 ORDER BY runs.created_at ASC
                 FOR UPDATE OF runs SKIP LOCKED",
                &[&now_epoch_ms()],
            )
            .map_err(|error| format!("failed to select candidate runs for lease: {error}"))?;

        let mut selected_run: Option<RunRecord> = None;
        for run_row in candidate_rows {
            let run = run_from_row(run_row)?;
            let job_row = tx
                .query_one(
                    "SELECT project_id, rate_limit_spec, compute_class, memory_mb
                     FROM jobs
                     WHERE id = $1",
                    &[&run.job_id],
                )
                .map_err(|error| format!("failed to load job for rate-limit check: {error}"))?;
            let project_id: String = job_row.get("project_id");
            let rate_limit_spec: Option<String> = job_row.get("rate_limit_spec");
            let compute_class: Option<String> = job_row.get("compute_class");
            let memory_mb: Option<i32> = job_row.get("memory_mb");

            if let Some(required_class) = compute_class {
                if runner.compute_class != required_class {
                    continue;
                }
            }
            let required_memory_mb = memory_mb.unwrap_or(512);
            let active_memory_row = tx
                .query_one(
                    "SELECT COALESCE(SUM(COALESCE(jobs.memory_mb, 512)), 0)::bigint AS active_memory_mb
                     FROM attempts
                     JOIN runs ON runs.id = attempts.run_id
                     JOIN jobs ON jobs.id = runs.job_id
                     WHERE attempts.status = 'running'
                       AND attempts.runner_id = $1",
                    &[&params.runner_id],
                )
                .map_err(|error| format!("failed to count active runner memory: {error}"))?;
            let active_memory_mb: i64 = active_memory_row.get("active_memory_mb");
            if active_memory_mb.saturating_add(i64::from(required_memory_mb))
                > i64::from(runner.memory_mb)
            {
                continue;
            }

            let allowed = if let Some(rate_limit_spec) = rate_limit_spec {
                let spec = parse_rate_limit_spec(&rate_limit_spec)?;
                let window_start_ms = now_epoch_ms().saturating_sub(spec.window_ms);
                let recent_count_row = if let Some(pool_name) = spec.pool.as_deref() {
                    tx.query_one(
                        "SELECT COUNT(*) AS count
                         FROM attempts
                         JOIN runs ON runs.id = attempts.run_id
                         JOIN jobs ON jobs.id = runs.job_id
                         WHERE attempts.started_at >= TO_TIMESTAMP($1::double precision / 1000.0)
                           AND jobs.project_id = $2
                           AND jobs.rate_limit_spec IS NOT NULL
                           AND POSITION(':' IN jobs.rate_limit_spec) > 0
                           AND split_part(jobs.rate_limit_spec, ':', 1) = $3",
                        &[&(window_start_ms as f64), &project_id, &pool_name],
                    )
                    .map_err(|error| format!("failed to count pooled rate-limit usage: {error}"))?
                } else {
                    tx.query_one(
                        "SELECT COUNT(*) AS count
                         FROM attempts
                         JOIN runs ON runs.id = attempts.run_id
                         WHERE attempts.started_at >= TO_TIMESTAMP($1::double precision / 1000.0)
                           AND runs.job_id = $2",
                        &[&(window_start_ms as f64), &run.job_id],
                    )
                    .map_err(|error| format!("failed to count job rate-limit usage: {error}"))?
                };

                let recent_count: i64 = recent_count_row.get("count");
                recent_count < i64::from(spec.limit)
            } else {
                true
            };

            if allowed {
                selected_run = Some(run);
                break;
            }
        }

        let Some(run) = selected_run else {
            tx.commit()
                .map_err(|error| format!("failed to close empty lease transaction: {error}"))?;
            return Ok(None);
        };
        let attempt_number = (run.attempt_count + 1) as i32;
        let attempt_id = self.next_id("att");
        let lease_id = self.next_id("lease");
        let expires_at = timestamp_after_seconds(params.lease_ttl_secs);

        let updated_run_row = tx
            .query_one(
                "UPDATE runs
                 SET status = 'running',
                     failure_reason = NULL,
                     failure_class = NULL,
                     retry_after_epoch_ms = NULL,
                     waiting_for_provider_slug = NULL,
                     attempt_count = attempt_count + 1,
                     started_at = COALESCE(started_at, NOW()),
                     updated_at = NOW()
                 WHERE id = $1
                 RETURNING *, (EXTRACT(EPOCH FROM scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms",
                &[&run.id],
            )
            .map_err(|error| format!("failed to mark run running: {error}"))?;

        let attempt_row = tx
            .query_one(
                "INSERT INTO attempts (
                    id, run_id, attempt_number, status, runner_id, started_at, cancel_requested_at
                 ) VALUES ($1, $2, $3, 'running', $4, NOW(), NULL)
                 RETURNING *,
                           (EXTRACT(EPOCH FROM started_at) * 1000)::bigint AS started_at_epoch_ms,
                           (EXTRACT(EPOCH FROM finished_at) * 1000)::bigint AS finished_at_epoch_ms,
                           (EXTRACT(EPOCH FROM cancel_requested_at) * 1000)::bigint AS cancel_requested_at_epoch_ms",
                &[&attempt_id, &run.id, &attempt_number, &params.runner_id],
            )
            .map_err(|error| format!("failed to insert attempt: {error}"))?;

        let lease_row = tx
            .query_one(
                "INSERT INTO leases (id, attempt_id, runner_id, expires_at, revoke_requested_at)
                 VALUES ($1, $2, $3, TO_TIMESTAMP($4::double precision / 1000.0), NULL)
                 RETURNING id, attempt_id, runner_id,
                           (EXTRACT(EPOCH FROM expires_at) * 1000)::bigint AS expires_at_epoch_ms,
                           NULL::bigint AS acked_at_epoch_ms,
                           NULL::bigint AS released_at_epoch_ms,
                           NULL::bigint AS revoke_requested_at_epoch_ms",
                &[
                    &lease_id,
                    &attempt_id,
                    &params.runner_id,
                    &(expires_at as f64),
                ],
            )
            .map_err(|error| format!("failed to insert lease: {error}"))?;

        tx.execute(
            "UPDATE attempts SET lease_id = $1 WHERE id = $2",
            &[&lease_id, &attempt_id],
        )
        .map_err(|error| format!("failed to attach lease to attempt: {error}"))?;

        tx.commit()
            .map_err(|error| format!("failed to commit lease transaction: {error}"))?;

        Ok(Some((
            run_from_row(updated_run_row)?,
            attempt_from_row(attempt_row)?,
            lease_from_projection_row(lease_row)?,
        )))
    }

    fn complete_attempt(
        &self,
        params: CompleteAttemptParams,
    ) -> Result<(AttemptRecord, RunRecord), String> {
        let mut client = self.connect_client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| format!("failed to start completion transaction: {error}"))?;

        let attempt_row = tx
            .query_opt(
                "SELECT *,
                        (EXTRACT(EPOCH FROM started_at) * 1000)::bigint AS started_at_epoch_ms,
                        (EXTRACT(EPOCH FROM finished_at) * 1000)::bigint AS finished_at_epoch_ms,
                        (EXTRACT(EPOCH FROM cancel_requested_at) * 1000)::bigint AS cancel_requested_at_epoch_ms
                 FROM attempts
                 WHERE id = $1
                 FOR UPDATE",
                &[&params.attempt_id],
            )
            .map_err(|error| format!("failed to load attempt for completion: {error}"))?
            .ok_or_else(|| "attempt not found".to_string())?;
        let attempt = attempt_from_row(attempt_row)?;

        if attempt.finished_at_epoch_ms.is_some() || is_terminal_attempt(attempt.status) {
            return Err("attempt already finished".to_string());
        }
        if attempt.cancel_requested_at_epoch_ms.is_some()
            && params.status != AttemptStatus::Canceled
        {
            return Err("attempt cancel requested".to_string());
        }

        if attempt.runner_id.as_deref() != Some(params.runner_id.as_str()) {
            return Err("runner mismatch".to_string());
        }

        let lease_id = attempt
            .lease_id
            .as_deref()
            .ok_or_else(|| "attempt lease missing".to_string())?;
        let lease_row = tx
            .query_opt(
                "SELECT id,
                        attempt_id,
                        runner_id,
                        (EXTRACT(EPOCH FROM expires_at) * 1000)::bigint AS expires_at_epoch_ms,
                        (EXTRACT(EPOCH FROM acked_at) * 1000)::bigint AS acked_at_epoch_ms,
                        (EXTRACT(EPOCH FROM released_at) * 1000)::bigint AS released_at_epoch_ms,
                        (EXTRACT(EPOCH FROM revoke_requested_at) * 1000)::bigint AS revoke_requested_at_epoch_ms
                 FROM leases
                 WHERE id = $1
                 FOR UPDATE",
                &[&lease_id],
            )
            .map_err(|error| format!("failed to load lease for completion: {error}"))?
            .ok_or_else(|| "attempt lease missing".to_string())?;
        let lease = lease_from_projection_row(lease_row)?;
        if lease.acked_at_epoch_ms.is_some()
            || lease.released_at_epoch_ms.is_some()
            || lease.expires_at_epoch_ms <= now_epoch_ms()
        {
            return Err("attempt lease no longer valid".to_string());
        }

        let updated_attempt_row = tx
            .query_one(
                "UPDATE attempts
                 SET status = $1,
                     failure_reason = $2,
                     failure_class = $3,
                     finished_at = NOW()
                 WHERE id = $4
                 RETURNING *,
                           (EXTRACT(EPOCH FROM started_at) * 1000)::bigint AS started_at_epoch_ms,
                           (EXTRACT(EPOCH FROM finished_at) * 1000)::bigint AS finished_at_epoch_ms,
                           (EXTRACT(EPOCH FROM cancel_requested_at) * 1000)::bigint AS cancel_requested_at_epoch_ms",
                &[
                    &attempt_status_to_str(params.status),
                    &params.failure_reason,
                    &params.failure_class,
                    &params.attempt_id,
                ],
            )
            .map_err(|error| format!("failed to update attempt: {error}"))?;

        tx.execute(
            "UPDATE leases
             SET acked_at = NOW(),
                 released_at = NOW()
             WHERE id = $1",
            &[&lease_id],
        )
        .map_err(|error| format!("failed to ack lease: {error}"))?;

        let run_row = tx
            .query_one(
                "SELECT *, (EXTRACT(EPOCH FROM scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms
                 FROM runs
                 WHERE id = $1
                 FOR UPDATE",
                &[&attempt.run_id],
            )
            .map_err(|error| format!("failed to load run for completion: {error}"))?;
        let run = run_from_row(run_row)?;
        let job_row = tx
            .query_one(
                "SELECT *,
                        (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_at_epoch_ms
                 FROM jobs
                 WHERE id = $1",
                &[&run.job_id],
            )
            .map_err(|error| format!("failed to load job for completion: {error}"))?;
        let job = job_from_row(job_row)?;
        let provider_slug = provider_slug_from_job(&job)?;
        Self::apply_function_signal_tx(
            &mut tx,
            &job.id,
            params.failure_class.as_deref(),
            params.status,
            now_epoch_ms(),
        )?;

        if let Some(provider_slug) = provider_slug.as_deref() {
            if params.status == AttemptStatus::Succeeded {
                self.apply_provider_signal_tx(
                    &mut tx,
                    provider_slug,
                    ProviderCheckStatus::Success,
                    None,
                    now_epoch_ms(),
                )?;
            } else if is_provider_failure_class(params.failure_class.as_deref()) {
                self.apply_provider_signal_tx(
                    &mut tx,
                    provider_slug,
                    ProviderCheckStatus::Failure,
                    params.failure_class.as_deref(),
                    now_epoch_ms(),
                )?;
            }
        }

        let function_health = Self::function_health_for_job_tx(&mut tx, &job.id)?;
        let disposition = compute_retry_disposition(
            &run.id,
            run.attempt_count,
            run.max_attempts,
            params.status,
            params.failure_reason.clone(),
            params.failure_class.clone(),
            function_health.as_ref(),
            now_epoch_ms(),
        );

        let updated_run_row = if disposition.finished_now {
            tx.query_one(
                "UPDATE runs
                 SET status = $1,
                     failure_reason = $2,
                     failure_class = $3,
                     retry_after_epoch_ms = NULL,
                     waiting_for_provider_slug = NULL,
                     finished_at = NOW(),
                     updated_at = NOW()
                 WHERE id = $4
                 RETURNING *, (EXTRACT(EPOCH FROM scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms",
                &[
                    &run_status_to_str(disposition.next_status),
                    &disposition.failure_reason,
                    &disposition.failure_class,
                    &run.id,
                ],
            )
            .map_err(|error| format!("failed to finalize run: {error}"))?
        } else {
            tx.query_one(
                "UPDATE runs
                 SET status = $1,
                     failure_reason = $2,
                     failure_class = $3,
                     retry_after_epoch_ms = $4,
                     waiting_for_provider_slug = $5,
                     updated_at = NOW()
                 WHERE id = $6
                 RETURNING *, (EXTRACT(EPOCH FROM scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms",
                &[
                    &run_status_to_str(disposition.next_status),
                    &disposition.failure_reason,
                    &disposition.failure_class,
                    &disposition.retry_after_epoch_ms,
                    &disposition.waiting_for_scope_key,
                    &run.id,
                ],
            )
            .map_err(|error| format!("failed to requeue run: {error}"))?
        };

        tx.commit()
            .map_err(|error| format!("failed to commit completion transaction: {error}"))?;

        Ok((
            attempt_from_row(updated_attempt_row)?,
            run_from_row(updated_run_row)?,
        ))
    }

    fn cancel_run(&self, params: CancelRunParams) -> Result<RunRecord, String> {
        let mut client = self.connect_client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| format!("failed to start cancel transaction: {error}"))?;

        let run_row = tx
            .query_opt(
                "SELECT *,
                        (EXTRACT(EPOCH FROM scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms
                 FROM runs
                 WHERE id = $1
                 FOR UPDATE",
                &[&params.run_id],
            )
            .map_err(|error| format!("failed to load run for cancel: {error}"))?
            .ok_or_else(|| "run not found".to_string())?;
        let run = run_from_row(run_row)?;

        let updated_run_row = match run.status {
            RunStatus::Queued => tx
                .query_one(
                    "UPDATE runs
                     SET status = 'canceled',
                         failure_reason = 'canceled',
                         finished_at = TO_TIMESTAMP($2::double precision / 1000.0),
                         updated_at = TO_TIMESTAMP($2::double precision / 1000.0)
                     WHERE id = $1
                     RETURNING *,
                               (EXTRACT(EPOCH FROM scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms",
                    &[&run.id, &(params.requested_at_epoch_ms as f64)],
                )
                .map_err(|error| format!("failed to cancel queued run: {error}"))?,
            RunStatus::Running => {
                let attempt_row = tx
                    .query_opt(
                        "SELECT id, lease_id
                         FROM attempts
                         WHERE run_id = $1
                           AND status = 'running'
                         FOR UPDATE",
                        &[&run.id],
                    )
                    .map_err(|error| format!("failed to load running attempt for cancel: {error}"))?
                    .ok_or_else(|| "running attempt not found".to_string())?;
                let attempt_id: String = attempt_row.get("id");
                let lease_id: Option<String> = attempt_row.get("lease_id");

                tx.execute(
                    "UPDATE attempts
                     SET cancel_requested_at = TO_TIMESTAMP($2::double precision / 1000.0)
                     WHERE id = $1",
                    &[&attempt_id, &(params.requested_at_epoch_ms as f64)],
                )
                .map_err(|error| format!("failed to mark attempt cancel requested: {error}"))?;

                if let Some(lease_id) = lease_id {
                    tx.execute(
                        "UPDATE leases
                         SET revoke_requested_at = TO_TIMESTAMP($2::double precision / 1000.0)
                         WHERE id = $1",
                        &[&lease_id, &(params.requested_at_epoch_ms as f64)],
                    )
                    .map_err(|error| format!("failed to mark lease revoke requested: {error}"))?;
                }

                tx.query_one(
                    "UPDATE runs
                     SET failure_reason = 'cancel requested',
                         updated_at = TO_TIMESTAMP($2::double precision / 1000.0)
                     WHERE id = $1
                     RETURNING *,
                               (EXTRACT(EPOCH FROM scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms",
                    &[&run.id, &(params.requested_at_epoch_ms as f64)],
                )
                .map_err(|error| format!("failed to mark run cancel requested: {error}"))?
            }
            RunStatus::Succeeded
            | RunStatus::Failed
            | RunStatus::TimedOut
            | RunStatus::Canceled => return Err("run already finished".to_string()),
        };

        tx.commit()
            .map_err(|error| format!("failed to commit cancel transaction: {error}"))?;
        run_from_row(updated_run_row)
    }

    fn recover_lost_attempts(&self, now_epoch_ms: i64) -> Result<Vec<RunRecord>, String> {
        let mut client = self.connect_client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| format!("failed to start recovery transaction: {error}"))?;

        let rows = tx
            .query(
                "SELECT attempts.id AS attempt_id,
                        attempts.run_id AS run_id,
                        attempts.lease_id AS lease_id,
                        runs.attempt_count AS attempt_count,
                        runs.max_attempts AS max_attempts
                 FROM attempts
                 JOIN runs ON runs.id = attempts.run_id
                 JOIN leases ON leases.id = attempts.lease_id
                 LEFT JOIN runners ON runners.id = leases.runner_id
                 WHERE attempts.status = 'running'
                   AND leases.acked_at IS NULL
                   AND leases.released_at IS NULL
                   AND (
                     leases.expires_at <= TO_TIMESTAMP($1::double precision / 1000.0)
                     OR (
                       runners.id IS NOT NULL
                       AND runners.last_heartbeat_at
                           + (runners.heartbeat_timeout_secs * INTERVAL '1 second')
                           <= TO_TIMESTAMP($1::double precision / 1000.0)
                     )
                   )
                 FOR UPDATE OF attempts, runs, leases SKIP LOCKED",
                &[&(now_epoch_ms as f64)],
            )
            .map_err(|error| format!("failed to load lost attempts: {error}"))?;

        let mut recovered_runs = Vec::new();
        for row in rows {
            let attempt_id: String = row.get("attempt_id");
            let run_id: String = row.get("run_id");
            let lease_id: String = row.get("lease_id");
            let attempt_count: i32 = row.get("attempt_count");
            let max_attempts: i32 = row.get("max_attempts");
            let final_failure = attempt_count >= max_attempts;

            tx.execute(
                "UPDATE attempts
                 SET status = 'failed',
                     failure_reason = 'runner lost lease',
                     failure_class = 'gum_internal_error',
                     finished_at = TO_TIMESTAMP($2::double precision / 1000.0)
                 WHERE id = $1",
                &[&attempt_id, &(now_epoch_ms as f64)],
            )
            .map_err(|error| format!("failed to mark lost attempt failed: {error}"))?;

            tx.execute(
                "UPDATE leases
                 SET released_at = TO_TIMESTAMP($2::double precision / 1000.0)
                 WHERE id = $1",
                &[&lease_id, &(now_epoch_ms as f64)],
            )
            .map_err(|error| format!("failed to release lost lease: {error}"))?;

            // Recovery only changes the currently leased attempt. The run keeps its
            // existing attempt_count so a requeue naturally leases the next attempt number.
            let run_row = if final_failure {
                tx.query_one(
                    "UPDATE runs
                     SET status = 'failed',
                         failure_reason = 'runner lost lease',
                         failure_class = 'gum_internal_error',
                         retry_after_epoch_ms = NULL,
                         waiting_for_provider_slug = NULL,
                         finished_at = TO_TIMESTAMP($2::double precision / 1000.0),
                         updated_at = TO_TIMESTAMP($2::double precision / 1000.0)
                     WHERE id = $1
                     RETURNING *, (EXTRACT(EPOCH FROM scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms",
                    &[&run_id, &(now_epoch_ms as f64)],
                )
                .map_err(|error| format!("failed to finalize lost run: {error}"))?
            } else {
                tx.query_one(
                    "UPDATE runs
                     SET status = 'queued',
                         failure_reason = NULL,
                         failure_class = NULL,
                         retry_after_epoch_ms = NULL,
                         waiting_for_provider_slug = NULL,
                         updated_at = TO_TIMESTAMP($2::double precision / 1000.0)
                     WHERE id = $1
                     RETURNING *, (EXTRACT(EPOCH FROM scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms",
                    &[&run_id, &(now_epoch_ms as f64)],
                )
                .map_err(|error| format!("failed to requeue lost run: {error}"))?
            };

            recovered_runs.push(run_from_row(run_row)?);
        }

        tx.commit()
            .map_err(|error| format!("failed to commit recovery transaction: {error}"))?;
        Ok(recovered_runs)
    }

    fn tick_schedules(&self, now_epoch_ms: i64) -> Result<Vec<RunRecord>, String> {
        let mut client = self.connect_client()?;
        let job_rows = client
            .query(
                "SELECT id, project_id, deploy_id, retries, schedule_expr,
                        (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_at_epoch_ms
                 FROM jobs
                 WHERE enabled = TRUE
                   AND schedule_expr IS NOT NULL
                 ORDER BY created_at ASC",
                &[],
            )
            .map_err(|error| format!("failed to load scheduled jobs: {error}"))?;

        let mut created_runs = Vec::new();

        for row in job_rows {
            let job_id: String = row.get("id");
            let project_id: String = row.get("project_id");
            let deploy_id: String = row.get("deploy_id");
            let retries: i32 = row.get("retries");
            let schedule_expr: String = row.get("schedule_expr");
            let created_at_epoch_ms: i64 = row.get("created_at_epoch_ms");
            let interval_ms = parse_schedule_interval_ms(&schedule_expr)?;

            // Schedules stay anchored to the original job creation time. The latest
            // scheduled run tells us which fire time was last materialized, so a new
            // scheduler process can catch up without drifting the schedule.
            let latest_scheduled_ms = client
                .query_opt(
                    "SELECT (EXTRACT(EPOCH FROM scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms
                     FROM runs
                     WHERE job_id = $1
                       AND trigger_type = 'schedule'
                     ORDER BY scheduled_at DESC
                     LIMIT 1",
                    &[&job_id],
                )
                .map_err(|error| format!("failed to load latest scheduled run: {error}"))?
                .map(|latest| latest.get::<_, i64>("scheduled_at_epoch_ms"))
                .unwrap_or(created_at_epoch_ms);

            let mut next_due_ms = latest_scheduled_ms.saturating_add(interval_ms);
            while next_due_ms <= now_epoch_ms {
                let run_id = self.next_id("run");
                let inserted = client
                    .query_opt(
                        "INSERT INTO runs (
                            id, project_id, job_id, deploy_id, trigger_type, status,
                            input_json, attempt_count, max_attempts, scheduled_at
                         ) VALUES (
                            $1, $2, $3, $4, 'schedule', 'queued',
                            '{}'::jsonb, 0, $5, TO_TIMESTAMP($6::double precision / 1000.0)
                         )
                         ON CONFLICT DO NOTHING
                         RETURNING *, (EXTRACT(EPOCH FROM scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms",
                        &[
                            &run_id,
                            &project_id,
                            &job_id,
                            &deploy_id,
                            &(retries + 1),
                            &(next_due_ms as f64),
                        ],
                    )
                    .map_err(|error| format!("failed to insert scheduled run: {error}"))?;

                if let Some(row) = inserted {
                    created_runs.push(run_from_row(row)?);
                }

                next_due_ms = next_due_ms.saturating_add(interval_ms);
            }
        }

        Ok(created_runs)
    }

    fn append_log(&self, log: LogRecord) -> Result<(), String> {
        let mut client = self.connect_client()?;
        client
            .execute(
                "INSERT INTO logs (id, run_id, attempt_id, stream, message)
                 VALUES ($1, $2, $3, $4, $5)",
                &[
                    &log.id,
                    &log.run_id,
                    &log.attempt_id,
                    &log.stream,
                    &log.message,
                ],
            )
            .map_err(|error| format!("failed to append log: {error}"))?;
        Ok(())
    }

    fn list_run_logs(&self, run_id: &str) -> Result<Vec<LogRecord>, String> {
        let mut client = self.connect_client()?;
        let rows = client
            .query(
                "SELECT id, run_id, attempt_id, stream, message
                 FROM logs
                 WHERE run_id = $1
                 ORDER BY ts ASC, id ASC",
                &[&run_id],
            )
            .map_err(|error| format!("failed to list logs: {error}"))?;

        rows.into_iter().map(log_from_row).collect()
    }
}

fn deploy_from_row(row: Row) -> Result<DeployRecord, String> {
    Ok(DeployRecord {
        id: row.get("id"),
        project_id: row.get("project_id"),
        version: row.get("version"),
        bundle_url: row.get("bundle_url"),
        bundle_sha256: row.get("bundle_sha256"),
        sdk_language: row.get("sdk_language"),
        entrypoint: row.get("entrypoint"),
        status: deploy_status_from_str(row.get("status"))?,
    })
}

fn job_from_row(row: Row) -> Result<JobRecord, String> {
    let retries: i32 = row.get("retries");
    let timeout_secs: i32 = row.get("timeout_secs");
    let concurrency_limit: Option<i32> = row.get("concurrency_limit");
    let memory_mb: Option<i32> = row.get("memory_mb");
    let created_at_epoch_ms: i64 = row
        .try_get("created_at_epoch_ms")
        .map_err(|error| format!("job row missing created_at_epoch_ms: {error}"))?;

    Ok(JobRecord {
        id: row.get("id"),
        project_id: row.get("project_id"),
        deploy_id: row.get("deploy_id"),
        name: row.get("name"),
        handler_ref: row.get("handler_ref"),
        trigger_mode: row.get("trigger_mode"),
        schedule_expr: row.get("schedule_expr"),
        retries: retries as u32,
        timeout_secs: timeout_secs as u32,
        rate_limit_spec: row.get("rate_limit_spec"),
        concurrency_limit: concurrency_limit.map(|value| value as u32),
        memory_mb: memory_mb.map(|value| value as u32),
        key_field: row.get("key_field"),
        compute_class: row.get("compute_class"),
        enabled: row.get("enabled"),
        created_at_epoch_ms,
    })
}

fn run_from_row(row: Row) -> Result<RunRecord, String> {
    let attempt_count: i32 = row.get("attempt_count");
    let max_attempts: i32 = row.get("max_attempts");
    let scheduled_at_epoch_ms: i64 = row
        .try_get("scheduled_at_epoch_ms")
        .map_err(|error| format!("run row missing scheduled_at_epoch_ms: {error}"))?;

    Ok(RunRecord {
        id: row.get("id"),
        project_id: row.get("project_id"),
        job_id: row.get("job_id"),
        deploy_id: row.get("deploy_id"),
        trigger_type: trigger_type_from_str(row.get("trigger_type"))?,
        status: run_status_from_str(row.get("status"))?,
        input_json: row.get("input_json"),
        attempt_count: attempt_count as u32,
        max_attempts: max_attempts as u32,
        scheduled_at_epoch_ms,
        failure_reason: row.get("failure_reason"),
        failure_class: row.get("failure_class"),
        retry_after_epoch_ms: row.get("retry_after_epoch_ms"),
        waiting_for_provider_slug: row.get("waiting_for_provider_slug"),
        replay_of_run_id: row.get("replay_of_run_id"),
    })
}

fn attempt_from_row(row: Row) -> Result<AttemptRecord, String> {
    let attempt_number: i32 = row.get("attempt_number");
    let started_at_epoch_ms: i64 = row
        .try_get("started_at_epoch_ms")
        .map_err(|error| format!("attempt row missing started_at_epoch_ms: {error}"))?;
    let finished_at_epoch_ms: Option<i64> = row
        .try_get("finished_at_epoch_ms")
        .map_err(|error| format!("attempt row missing finished_at_epoch_ms: {error}"))?;
    let cancel_requested_at_epoch_ms: Option<i64> = row
        .try_get("cancel_requested_at_epoch_ms")
        .map_err(|error| format!("attempt row missing cancel_requested_at_epoch_ms: {error}"))?;
    Ok(AttemptRecord {
        id: row.get("id"),
        run_id: row.get("run_id"),
        attempt_number: attempt_number as u32,
        status: attempt_status_from_str(row.get("status"))?,
        lease_id: row.get("lease_id"),
        runner_id: row.get("runner_id"),
        started_at_epoch_ms,
        finished_at_epoch_ms,
        failure_reason: row.get("failure_reason"),
        failure_class: row.get("failure_class"),
        cancel_requested_at_epoch_ms,
    })
}

fn lease_from_projection_row(row: Row) -> Result<LeaseRecord, String> {
    let revoke_requested_at_epoch_ms: Option<i64> = row
        .try_get("revoke_requested_at_epoch_ms")
        .map_err(|error| format!("lease row missing revoke_requested_at_epoch_ms: {error}"))?;
    Ok(LeaseRecord {
        id: row.get("id"),
        attempt_id: row.get("attempt_id"),
        runner_id: row.get("runner_id"),
        expires_at_epoch_ms: row.get("expires_at_epoch_ms"),
        acked_at_epoch_ms: row.get("acked_at_epoch_ms"),
        released_at_epoch_ms: row.get("released_at_epoch_ms"),
        revoke_requested_at_epoch_ms,
    })
}

fn runner_from_row(row: Row) -> Result<RunnerRecord, String> {
    let memory_mb: i32 = row.get("memory_mb");
    let max_concurrent_leases: i32 = row.get("max_concurrent_leases");
    let heartbeat_timeout_secs: i32 = row.get("heartbeat_timeout_secs");
    Ok(RunnerRecord {
        id: row.get("id"),
        compute_class: row.get("compute_class"),
        memory_mb: memory_mb as u32,
        max_concurrent_leases: max_concurrent_leases as u32,
        heartbeat_timeout_secs: heartbeat_timeout_secs as u64,
        last_heartbeat_at_epoch_ms: row.get("last_heartbeat_at_epoch_ms"),
    })
}

fn provider_target_from_row(row: Row) -> Result<ProviderTargetRecord, String> {
    let created_at_epoch_ms: i64 = row
        .try_get("created_at_epoch_ms")
        .map_err(|error| format!("provider target row missing created_at_epoch_ms: {error}"))?;
    Ok(ProviderTargetRecord {
        id: row.get("id"),
        name: row.get("name"),
        slug: row.get("slug"),
        probe_kind: row.get("probe_kind"),
        probe_config_json: row.get("probe_config_json"),
        enabled: row.get("enabled"),
        created_at_epoch_ms,
    })
}

fn provider_check_from_row(row: Row) -> Result<ProviderCheckRecord, String> {
    let latency_ms: Option<i32> = row.get("latency_ms");
    let status_code: Option<i32> = row.get("status_code");
    let checked_at_epoch_ms: i64 = row
        .try_get("checked_at_epoch_ms")
        .map_err(|error| format!("provider check row missing checked_at_epoch_ms: {error}"))?;
    Ok(ProviderCheckRecord {
        id: row.get("id"),
        provider_target_id: row.get("provider_target_id"),
        status: provider_check_status_from_str(row.get("status"))?,
        latency_ms: latency_ms.map(|value| value as u32),
        error_class: row.get("error_class"),
        status_code: status_code.map(|value| value as u16),
        checked_at_epoch_ms,
    })
}

fn provider_health_from_row(row: Row) -> Result<ProviderHealthRecord, String> {
    Ok(ProviderHealthRecord {
        provider_target_id: row.get("provider_target_id"),
        provider_name: row.get("provider_name"),
        provider_slug: row.get("provider_slug"),
        state: provider_health_state_from_str(row.get("state"))?,
        reason: row.get("reason"),
        last_changed_at_epoch_ms: row.get("last_changed_at_epoch_ms"),
        last_success_at_epoch_ms: row.get("last_success_at_epoch_ms"),
        last_failure_at_epoch_ms: row.get("last_failure_at_epoch_ms"),
        degraded_score: row.get("degraded_score"),
        down_score: row.get("down_score"),
    })
}

fn function_health_from_row(row: Row) -> Result<FunctionHealthRecord, String> {
    let consecutive_infra_failures: i32 = row.get("consecutive_infra_failures");
    Ok(FunctionHealthRecord {
        job_id: row.get("job_id"),
        state: function_health_state_from_str(row.get("state"))?,
        consecutive_infra_failures: consecutive_infra_failures as u32,
        reason: row.get("reason"),
        hold_until_epoch_ms: row.get("hold_until_epoch_ms"),
        last_changed_at_epoch_ms: row.get("last_changed_at_epoch_ms"),
        last_success_at_epoch_ms: row.get("last_success_at_epoch_ms"),
        last_failure_at_epoch_ms: row.get("last_failure_at_epoch_ms"),
    })
}

fn log_from_row(row: Row) -> Result<LogRecord, String> {
    Ok(LogRecord {
        id: row.get("id"),
        run_id: row.get("run_id"),
        attempt_id: row.get("attempt_id"),
        stream: row.get("stream"),
        message: row.get("message"),
    })
}

fn deploy_status_to_str(value: DeployStatus) -> &'static str {
    match value {
        DeployStatus::Uploading => "uploading",
        DeployStatus::Ready => "ready",
        DeployStatus::Failed => "failed",
    }
}

fn deploy_status_from_str(value: String) -> Result<DeployStatus, String> {
    match value.as_str() {
        "uploading" => Ok(DeployStatus::Uploading),
        "ready" => Ok(DeployStatus::Ready),
        "failed" => Ok(DeployStatus::Failed),
        _ => Err(format!("unknown deploy status: {value}")),
    }
}

fn trigger_type_to_str(value: TriggerType) -> &'static str {
    match value {
        TriggerType::Enqueue => "enqueue",
        TriggerType::Schedule => "schedule",
        TriggerType::Replay => "replay",
        TriggerType::Backfill => "backfill",
    }
}

fn trigger_type_from_str(value: String) -> Result<TriggerType, String> {
    match value.as_str() {
        "enqueue" => Ok(TriggerType::Enqueue),
        "schedule" => Ok(TriggerType::Schedule),
        "replay" => Ok(TriggerType::Replay),
        "backfill" => Ok(TriggerType::Backfill),
        _ => Err(format!("unknown trigger type: {value}")),
    }
}

fn run_status_to_str(value: RunStatus) -> &'static str {
    match value {
        RunStatus::Queued => "queued",
        RunStatus::Running => "running",
        RunStatus::Succeeded => "succeeded",
        RunStatus::Failed => "failed",
        RunStatus::TimedOut => "timed_out",
        RunStatus::Canceled => "canceled",
    }
}

fn run_status_from_str(value: String) -> Result<RunStatus, String> {
    match value.as_str() {
        "queued" => Ok(RunStatus::Queued),
        "running" => Ok(RunStatus::Running),
        "succeeded" => Ok(RunStatus::Succeeded),
        "failed" => Ok(RunStatus::Failed),
        "timed_out" => Ok(RunStatus::TimedOut),
        "canceled" => Ok(RunStatus::Canceled),
        _ => Err(format!("unknown run status: {value}")),
    }
}

fn attempt_status_to_str(value: AttemptStatus) -> &'static str {
    match value {
        AttemptStatus::Queued => "queued",
        AttemptStatus::Leased => "leased",
        AttemptStatus::Running => "running",
        AttemptStatus::Succeeded => "succeeded",
        AttemptStatus::Failed => "failed",
        AttemptStatus::TimedOut => "timed_out",
        AttemptStatus::Canceled => "canceled",
    }
}

fn attempt_status_from_str(value: String) -> Result<AttemptStatus, String> {
    match value.as_str() {
        "queued" => Ok(AttemptStatus::Queued),
        "leased" => Ok(AttemptStatus::Leased),
        "running" => Ok(AttemptStatus::Running),
        "succeeded" => Ok(AttemptStatus::Succeeded),
        "failed" => Ok(AttemptStatus::Failed),
        "timed_out" => Ok(AttemptStatus::TimedOut),
        "canceled" => Ok(AttemptStatus::Canceled),
        _ => Err(format!("unknown attempt status: {value}")),
    }
}

fn provider_check_status_to_str(value: ProviderCheckStatus) -> &'static str {
    match value {
        ProviderCheckStatus::Success => "success",
        ProviderCheckStatus::Failure => "failure",
    }
}

fn provider_check_status_from_str(value: String) -> Result<ProviderCheckStatus, String> {
    match value.as_str() {
        "success" => Ok(ProviderCheckStatus::Success),
        "failure" => Ok(ProviderCheckStatus::Failure),
        _ => Err(format!("unknown provider check status: {value}")),
    }
}

fn provider_health_state_to_str(value: ProviderHealthState) -> &'static str {
    match value {
        ProviderHealthState::Healthy => "healthy",
        ProviderHealthState::Degraded => "degraded",
        ProviderHealthState::Down => "down",
    }
}

fn provider_health_state_from_str(value: String) -> Result<ProviderHealthState, String> {
    match value.as_str() {
        "healthy" => Ok(ProviderHealthState::Healthy),
        "degraded" => Ok(ProviderHealthState::Degraded),
        "down" => Ok(ProviderHealthState::Down),
        _ => Err(format!("unknown provider health state: {value}")),
    }
}

fn function_health_state_to_str(value: FunctionHealthState) -> &'static str {
    match value {
        FunctionHealthState::Healthy => "healthy",
        FunctionHealthState::Degraded => "degraded",
        FunctionHealthState::Down => "down",
    }
}

fn function_health_state_from_str(value: String) -> Result<FunctionHealthState, String> {
    match value.as_str() {
        "healthy" => Ok(FunctionHealthState::Healthy),
        "degraded" => Ok(FunctionHealthState::Degraded),
        "down" => Ok(FunctionHealthState::Down),
        _ => Err(format!("unknown function health state: {value}")),
    }
}

fn now_epoch_ms() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis() as i64,
        Err(_) => 0,
    }
}

fn timestamp_after_seconds(seconds: u64) -> i64 {
    now_epoch_ms() + (seconds as i64 * 1000)
}

fn is_terminal_attempt(status: AttemptStatus) -> bool {
    matches!(
        status,
        AttemptStatus::Succeeded
            | AttemptStatus::Failed
            | AttemptStatus::TimedOut
            | AttemptStatus::Canceled
    )
}
