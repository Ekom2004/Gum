use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use gum_api::routes::{
    AppendLogRequest, CompleteAttemptRequest, EnqueueRunRequest, LeaseRunResponse,
    RegisterDeployRequest, RegisterRunnerRequest, RegisteredJob,
};
use gum_api::service;
use gum_runner::execution::{execute_leased_run, execute_leased_run_with_cancel};
use gum_runner::runner_loop::LeasedRun;
use gum_store::memory::MemoryStore;
use gum_store::models::ProjectRecord;
use gum_store::queries::GumStore;
use gum_types::{AttemptStatus, RunStatus};
use serde_json::json;
use tokio::sync::watch;

#[tokio::test]
async fn timed_out_execution_marks_run_timed_out_and_keeps_logs() {
    let store = MemoryStore::default();
    store
        .insert_project(ProjectRecord {
            id: "proj_1".to_string(),
            name: "Acme".to_string(),
            slug: "acme".to_string(),
            api_key_hash: "hash".to_string(),
        })
        .expect("project insert should work");

    let bundle_path = create_bundle(
        "jobs.py",
        r#"
import time

def slow_job():
    print("starting slow job", flush=True)
    time.sleep(2)
"#,
    );

    service::register_deploy(
        &store,
        RegisterDeployRequest {
            project_id: "proj_1".to_string(),
            version: "v1".to_string(),
            bundle_url: format!("file://{}", bundle_path.display()),
            bundle_sha256: "abc123".to_string(),
            sdk_language: "python".to_string(),
            entrypoint: "jobs.py".to_string(),
            jobs: vec![RegisteredJob {
                id: "job_slow".to_string(),
                name: "slow_job".to_string(),
                handler_ref: "jobs:slow_job".to_string(),
                trigger_mode: "manual".to_string(),
                schedule_expr: None,
                retries: 0,
                timeout_secs: 1,
                rate_limit_spec: None,
                concurrency_limit: None,
                compute_class: None,
            }],
        },
    )
    .expect("register deploy should work");
    service::register_runner(
        &store,
        RegisterRunnerRequest {
            runner_id: "runner_1".to_string(),
            compute_class: "standard".to_string(),
            max_concurrent_leases: 1,
            heartbeat_timeout_secs: 30,
        },
    )
    .expect("register runner should work");

    let run = service::enqueue_run(
        &store,
        "proj_1",
        "job_slow",
        EnqueueRunRequest { input: json!({}) },
    )
    .expect("enqueue should work");

    let leased = service::lease_run(
        &store,
        gum_api::routes::LeaseRunRequest {
            runner_id: "runner_1".to_string(),
            lease_ttl_secs: 30,
        },
    )
    .expect("lease should work")
    .expect("run should be leased");

    let outcome = execute_leased_run(&leased_run_from_response(&leased)).await;
    assert_eq!(outcome.status, AttemptStatus::TimedOut);
    assert_eq!(
        outcome.failure_reason.as_deref(),
        Some("job timed out after 1s")
    );

    append_output_logs(&store, &leased, &outcome.stdout, &outcome.stderr);

    let completed = service::complete_attempt(
        &store,
        &leased.attempt_id,
        CompleteAttemptRequest {
            runner_id: "runner_1".to_string(),
            status: outcome.status,
            failure_reason: outcome.failure_reason.clone(),
            failure_class: outcome.failure_class.clone(),
        },
    )
    .expect("completion should work");

    assert_eq!(completed.status, RunStatus::TimedOut);
    assert_eq!(completed.failure_reason, outcome.failure_reason);

    let logs = service::get_logs(&store, &run.id).expect("logs should load");
    assert!(
        logs.iter().all(|line| line.attempt_id == leased.attempt_id),
        "timed out attempt logs should stay queryable after completion"
    );

    let run_record = store
        .get_run(&run.id)
        .expect("run lookup should work")
        .expect("run should exist");
    assert_eq!(run_record.status, RunStatus::TimedOut);
    assert_eq!(
        run_record.failure_reason.as_deref(),
        Some("job timed out after 1s")
    );
}

