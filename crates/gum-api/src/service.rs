use gum_store::models::LogRecord;
use gum_store::queries::{
    CompleteAttemptParams, EnqueueRunParams, GumStore, LeaseNextAttemptParams, RegisterDeployParams,
    RegisterJobParams, ReplayRunParams,
};
use gum_types::AttemptStatus;

use crate::routes::{
    AppendLogRequest, CompleteAttemptRequest, EnqueueRunRequest, LeaseRunRequest, LeaseRunResponse,
    LogLine, RegisterDeployRequest, RegisterDeployResponse, ReplayRunResponse, RunResponse,
};

pub fn register_deploy<S: GumStore>(
    store: &S,
    request: RegisterDeployRequest,
) -> Result<RegisterDeployResponse, String> {
    let params = RegisterDeployParams {
        project_id: request.project_id,
        version: request.version,
        bundle_url: request.bundle_url,
        bundle_sha256: request.bundle_sha256,
        sdk_language: request.sdk_language,
        entrypoint: request.entrypoint,
        jobs: request
            .jobs
            .into_iter()
            .map(|job| RegisterJobParams {
                id: job.id,
                name: job.name,
                handler_ref: job.handler_ref,
                trigger_mode: job.trigger_mode,
                schedule_expr: job.schedule_expr,
                retries: job.retries,
                timeout_secs: job.timeout_secs,
                rate_limit_spec: job.rate_limit_spec,
                concurrency_limit: job.concurrency_limit,
            })
            .collect(),
    };

    let (deploy, jobs) = store.register_deploy(params)?;
    Ok(RegisterDeployResponse {
        id: deploy.id,
        registered_jobs: jobs.len(),
    })
}

pub fn enqueue_run<S: GumStore>(
    store: &S,
    project_id: &str,
    job_id: &str,
    request: EnqueueRunRequest,
) -> Result<RunResponse, String> {
    let job = store
        .get_job(job_id)?
        .ok_or_else(|| "job not found".to_string())?;
    let run = store.enqueue_run(EnqueueRunParams {
        project_id: project_id.to_string(),
        job_id: job_id.to_string(),
        deploy_id: job.deploy_id,
        input_json: request.input,
    })?;
    Ok(run_response(run))
}

pub fn get_run<S: GumStore>(store: &S, run_id: &str) -> Result<Option<RunResponse>, String> {
    Ok(store.get_run(run_id)?.map(run_response))
}

pub fn replay_run<S: GumStore>(store: &S, run_id: &str) -> Result<ReplayRunResponse, String> {
    let replay = store.replay_run(ReplayRunParams {
        source_run_id: run_id.to_string(),
    })?;
    let replay_of = replay
        .replay_of_run_id
        .clone()
        .ok_or_else(|| "replayed run missing source lineage".to_string())?;
    Ok(ReplayRunResponse {
        id: replay.id,
        status: replay.status,
        replay_of,
    })
}

pub fn get_logs<S: GumStore>(store: &S, run_id: &str) -> Result<Vec<LogLine>, String> {
    Ok(store
        .list_run_logs(run_id)?
        .into_iter()
        .map(|entry| LogLine {
            attempt_id: entry.attempt_id,
            stream: entry.stream,
            message: entry.message,
        })
        .collect())
}

pub fn tick_schedules<S: GumStore>(store: &S, now_epoch_ms: i64) -> Result<Vec<RunResponse>, String> {
    Ok(store
        .tick_schedules(now_epoch_ms)?
        .into_iter()
        .map(run_response)
        .collect())
}

pub fn lease_run<S: GumStore>(
    store: &S,
    request: LeaseRunRequest,
) -> Result<Option<LeaseRunResponse>, String> {
    let Some((run, attempt, lease)) = store.lease_next_attempt(LeaseNextAttemptParams {
        runner_id: request.runner_id,
        lease_ttl_secs: 30,
    })?
    else {
        return Ok(None);
    };

    let deploy = store
        .get_deploy(&run.deploy_id)?
        .ok_or_else(|| "deploy not found".to_string())?;
    let job = store
        .get_job(&run.job_id)?
        .ok_or_else(|| "job not found".to_string())?;

    Ok(Some(LeaseRunResponse {
        lease_id: lease.id,
        attempt_id: attempt.id,
        run_id: run.id,
        job_id: run.job_id,
        deploy_id: run.deploy_id,
        input: run.input_json,
        bundle_url: deploy.bundle_url,
        entrypoint: deploy.entrypoint,
        handler_ref: job.handler_ref,
        timeout_secs: job.timeout_secs,
        lease_ttl_secs: 30,
    }))
}

pub fn complete_attempt<S: GumStore>(
    store: &S,
    attempt_id: &str,
    request: CompleteAttemptRequest,
) -> Result<RunResponse, String> {
    let (_, run) = store.complete_attempt(CompleteAttemptParams {
        attempt_id: attempt_id.to_string(),
        runner_id: request.runner_id,
        status: request.status,
        failure_reason: request.failure_reason,
    })?;
    Ok(run_response(run))
}

pub fn append_log<S: GumStore>(
    store: &S,
    run_id: &str,
    attempt_id: &str,
    request: AppendLogRequest,
) -> Result<(), String> {
    store.append_log(LogRecord {
        id: format!(
            "log_{run_id}_{attempt_id}_{}_{}",
            request.stream,
            message_fingerprint(&request.message)
        ),
        run_id: run_id.to_string(),
        attempt_id: attempt_id.to_string(),
        stream: request.stream,
        message: request.message,
    })
}

fn run_response(run: gum_store::models::RunRecord) -> RunResponse {
    RunResponse {
        id: run.id,
        job_id: run.job_id,
        status: run.status,
        attempt: run.attempt_count,
        failure_reason: run.failure_reason,
        replay_of: run.replay_of_run_id,
    }
}

pub fn is_terminal_attempt(status: AttemptStatus) -> bool {
    matches!(
        status,
        AttemptStatus::Succeeded
            | AttemptStatus::Failed
            | AttemptStatus::TimedOut
            | AttemptStatus::Canceled
    )
}

fn message_fingerprint(message: &str) -> u64 {
    let mut hash = 1469598103934665603_u64;
    for byte in message.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}
