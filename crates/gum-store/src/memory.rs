use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use gum_types::{AttemptStatus, DeployStatus, RunStatus, TriggerType};

use crate::models::{
    AttemptRecord, ConcurrencyStatusRecord, ControlLeaseRecord, DeployRecord, FunctionHealthRecord,
    FunctionHealthState, JobRecord, LeaseRecord, LeaseStateRecord, LeaseStatusRecord, LogRecord,
    ProjectRecord, ProviderCheckRecord, ProviderCheckStatus, ProviderHealthRecord,
    ProviderHealthState, ProviderTargetRecord, RunRecord, RunnerRecord, RunnerStatusRecord,
};
use crate::queries::{
    compute_retry_disposition, function_health_hold_delay_ms, is_infrastructure_failure_class,
    is_provider_failure_class, parse_rate_limit_spec, parse_schedule_interval_ms,
    provider_slug_from_job, CancelRunParams, CompleteAttemptParams, ControlLeaseParams,
    EnqueueRunParams, GumStore, HeartbeatRunnerParams, LeaseNextAttemptParams,
    RecordProviderCheckParams, RegisterDeployParams, RegisterRunnerParams, ReplayRunParams,
    SetFunctionHealthParams, SetProviderHealthParams, UpsertProviderTargetParams,
};

#[derive(Default)]
struct MemoryState {
    projects: HashMap<String, ProjectRecord>,
    deploys: HashMap<String, DeployRecord>,
    jobs: HashMap<String, JobRecord>,
    runs: HashMap<String, RunRecord>,
    attempts: HashMap<String, AttemptRecord>,
    leases: HashMap<String, LeaseRecord>,
    runners: HashMap<String, RunnerRecord>,
    control_leases: HashMap<String, ControlLeaseRecord>,
    provider_targets: HashMap<String, ProviderTargetRecord>,
    provider_health: HashMap<String, ProviderHealthRecord>,
    provider_checks: Vec<ProviderCheckRecord>,
    function_health: HashMap<String, FunctionHealthRecord>,
    logs: Vec<LogRecord>,
}

#[derive(Clone, Default)]
pub struct MemoryStore {
    ids: Arc<AtomicU64>,
    state: Arc<Mutex<MemoryState>>,
}

