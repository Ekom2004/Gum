use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use postgres::{Client, NoTls, Row};
use gum_types::{AttemptStatus, DeployStatus, RunStatus, TriggerType};

use crate::models::{
    AttemptRecord, DeployRecord, JobRecord, LeaseRecord, LogRecord, ProjectRecord, RunRecord,
};
use crate::queries::{
    parse_rate_limit_spec, parse_schedule_interval_ms, CompleteAttemptParams, EnqueueRunParams,
    GumStore, LeaseNextAttemptParams, RegisterDeployParams, ReplayRunParams,
};

const MIGRATION_0001: &str = include_str!("../migrations/0001_slice1.sql");
const MIGRATION_0002: &str = include_str!("../migrations/0002_scheduler.sql");

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
            .execute(
                "INSERT INTO projects (id, name, slug, api_key_hash)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (id) DO UPDATE
                 SET name = EXCLUDED.name,
                     slug = EXCLUDED.slug,
                     api_key_hash = EXCLUDED.api_key_hash,
                     updated_at = NOW()",
                &[&project.id, &project.name, &project.slug, &project.api_key_hash],
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
            .query_opt("SELECT id FROM projects WHERE id = $1", &[&params.project_id])
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
                enabled: true,
                created_at_epoch_ms,
            };

            tx.execute(
                "INSERT INTO jobs (
                    id, project_id, deploy_id, name, handler_ref, trigger_mode, schedule_expr,
                    retries, timeout_secs, rate_limit_spec, concurrency_limit, enabled
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
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

    fn enqueue_run(&self, params: EnqueueRunParams) -> Result<RunRecord, String> {
        let mut client = self.connect_client()?;
        let row = client
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

        let run_id = self.next_id("run");
        let max_attempts = retries + 1;
        let inserted = client
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

        run_from_row(inserted)
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
        let mut client = self.connect_client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| format!("failed to start lease transaction: {error}"))?;

        let candidate_rows = tx
            .query(
                "SELECT runs.*, (EXTRACT(EPOCH FROM runs.scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms
                 FROM runs
                 JOIN jobs ON jobs.id = runs.job_id
                 WHERE runs.status = 'queued'
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
                &[],
            )
            .map_err(|error| format!("failed to select candidate runs for lease: {error}"))?;

        let mut selected_run: Option<RunRecord> = None;
        for run_row in candidate_rows {
            let run = run_from_row(run_row)?;
            let job_row = tx
                .query_one(
                    "SELECT project_id, rate_limit_spec
                     FROM jobs
                     WHERE id = $1",
                    &[&run.job_id],
                )
                .map_err(|error| format!("failed to load job for rate-limit check: {error}"))?;
            let project_id: String = job_row.get("project_id");
            let rate_limit_spec: Option<String> = job_row.get("rate_limit_spec");

            let allowed = if let Some(rate_limit_spec) = rate_limit_spec {
                let spec = parse_rate_limit_spec(&rate_limit_spec)?;
                let window_start_ms = now_epoch_ms().saturating_sub(spec.window_ms);
                let recent_count_row = if spec.pool.is_some() {
                    tx.query_one(
                        "SELECT COUNT(*) AS count
                         FROM attempts
                         JOIN runs ON runs.id = attempts.run_id
                         JOIN jobs ON jobs.id = runs.job_id
                         WHERE attempts.started_at >= TO_TIMESTAMP($1::double precision / 1000.0)
                           AND jobs.project_id = $2
                           AND jobs.rate_limit_spec = $3",
                        &[&(window_start_ms as f64), &project_id, &rate_limit_spec],
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
                    id, run_id, attempt_number, status, runner_id, started_at
                 ) VALUES ($1, $2, $3, 'running', $4, NOW())
                 RETURNING *,
                           (EXTRACT(EPOCH FROM started_at) * 1000)::bigint AS started_at_epoch_ms,
                           (EXTRACT(EPOCH FROM finished_at) * 1000)::bigint AS finished_at_epoch_ms",
                &[&attempt_id, &run.id, &attempt_number, &params.runner_id],
            )
            .map_err(|error| format!("failed to insert attempt: {error}"))?;

        let lease_row = tx
            .query_one(
                "INSERT INTO leases (id, attempt_id, runner_id, expires_at)
                 VALUES ($1, $2, $3, TO_TIMESTAMP($4::double precision / 1000.0))
                 RETURNING id, attempt_id, runner_id,
                           (EXTRACT(EPOCH FROM expires_at) * 1000)::bigint AS expires_at_epoch_ms,
                           NULL::bigint AS acked_at_epoch_ms,
                           NULL::bigint AS released_at_epoch_ms",
                &[&lease_id, &attempt_id, &params.runner_id, &(expires_at as f64)],
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
                        (EXTRACT(EPOCH FROM finished_at) * 1000)::bigint AS finished_at_epoch_ms
                 FROM attempts
                 WHERE id = $1
                 FOR UPDATE",
                &[&params.attempt_id],
            )
            .map_err(|error| format!("failed to load attempt for completion: {error}"))?
            .ok_or_else(|| "attempt not found".to_string())?;
        let attempt = attempt_from_row(attempt_row)?;

        if attempt.runner_id.as_deref() != Some(params.runner_id.as_str()) {
            return Err("runner mismatch".to_string());
        }

        let updated_attempt_row = tx
            .query_one(
                "UPDATE attempts
                 SET status = $1,
                     failure_reason = $2,
                     finished_at = NOW()
                 WHERE id = $3
                 RETURNING *,
                           (EXTRACT(EPOCH FROM started_at) * 1000)::bigint AS started_at_epoch_ms,
                           (EXTRACT(EPOCH FROM finished_at) * 1000)::bigint AS finished_at_epoch_ms",
                &[
                    &attempt_status_to_str(params.status),
                    &params.failure_reason,
                    &params.attempt_id,
                ],
            )
            .map_err(|error| format!("failed to update attempt: {error}"))?;

        if let Some(lease_id) = &attempt.lease_id {
            tx.execute(
                "UPDATE leases SET acked_at = NOW() WHERE id = $1",
                &[lease_id],
            )
            .map_err(|error| format!("failed to ack lease: {error}"))?;
        }

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

        let (next_status, failure_reason, finished_now) = match params.status {
            AttemptStatus::Succeeded => (RunStatus::Succeeded, None, true),
            AttemptStatus::TimedOut => {
                if run.attempt_count < run.max_attempts {
                    (RunStatus::Queued, None, false)
                } else {
                    (RunStatus::TimedOut, params.failure_reason.clone(), true)
                }
            }
            AttemptStatus::Failed => {
                if run.attempt_count < run.max_attempts {
                    (RunStatus::Queued, None, false)
                } else {
                    (RunStatus::Failed, params.failure_reason.clone(), true)
                }
            }
            AttemptStatus::Canceled => (RunStatus::Canceled, params.failure_reason.clone(), true),
            AttemptStatus::Queued | AttemptStatus::Leased | AttemptStatus::Running => {
                return Err("attempt completion requires terminal status".to_string())
            }
        };

        let updated_run_row = if finished_now {
            tx.query_one(
                "UPDATE runs
                 SET status = $1,
                     failure_reason = $2,
                     finished_at = NOW(),
                     updated_at = NOW()
                 WHERE id = $3
                 RETURNING *, (EXTRACT(EPOCH FROM scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms",
                &[&run_status_to_str(next_status), &failure_reason, &run.id],
            )
            .map_err(|error| format!("failed to finalize run: {error}"))?
        } else {
            tx.query_one(
                "UPDATE runs
                 SET status = $1,
                     failure_reason = NULL,
                     updated_at = NOW()
                 WHERE id = $2
                 RETURNING *, (EXTRACT(EPOCH FROM scheduled_at) * 1000)::bigint AS scheduled_at_epoch_ms",
                &[&run_status_to_str(next_status), &run.id],
            )
            .map_err(|error| format!("failed to requeue run: {error}"))?
        };

        tx.commit()
            .map_err(|error| format!("failed to commit completion transaction: {error}"))?;

        Ok((attempt_from_row(updated_attempt_row)?, run_from_row(updated_run_row)?))
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
                &[&log.id, &log.run_id, &log.attempt_id, &log.stream, &log.message],
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
    })
}

fn lease_from_projection_row(row: Row) -> Result<LeaseRecord, String> {
    Ok(LeaseRecord {
        id: row.get("id"),
        attempt_id: row.get("attempt_id"),
        runner_id: row.get("runner_id"),
        expires_at_epoch_ms: row.get("expires_at_epoch_ms"),
        acked_at_epoch_ms: row.get("acked_at_epoch_ms"),
        released_at_epoch_ms: row.get("released_at_epoch_ms"),
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

fn now_epoch_ms() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis() as i64,
        Err(_) => 0,
    }
}

fn timestamp_after_seconds(seconds: u64) -> i64 {
    now_epoch_ms() + (seconds as i64 * 1000)
}