#[tokio::test]
async fn canceled_execution_marks_run_canceled() {
    let store = MemoryStore::default();
    store
        .insert_project(ProjectRecord {
            id: "proj_1".to_string(),
            name: "Acme".to_string(),
            slug: "acme".to_string(),
            api_key_hash: "hash".to_string(),
        })
        .expect("project insert should work");

    let bundle_path = create_bundle(
        "jobs.py",
        r#"
import time

def slow_job():
    print("starting cancelable job", flush=True)
    time.sleep(5)
"#,
    );

    service::register_deploy(
        &store,
        RegisterDeployRequest {
            project_id: "proj_1".to_string(),
            version: "v1".to_string(),
            bundle_url: format!("file://{}", bundle_path.display()),
            bundle_sha256: "abc123".to_string(),
            sdk_language: "python".to_string(),
            entrypoint: "jobs.py".to_string(),
            jobs: vec![RegisteredJob {
                id: "job_cancel".to_string(),
                name: "slow_job".to_string(),
                handler_ref: "jobs:slow_job".to_string(),
                trigger_mode: "manual".to_string(),
                schedule_expr: None,
                retries: 0,
                timeout_secs: 10,
                rate_limit_spec: None,
                concurrency_limit: None,
                compute_class: None,
            }],
        },
    )
    .expect("register deploy should work");
    service::register_runner(
        &store,
        RegisterRunnerRequest {
            runner_id: "runner_1".to_string(),
            compute_class: "standard".to_string(),
            max_concurrent_leases: 1,
            heartbeat_timeout_secs: 30,
        },
    )
    .expect("register runner should work");

    let run = service::enqueue_run(
        &store,
        "proj_1",
        "job_cancel",
        EnqueueRunRequest { input: json!({}) },
    )
    .expect("enqueue should work");

    let leased = service::lease_run(
        &store,
        gum_api::routes::LeaseRunRequest {
            runner_id: "runner_1".to_string(),
            lease_ttl_secs: 30,
        },
    )
    .expect("lease should work")
    .expect("run should be leased");

    let (cancel_tx, cancel_rx) = watch::channel(false);
    let cancel_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let _ = cancel_tx.send(true);
    });

    let outcome =
        execute_leased_run_with_cancel(&leased_run_from_response(&leased), cancel_rx).await;
    cancel_task.await.expect("cancel task should join");

    assert_eq!(outcome.status, AttemptStatus::Canceled);
    assert_eq!(outcome.failure_reason.as_deref(), Some("job canceled"));

    append_output_logs(&store, &leased, &outcome.stdout, &outcome.stderr);

    let completed = service::complete_attempt(
        &store,
        &leased.attempt_id,
        CompleteAttemptRequest {
            runner_id: "runner_1".to_string(),
            status: outcome.status,
            failure_reason: outcome.failure_reason.clone(),
            failure_class: outcome.failure_class.clone(),
        },
    )
    .expect("completion should work");

    assert_eq!(completed.status, RunStatus::Canceled);
    assert_eq!(completed.failure_reason.as_deref(), Some("job canceled"));

    let run_record = store
        .get_run(&run.id)
        .expect("run lookup should work")
        .expect("run should exist");
    assert_eq!(run_record.status, RunStatus::Canceled);
}

fn append_output_logs(store: &MemoryStore, leased: &LeaseRunResponse, stdout: &str, stderr: &str) {
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }

        service::append_log(
            store,
            &leased.run_id,
            &leased.attempt_id,
            AppendLogRequest {
                stream: "stdout".to_string(),
                message: line.to_string(),
            },
        )
        .expect("stdout append should work");
    }

    for line in stderr.lines() {
        if line.trim().is_empty() {
            continue;
        }

        service::append_log(
            store,
            &leased.run_id,
            &leased.attempt_id,
            AppendLogRequest {
                stream: "stderr".to_string(),
                message: line.to_string(),
            },
        )
        .expect("stderr append should work");
    }
}

fn leased_run_from_response(response: &LeaseRunResponse) -> LeasedRun {
    LeasedRun {
        lease_id: response.lease_id.clone(),
        attempt_id: response.attempt_id.clone(),
        run_id: response.run_id.clone(),
        job_id: response.job_id.clone(),
        deploy_id: response.deploy_id.clone(),
        bundle_url: response.bundle_url.clone(),
        entrypoint: response.entrypoint.clone(),
        handler_ref: response.handler_ref.clone(),
        timeout_secs: response.timeout_secs,
        input: response.input.clone(),
    }
}

fn create_bundle(entrypoint: &str, source: &str) -> PathBuf {
    let base = unique_temp_dir("gum-runner-timeout-test");
    fs::create_dir_all(&base).expect("temp source dir should be created");

    let source_path = base.join(entrypoint);
    fs::write(&source_path, source).expect("job source should be written");

    let bundle_path = base.join("bundle.tar.gz");
    let status = Command::new("tar")
        .arg("-czf")
        .arg(&bundle_path)
        .arg("-C")
        .arg(&base)
        .arg(entrypoint)
        .status()
        .expect("tar should start");
    assert!(status.success(), "tar should produce a bundle");

    bundle_path
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let suffix = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(_) => 0,
    };
    std::env::temp_dir().join(format!("{prefix}-{suffix}"))
}