impl MemoryStore {
    pub fn insert_project(&self, project: ProjectRecord) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        state.projects.insert(project.id.clone(), project);
        Ok(())
    }

    fn next_id(&self, prefix: &str) -> String {
        let id = self.ids.fetch_add(1, Ordering::Relaxed) + 1;
        format!("{prefix}_{id}")
    }

    fn upsert_runner_locked(
        state: &mut MemoryState,
        runner_id: &str,
        compute_class: &str,
        max_concurrent_leases: u32,
        heartbeat_timeout_secs: u64,
        now_epoch_ms: i64,
    ) -> RunnerRecord {
        let record = RunnerRecord {
            id: runner_id.to_string(),
            compute_class: compute_class.to_string(),
            max_concurrent_leases,
            heartbeat_timeout_secs,
            last_heartbeat_at_epoch_ms: now_epoch_ms,
        };
        state.runners.insert(record.id.clone(), record.clone());
        record
    }

    fn recover_lost_attempts_locked(state: &mut MemoryState, now_epoch_ms: i64) -> Vec<RunRecord> {
        let lost_attempt_ids: Vec<String> = state
            .attempts
            .values()
            .filter(|attempt| attempt.status == AttemptStatus::Running)
            .filter_map(|attempt| {
                let lease_id = attempt.lease_id.as_ref()?;
                let lease = state.leases.get(lease_id)?;
                if lease.acked_at_epoch_ms.is_some() || lease.released_at_epoch_ms.is_some() {
                    return None;
                }

                let lease_expired = lease.expires_at_epoch_ms <= now_epoch_ms;
                let runner_stale = state
                    .runners
                    .get(&lease.runner_id)
                    .map(|runner| {
                        runner
                            .last_heartbeat_at_epoch_ms
                            .saturating_add((runner.heartbeat_timeout_secs as i64) * 1000)
                            <= now_epoch_ms
                    })
                    .unwrap_or(false);

                if lease_expired || runner_stale {
                    Some(attempt.id.clone())
                } else {
                    None
                }
            })
            .collect();

        let mut recovered_runs = Vec::new();

        for attempt_id in lost_attempt_ids {
            let Some(attempt) = state.attempts.get_mut(&attempt_id) else {
                continue;
            };
            if attempt.status != AttemptStatus::Running {
                continue;
            }

            attempt.status = AttemptStatus::Failed;
            attempt.failure_reason = Some("runner lost lease".to_string());
            attempt.failure_class = Some("gum_internal_error".to_string());
            attempt.finished_at_epoch_ms = Some(now_epoch_ms);
            let attempt_snapshot = attempt.clone();

            if let Some(lease_id) = &attempt_snapshot.lease_id {
                if let Some(lease) = state.leases.get_mut(lease_id) {
                    lease.released_at_epoch_ms = Some(now_epoch_ms);
                }
            }

            let Some(run) = state.runs.get_mut(&attempt_snapshot.run_id) else {
                continue;
            };

            if run.attempt_count < run.max_attempts {
                run.status = RunStatus::Queued;
                run.failure_reason = None;
                run.failure_class = None;
                run.retry_after_epoch_ms = None;
                run.waiting_for_provider_slug = None;
            } else {
                run.status = RunStatus::Failed;
                run.failure_reason = Some("runner lost lease".to_string());
                run.failure_class = Some("gum_internal_error".to_string());
                run.retry_after_epoch_ms = None;
                run.waiting_for_provider_slug = None;
            }

            recovered_runs.push(run.clone());
        }

        recovered_runs
    }

    fn apply_provider_signal_locked(
        &self,
        state: &mut MemoryState,
        provider_slug: &str,
        signal_status: ProviderCheckStatus,
        error_class: Option<&str>,
        now_epoch_ms: i64,
    ) {
        let Some(target) = state
            .provider_targets
            .values()
            .find(|candidate| candidate.slug == provider_slug)
            .cloned()
        else {
            return;
        };

        let check = ProviderCheckRecord {
            id: self.next_id("pcheck"),
            provider_target_id: target.id.clone(),
            status: signal_status,
            latency_ms: None,
            error_class: error_class.map(ToString::to_string),
            status_code: None,
            checked_at_epoch_ms: now_epoch_ms,
        };
        state.provider_checks.push(check);

        let previous = state.provider_health.get(&target.id).cloned();
        let (
            state_name,
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
                let previous_down = previous
                    .as_ref()
                    .map(|record| record.down_score)
                    .unwrap_or(0);
                let next_down = (previous_down + 1).clamp(0, 10);
                let next_state = if next_down >= 3 {
                    ProviderHealthState::Down
                } else {
                    ProviderHealthState::Degraded
                };
                (
                    next_state,
                    Some(
                        error_class
                            .map(ToString::to_string)
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
            .map(|record| record.state == state_name)
            .unwrap_or(false)
        {
            previous
                .as_ref()
                .map(|record| record.last_changed_at_epoch_ms)
                .unwrap_or(now_epoch_ms)
        } else {
            now_epoch_ms
        };

        state.provider_health.insert(
            target.id.clone(),
            ProviderHealthRecord {
                provider_target_id: target.id,
                provider_name: target.name,
                provider_slug: target.slug,
                state: state_name,
                reason,
                last_changed_at_epoch_ms,
                last_success_at_epoch_ms,
                last_failure_at_epoch_ms,
                degraded_score,
                down_score,
            },
        );
    }

    fn function_health_for_job(state: &MemoryState, job_id: &str) -> Option<FunctionHealthRecord> {
        state.function_health.get(job_id).cloned()
    }

    fn apply_function_signal_locked(
        state: &mut MemoryState,
        job_id: &str,
        failure_class: Option<&str>,
        attempt_status: AttemptStatus,
        now_epoch_ms: i64,
    ) {
        let previous = state.function_health.get(job_id).cloned();
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
                    .and_then(|record| record.last_failure_at_epoch_ms),
            }
        } else if is_infra_failure {
            let consecutive_infra_failures = previous
                .as_ref()
                .map(|record| record.consecutive_infra_failures)
                .unwrap_or(0)
                .saturating_add(1);
            let state_name = if consecutive_infra_failures >= 5 {
                FunctionHealthState::Down
            } else if consecutive_infra_failures >= 3 {
                FunctionHealthState::Degraded
            } else {
                FunctionHealthState::Healthy
            };
            let hold_until_epoch_ms = if matches!(
                state_name,
                FunctionHealthState::Degraded | FunctionHealthState::Down
            ) {
                Some(now_epoch_ms + function_health_hold_delay_ms())
            } else {
                None
            };
            FunctionHealthRecord {
                job_id: job_id.to_string(),
                state: state_name,
                consecutive_infra_failures,
                reason: Some(
                    failure_class
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "infrastructure failure".to_string()),
                ),
                hold_until_epoch_ms,
                last_changed_at_epoch_ms: if previous
                    .as_ref()
                    .map(|record| record.state == state_name)
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

        state.function_health.insert(job_id.to_string(), next);
    }
}

impl GumStore for MemoryStore {
    fn register_deploy(
        &self,
        params: RegisterDeployParams,
    ) -> Result<(DeployRecord, Vec<JobRecord>), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        if !state.projects.contains_key(&params.project_id) {
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
                compute_class: job.compute_class,
                enabled: true,
                created_at_epoch_ms,
            };
            state.jobs.insert(record.id.clone(), record.clone());
            jobs.push(record);
        }

        state.deploys.insert(deploy.id.clone(), deploy.clone());
        Ok((deploy, jobs))
    }

    fn upsert_provider_target(
        &self,
        params: UpsertProviderTargetParams,
    ) -> Result<ProviderTargetRecord, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        let created_at_epoch_ms = state
            .provider_targets
            .get(&params.id)
            .map(|record| record.created_at_epoch_ms)
            .unwrap_or_else(now_epoch_ms);
        let record = ProviderTargetRecord {
            id: params.id,
            name: params.name,
            slug: params.slug,
            probe_kind: params.probe_kind,
            probe_config_json: params.probe_config_json,
            enabled: params.enabled,
            created_at_epoch_ms,
        };
        state
            .provider_targets
            .insert(record.id.clone(), record.clone());
        Ok(record)
    }

    fn record_provider_check(
        &self,
        params: RecordProviderCheckParams,
    ) -> Result<ProviderCheckRecord, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        if !state
            .provider_targets
            .contains_key(&params.provider_target_id)
        {
            return Err("provider target not found".to_string());
        }
        let record = ProviderCheckRecord {
            id: self.next_id("pch"),
            provider_target_id: params.provider_target_id,
            status: params.status,
            latency_ms: params.latency_ms,
            error_class: params.error_class,
            status_code: params.status_code,
            checked_at_epoch_ms: params.checked_at_epoch_ms,
        };
        state.provider_checks.push(record.clone());
        Ok(record)
    }

    fn set_provider_health(
        &self,
        params: SetProviderHealthParams,
    ) -> Result<ProviderHealthRecord, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        let target = state
            .provider_targets
            .get(&params.provider_target_id)
            .ok_or_else(|| "provider target not found".to_string())?;
        let record = ProviderHealthRecord {
            provider_target_id: params.provider_target_id,
            provider_name: target.name.clone(),
            provider_slug: target.slug.clone(),
            state: params.state,
            reason: params.reason,
            last_changed_at_epoch_ms: params.last_changed_at_epoch_ms,
            last_success_at_epoch_ms: params.last_success_at_epoch_ms,
            last_failure_at_epoch_ms: params.last_failure_at_epoch_ms,
            degraded_score: params.degraded_score,
            down_score: params.down_score,
        };
        state
            .provider_health
            .insert(record.provider_target_id.clone(), record.clone());
        Ok(record)
    }

    fn set_function_health(
        &self,
        params: SetFunctionHealthParams,
    ) -> Result<FunctionHealthRecord, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        let record = FunctionHealthRecord {
            job_id: params.job_id,
            state: params.state,
            consecutive_infra_failures: params.consecutive_infra_failures,
            reason: params.reason,
            hold_until_epoch_ms: params.hold_until_epoch_ms,
            last_changed_at_epoch_ms: params.last_changed_at_epoch_ms,
            last_success_at_epoch_ms: params.last_success_at_epoch_ms,
            last_failure_at_epoch_ms: params.last_failure_at_epoch_ms,
        };
        state
            .function_health
            .insert(record.job_id.clone(), record.clone());
        Ok(record)
    }

    fn get_function_health(&self, job_id: &str) -> Result<Option<FunctionHealthRecord>, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        Ok(state.function_health.get(job_id).cloned())
    }

    fn list_provider_targets(&self) -> Result<Vec<ProviderTargetRecord>, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        let mut records = state.provider_targets.values().cloned().collect::<Vec<_>>();
        records.sort_by(|left, right| left.slug.cmp(&right.slug));
        Ok(records)
    }

    fn list_provider_health(&self) -> Result<Vec<ProviderHealthRecord>, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        let mut records = state.provider_health.values().cloned().collect::<Vec<_>>();
        records.sort_by(|left, right| left.provider_slug.cmp(&right.provider_slug));
        Ok(records)
    }

    fn register_runner(&self, params: RegisterRunnerParams) -> Result<RunnerRecord, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        Ok(Self::upsert_runner_locked(
            &mut state,
            &params.runner_id,
            &params.compute_class,
            params.max_concurrent_leases,
            params.heartbeat_timeout_secs,
            now_epoch_ms(),
        ))
    }

    fn heartbeat_runner(&self, params: HeartbeatRunnerParams) -> Result<RunnerRecord, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        let heartbeat_at = now_epoch_ms();
        let runner = Self::upsert_runner_locked(
            &mut state,
            &params.runner_id,
            &params.compute_class,
            params.max_concurrent_leases,
            params.heartbeat_timeout_secs,
            heartbeat_at,
        );

        for lease_id in params.active_lease_ids {
            let lease = state
                .leases
                .get_mut(&lease_id)
                .ok_or_else(|| format!("lease not found: {lease_id}"))?;
            if lease.runner_id != params.runner_id {
                return Err(format!("runner does not own lease: {lease_id}"));
            }
            if lease.acked_at_epoch_ms.is_some() || lease.released_at_epoch_ms.is_some() {
                continue;
            }

            lease.expires_at_epoch_ms =
                heartbeat_at.saturating_add((params.lease_ttl_secs as i64) * 1000);
        }

        Ok(runner)
    }

    fn try_acquire_control_lease(&self, params: ControlLeaseParams) -> Result<bool, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        let can_acquire = match state.control_leases.get(&params.lease_name) {
            Some(existing) => {
                existing.holder_id == params.holder_id
                    || existing.expires_at_epoch_ms <= params.now_epoch_ms
            }
            None => true,
        };

        if !can_acquire {
            return Ok(false);
        }

        state.control_leases.insert(
            params.lease_name.clone(),
            ControlLeaseRecord {
                name: params.lease_name,
                holder_id: params.holder_id,
                expires_at_epoch_ms: params
                    .now_epoch_ms
                    .saturating_add((params.ttl_secs as i64) * 1000),
            },
        );
        Ok(true)
    }

    fn get_job(&self, job_id: &str) -> Result<Option<JobRecord>, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        Ok(state.jobs.get(job_id).cloned())
    }

    fn get_run(&self, run_id: &str) -> Result<Option<RunRecord>, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        Ok(state.runs.get(run_id).cloned())
    }

    fn get_deploy(&self, deploy_id: &str) -> Result<Option<DeployRecord>, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        Ok(state.deploys.get(deploy_id).cloned())
    }

    fn get_lease_state(&self, lease_id: &str) -> Result<Option<LeaseStateRecord>, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        let Some(lease) = state.leases.get(lease_id) else {
            return Ok(None);
        };
        let Some(attempt) = state.attempts.get(&lease.attempt_id) else {
            return Ok(None);
        };
        Ok(Some(LeaseStateRecord {
            lease_id: lease.id.clone(),
            run_id: attempt.run_id.clone(),
            attempt_id: attempt.id.clone(),
            cancel_requested: lease.revoke_requested_at_epoch_ms.is_some()
                || attempt.cancel_requested_at_epoch_ms.is_some(),
        }))
    }

    fn list_recent_runs(&self, limit: usize) -> Result<Vec<RunRecord>, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        let mut runs: Vec<RunRecord> = state.runs.values().cloned().collect();
        runs.sort_by(|left, right| {
            right
                .scheduled_at_epoch_ms
                .cmp(&left.scheduled_at_epoch_ms)
                .then_with(|| right.id.cmp(&left.id))
        });
        if runs.len() > limit {
            runs.truncate(limit);
        }
        Ok(runs)
    }

    fn list_runners(&self) -> Result<Vec<RunnerStatusRecord>, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        let mut runners: Vec<RunnerStatusRecord> = state
            .runners
            .values()
            .map(|runner| RunnerStatusRecord {
                id: runner.id.clone(),
                compute_class: runner.compute_class.clone(),
                max_concurrent_leases: runner.max_concurrent_leases,
                last_heartbeat_at_epoch_ms: runner.last_heartbeat_at_epoch_ms,
                active_lease_count: state
                    .attempts
                    .values()
                    .filter(|attempt| attempt.status == AttemptStatus::Running)
                    .filter(|attempt| attempt.runner_id.as_deref() == Some(runner.id.as_str()))
                    .count() as u32,
            })
            .collect();
        runners.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(runners)
    }

    fn list_active_leases(&self) -> Result<Vec<LeaseStatusRecord>, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        let mut leases: Vec<LeaseStatusRecord> = state
            .leases
            .values()
            .filter(|lease| {
                lease.acked_at_epoch_ms.is_none() && lease.released_at_epoch_ms.is_none()
            })
            .filter_map(|lease| {
                let attempt = state.attempts.get(&lease.attempt_id)?;
                Some(LeaseStatusRecord {
                    lease_id: lease.id.clone(),
                    run_id: attempt.run_id.clone(),
                    attempt_id: attempt.id.clone(),
                    runner_id: lease.runner_id.clone(),
                    expires_at_epoch_ms: lease.expires_at_epoch_ms,
                    cancel_requested: lease.revoke_requested_at_epoch_ms.is_some()
                        || attempt.cancel_requested_at_epoch_ms.is_some(),
                })
            })
            .collect();
        leases.sort_by(|left, right| {
            left.expires_at_epoch_ms
                .cmp(&right.expires_at_epoch_ms)
                .then_with(|| left.lease_id.cmp(&right.lease_id))
        });
        Ok(leases)
    }

    fn list_concurrency_status(&self) -> Result<Vec<ConcurrencyStatusRecord>, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        let mut statuses: Vec<ConcurrencyStatusRecord> = state
            .jobs
            .values()
            .filter_map(|job| {
                let concurrency_limit = job.concurrency_limit?;

                let mut active_run_ids: Vec<String> = state
                    .attempts
                    .values()
                    .filter(|attempt| attempt.status == AttemptStatus::Running)
                    .filter_map(|attempt| state.runs.get(&attempt.run_id))
                    .filter(|run| run.job_id == job.id)
                    .map(|run| run.id.clone())
                    .collect();
                active_run_ids.sort();
                active_run_ids.dedup();

                let mut queued_run_ids: Vec<String> = state
                    .runs
                    .values()
                    .filter(|run| run.job_id == job.id && run.status == RunStatus::Queued)
                    .map(|run| run.id.clone())
                    .collect();
                queued_run_ids.sort();

                Some(ConcurrencyStatusRecord {
                    job_id: job.id.clone(),
                    job_name: job.name.clone(),
                    concurrency_limit,
                    active_run_ids,
                    queued_run_ids,
                })
            })
            .collect();
        statuses.sort_by(|left, right| {
            left.job_name
                .cmp(&right.job_name)
                .then_with(|| left.job_id.cmp(&right.job_id))
        });
        Ok(statuses)
    }

    fn enqueue_run(&self, params: EnqueueRunParams) -> Result<RunRecord, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        let job = state
            .jobs
            .get(&params.job_id)
            .cloned()
            .ok_or_else(|| "job not found".to_string())?;
        if !job.enabled {
            return Err("job disabled".to_string());
        }
        if job.project_id != params.project_id || job.deploy_id != params.deploy_id {
            return Err("job/project/deploy mismatch".to_string());
        }

        let run = RunRecord {
            id: self.next_id("run"),
            project_id: params.project_id,
            job_id: params.job_id,
            deploy_id: params.deploy_id,
            trigger_type: TriggerType::Enqueue,
            status: RunStatus::Queued,
            input_json: params.input_json,
            attempt_count: 0,
            max_attempts: job.retries + 1,
            scheduled_at_epoch_ms: now_epoch_ms(),
            failure_reason: None,
            failure_class: None,
            retry_after_epoch_ms: None,
            waiting_for_provider_slug: None,
            replay_of_run_id: None,
        };
        state.runs.insert(run.id.clone(), run.clone());
        Ok(run)
    }

    fn replay_run(&self, params: ReplayRunParams) -> Result<RunRecord, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        let source = state
            .runs
            .get(&params.source_run_id)
            .cloned()
            .ok_or_else(|| "run not found".to_string())?;

        let replay = RunRecord {
            id: self.next_id("run"),
            project_id: source.project_id.clone(),
            job_id: source.job_id.clone(),
            deploy_id: source.deploy_id.clone(),
            trigger_type: TriggerType::Replay,
            status: RunStatus::Queued,
            input_json: source.input_json.clone(),
            attempt_count: 0,
            max_attempts: source.max_attempts,
            scheduled_at_epoch_ms: now_epoch_ms(),
            failure_reason: None,
            failure_class: None,
            retry_after_epoch_ms: None,
            waiting_for_provider_slug: None,
            replay_of_run_id: Some(source.id),
        };
        state.runs.insert(replay.id.clone(), replay.clone());
        Ok(replay)
    }

    fn lease_next_attempt(
        &self,
        params: LeaseNextAttemptParams,
    ) -> Result<Option<(RunRecord, AttemptRecord, LeaseRecord)>, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        Self::recover_lost_attempts_locked(&mut state, now_epoch_ms());
        let runner = state
            .runners
            .get(&params.runner_id)
            .cloned()
            .ok_or_else(|| "runner not registered".to_string())?;
        let runner_active_leases = state
            .attempts
            .values()
            .filter(|attempt| attempt.status == AttemptStatus::Running)
            .filter(|attempt| attempt.runner_id.as_deref() == Some(params.runner_id.as_str()))
            .count() as u32;
        if runner_active_leases >= runner.max_concurrent_leases {
            return Ok(None);
        }

        let mut selected: Option<RunRecord> = None;
        for run in state.runs.values() {
            if run.status != RunStatus::Queued {
                continue;
            }
            if run
                .retry_after_epoch_ms
                .map(|retry_after| retry_after > now_epoch_ms())
                .unwrap_or(false)
            {
                continue;
            }
            let job = match state.jobs.get(&run.job_id) {
                Some(job) if job.enabled => job,
                _ => continue,
            };
            if state
                .function_health
                .get(&job.id)
                .and_then(|health| health.hold_until_epoch_ms)
                .map(|hold_until| hold_until > now_epoch_ms())
                .unwrap_or(false)
            {
                continue;
            }

            if let Some(required_class) = &job.compute_class {
                if runner.compute_class != *required_class {
                    continue;
                }
            }

            if let Some(limit) = job.concurrency_limit {
                let active = state
                    .attempts
                    .values()
                    .filter(|attempt| {
                        attempt.status == AttemptStatus::Running
                            && state
                                .runs
                                .get(&attempt.run_id)
                                .map(|run| run.job_id == job.id)
                                .unwrap_or(false)
                    })
                    .count() as u32;
                if active >= limit {
                    continue;
                }
            }

            if let Some(rate_limit_spec) = &job.rate_limit_spec {
                let spec = parse_rate_limit_spec(rate_limit_spec)?;
                let window_start_ms = now_epoch_ms().saturating_sub(spec.window_ms);
                let recent_starts = state
                    .attempts
                    .values()
                    .filter(|attempt| attempt.started_at_epoch_ms >= window_start_ms)
                    .filter(|attempt| {
                        let Some(run) = state.runs.get(&attempt.run_id) else {
                            return false;
                        };

                        if spec.pool.is_some() {
                            let Some(run_job) = state.jobs.get(&run.job_id) else {
                                return false;
                            };
                            run_job.project_id == job.project_id
                                && run_job.rate_limit_spec.as_deref()
                                    == Some(rate_limit_spec.as_str())
                        } else {
                            run.job_id == job.id
                        }
                    })
                    .count() as u32;
                if recent_starts >= spec.limit {
                    continue;
                }
            }

            selected = Some(run.clone());
            break;
        }

        let Some(run) = selected else {
            return Ok(None);
        };

        let attempt = AttemptRecord {
            id: self.next_id("att"),
            run_id: run.id.clone(),
            attempt_number: run.attempt_count + 1,
            status: AttemptStatus::Running,
            lease_id: None,
            runner_id: Some(params.runner_id.clone()),
            started_at_epoch_ms: now_epoch_ms(),
            finished_at_epoch_ms: None,
            failure_reason: None,
            failure_class: None,
            cancel_requested_at_epoch_ms: None,
        };

        let lease = LeaseRecord {
            id: self.next_id("lease"),
            attempt_id: attempt.id.clone(),
            runner_id: params.runner_id,
            expires_at_epoch_ms: now_epoch_ms() + (params.lease_ttl_secs as i64 * 1000),
            acked_at_epoch_ms: None,
            released_at_epoch_ms: None,
            revoke_requested_at_epoch_ms: None,
        };

        let mut leased_attempt = attempt.clone();
        leased_attempt.lease_id = Some(lease.id.clone());

        let leased_run = match state.runs.get_mut(&run.id) {
            Some(record) => {
                record.status = RunStatus::Running;
                record.attempt_count += 1;
                record.failure_reason = None;
                record.failure_class = None;
                record.retry_after_epoch_ms = None;
                record.waiting_for_provider_slug = None;
                record.clone()
            }
            None => return Err("selected run disappeared before lease".to_string()),
        };

        state
            .attempts
            .insert(leased_attempt.id.clone(), leased_attempt.clone());
        state.leases.insert(lease.id.clone(), lease.clone());

        Ok(Some((leased_run, leased_attempt, lease)))
    }

    fn complete_attempt(
        &self,
        params: CompleteAttemptParams,
    ) -> Result<(AttemptRecord, RunRecord), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;

        let now_epoch_ms = now_epoch_ms();
        let attempt_snapshot = {
            let attempt = state
                .attempts
                .get_mut(&params.attempt_id)
                .ok_or_else(|| "attempt not found".to_string())?;

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

            attempt.status = params.status;
            attempt.failure_reason = params.failure_reason.clone();
            attempt.failure_class = params.failure_class.clone();
            attempt.finished_at_epoch_ms = Some(now_epoch_ms);
            attempt.clone()
        };

        if let Some(lease_id) = &attempt_snapshot.lease_id {
            if let Some(lease) = state.leases.get_mut(lease_id) {
                lease.acked_at_epoch_ms = Some(now_epoch_ms);
            }
        }

        let run_snapshot = state
            .runs
            .get(&attempt_snapshot.run_id)
            .cloned()
            .ok_or_else(|| "run not found".to_string())?;
        let job = state
            .jobs
            .get(&run_snapshot.job_id)
            .cloned()
            .ok_or_else(|| "job not found".to_string())?;
        let provider_slug = provider_slug_from_job(&job)?;
        Self::apply_function_signal_locked(
            &mut state,
            &job.id,
            params.failure_class.as_deref(),
            params.status,
            now_epoch_ms,
        );

        if let Some(provider_slug) = provider_slug.as_deref() {
            if params.status == AttemptStatus::Succeeded {
                self.apply_provider_signal_locked(
                    &mut state,
                    provider_slug,
                    ProviderCheckStatus::Success,
                    None,
                    now_epoch_ms,
                );
            } else if is_provider_failure_class(params.failure_class.as_deref()) {
                self.apply_provider_signal_locked(
                    &mut state,
                    provider_slug,
                    ProviderCheckStatus::Failure,
                    params.failure_class.as_deref(),
                    now_epoch_ms,
                );
            }
        }

        let function_health = Self::function_health_for_job(&state, &job.id);

        let disposition = compute_retry_disposition(
            &run_snapshot.id,
            run_snapshot.attempt_count,
            run_snapshot.max_attempts,
            params.status,
            params.failure_reason.clone(),
            params.failure_class.clone(),
            function_health.as_ref(),
            now_epoch_ms,
        );

        let run = state
            .runs
            .get_mut(&attempt_snapshot.run_id)
            .ok_or_else(|| "run not found".to_string())?;

        match params.status {
            AttemptStatus::Queued | AttemptStatus::Leased | AttemptStatus::Running => {
                return Err("attempt completion requires terminal status".to_string())
            }
            _ => {
                run.status = disposition.next_status;
                run.failure_reason = disposition.failure_reason;
                run.failure_class = disposition.failure_class;
                run.retry_after_epoch_ms = disposition.retry_after_epoch_ms;
                run.waiting_for_provider_slug = disposition.waiting_for_scope_key;
            }
        }

        Ok((attempt_snapshot, run.clone()))
    }

    fn cancel_run(&self, params: CancelRunParams) -> Result<RunRecord, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;

        let run_status = state
            .runs
            .get(&params.run_id)
            .map(|run| run.status)
            .ok_or_else(|| "run not found".to_string())?;

        match run_status {
            RunStatus::Queued => {
                let run = state
                    .runs
                    .get_mut(&params.run_id)
                    .ok_or_else(|| "run disappeared".to_string())?;
                run.status = RunStatus::Canceled;
                run.failure_reason = Some("canceled".to_string());
                run.failure_class = None;
                run.retry_after_epoch_ms = None;
                run.waiting_for_provider_slug = None;
                Ok(run.clone())
            }
            RunStatus::Running => {
                let running_attempt_id = state
                    .attempts
                    .values()
                    .find(|attempt| {
                        attempt.run_id == params.run_id && attempt.status == AttemptStatus::Running
                    })
                    .map(|attempt| attempt.id.clone())
                    .ok_or_else(|| "running attempt not found".to_string())?;

                let attempt = state
                    .attempts
                    .get_mut(&running_attempt_id)
                    .ok_or_else(|| "running attempt disappeared".to_string())?;
                attempt.cancel_requested_at_epoch_ms = Some(params.requested_at_epoch_ms);
                let lease_id = attempt.lease_id.clone();

                if let Some(lease_id) = lease_id {
                    if let Some(lease) = state.leases.get_mut(&lease_id) {
                        lease.revoke_requested_at_epoch_ms = Some(params.requested_at_epoch_ms);
                    }
                }

                let run = state
                    .runs
                    .get_mut(&params.run_id)
                    .ok_or_else(|| "run disappeared".to_string())?;
                run.failure_reason = Some("cancel requested".to_string());
                run.failure_class = None;
                Ok(run.clone())
            }
            RunStatus::Succeeded
            | RunStatus::Failed
            | RunStatus::TimedOut
            | RunStatus::Canceled => Err("run already finished".to_string()),
        }
    }

    fn recover_lost_attempts(&self, now_epoch_ms: i64) -> Result<Vec<RunRecord>, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        Ok(Self::recover_lost_attempts_locked(&mut state, now_epoch_ms))
    }

    fn tick_schedules(&self, now_epoch_ms: i64) -> Result<Vec<RunRecord>, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        let jobs: Vec<JobRecord> = state
            .jobs
            .values()
            .filter(|job| job.enabled && job.schedule_expr.is_some())
            .cloned()
            .collect();

        let mut created_runs = Vec::new();

        for job in jobs {
            let Some(schedule_expr) = &job.schedule_expr else {
                continue;
            };
            let interval_ms = parse_schedule_interval_ms(schedule_expr)?;

            // Schedules are anchored to the job creation time, then advanced by whole
            // intervals. Looking at the latest scheduled fire time lets the scheduler
            // catch up after restarts without changing that anchor.
            let last_scheduled_ms = state
                .runs
                .values()
                .filter(|run| run.job_id == job.id && run.trigger_type == TriggerType::Schedule)
                .map(|run| run.scheduled_at_epoch_ms)
                .max()
                .unwrap_or(job.created_at_epoch_ms);

            let mut next_due_ms = last_scheduled_ms.saturating_add(interval_ms);
            while next_due_ms <= now_epoch_ms {
                // Dedupe is part of the scheduler contract: if two ticks cover the same
                // fire time, we still want at most one scheduled run for that job+time.
                let already_exists = state.runs.values().any(|run| {
                    run.job_id == job.id
                        && run.trigger_type == TriggerType::Schedule
                        && run.scheduled_at_epoch_ms == next_due_ms
                });

                if !already_exists {
                    let run = RunRecord {
                        id: self.next_id("run"),
                        project_id: job.project_id.clone(),
                        job_id: job.id.clone(),
                        deploy_id: job.deploy_id.clone(),
                        trigger_type: TriggerType::Schedule,
                        status: RunStatus::Queued,
                        input_json: serde_json::json!({}),
                        attempt_count: 0,
                        max_attempts: job.retries + 1,
                        scheduled_at_epoch_ms: next_due_ms,
                        failure_reason: None,
                        failure_class: None,
                        retry_after_epoch_ms: None,
                        waiting_for_provider_slug: None,
                        replay_of_run_id: None,
                    };
                    state.runs.insert(run.id.clone(), run.clone());
                    created_runs.push(run);
                }

                next_due_ms = next_due_ms.saturating_add(interval_ms);
            }
        }

        Ok(created_runs)
    }

    fn append_log(&self, log: LogRecord) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        state.logs.push(log);
        Ok(())
    }

    fn list_run_logs(&self, run_id: &str) -> Result<Vec<LogRecord>, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "memory store lock poisoned".to_string())?;
        Ok(state
            .logs
            .iter()
            .filter(|entry| entry.run_id == run_id)
            .cloned()
            .collect())
    }
}

fn now_epoch_ms() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis() as i64,
        Err(_) => 0,
    }
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
