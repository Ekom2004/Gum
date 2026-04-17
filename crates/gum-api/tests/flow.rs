use gum_api::routes::{
    AppendLogRequest, CompleteAttemptRequest, EnqueueRunRequest, LeaseRunRequest,
    RegisterDeployRequest, RegisteredJob,
};
use gum_api::service;
use gum_store::memory::MemoryStore;
use gum_store::queries::GumStore;
use gum_store::models::ProjectRecord;
use gum_types::{AttemptStatus, RunStatus};
use serde_json::json;

#[test]
fn enqueue_lease_complete_replay_flow_works() {
    let store = MemoryStore::default();
    store
        .insert_project(ProjectRecord {
        id: "proj_1".to_string(),
        name: "Acme".to_string(),
        slug: "acme".to_string(),
        api_key_hash: "hash".to_string(),
    })
        .expect("project insert should work");

    let deploy = service::register_deploy(
        &store,
        RegisterDeployRequest {
            project_id: "proj_1".to_string(),
            version: "v1".to_string(),
            bundle_url: "s3://gum/bundles/v1.tar.gz".to_string(),
            bundle_sha256: "abc123".to_string(),
            sdk_language: "python".to_string(),
            entrypoint: "jobs.py".to_string(),
            jobs: vec![RegisteredJob {
                id: "job_sync_customer".to_string(),
                name: "sync_customer".to_string(),
                handler_ref: "jobs:sync_customer".to_string(),
                trigger_mode: "manual".to_string(),
                schedule_expr: None,
                retries: 1,
                timeout_secs: 300,
                rate_limit_spec: Some("20/m".to_string()),
                concurrency_limit: Some(5),
            }],
        },
    )
    .expect("register deploy should work");

    assert_eq!(deploy.registered_jobs, 1);

    let run = service::enqueue_run(
        &store,
        "proj_1",
        "job_sync_customer",
        EnqueueRunRequest {
            input: json!({ "customer_id": "cus_123" }),
        },
    )
    .expect("enqueue should work");
    assert_eq!(run.status, RunStatus::Queued);

    let leased = service::lease_run(
        &store,
        LeaseRunRequest {
            runner_id: "runner_1".to_string(),
        },
    )
    .expect("lease should work")
    .expect("run should be leased");
    assert_eq!(leased.run_id, run.id);
    assert_eq!(leased.handler_ref, "jobs:sync_customer");
    assert_eq!(leased.timeout_secs, 300);

    service::append_log(
        &store,
        &leased.run_id,
        &leased.attempt_id,
        AppendLogRequest {
            stream: "stdout".to_string(),
            message: "starting sync".to_string(),
        },
    )
    .expect("log append should work");

    let completed = service::complete_attempt(
        &store,
        &leased.attempt_id,
        CompleteAttemptRequest {
            runner_id: "runner_1".to_string(),
            status: AttemptStatus::Succeeded,
            failure_reason: None,
        },
    )
    .expect("complete should work");
    assert_eq!(completed.status, RunStatus::Succeeded);

    let logs = service::get_logs(&store, &run.id).expect("logs should load");
    assert_eq!(logs.len(), 1);

    let replay = service::replay_run(&store, &run.id).expect("replay should work");
    assert_eq!(replay.replay_of, run.id);
}

#[test]
fn failed_attempt_requeues_when_retries_remain() {
    let store = MemoryStore::default();
    store
        .insert_project(ProjectRecord {
        id: "proj_1".to_string(),
        name: "Acme".to_string(),
        slug: "acme".to_string(),
        api_key_hash: "hash".to_string(),
    })
        .expect("project insert should work");

    service::register_deploy(
        &store,
        RegisterDeployRequest {
            project_id: "proj_1".to_string(),
            version: "v1".to_string(),
            bundle_url: "s3://gum/bundles/v1.tar.gz".to_string(),
            bundle_sha256: "abc123".to_string(),
            sdk_language: "python".to_string(),
            entrypoint: "jobs.py".to_string(),
            jobs: vec![RegisteredJob {
                id: "job_sync_customer".to_string(),
                name: "sync_customer".to_string(),
                handler_ref: "jobs:sync_customer".to_string(),
                trigger_mode: "manual".to_string(),
                schedule_expr: None,
                retries: 1,
                timeout_secs: 300,
                rate_limit_spec: None,
                concurrency_limit: None,
            }],
        },
    )
    .expect("register deploy should work");

    let run = service::enqueue_run(
        &store,
        "proj_1",
        "job_sync_customer",
        EnqueueRunRequest {
            input: json!({ "customer_id": "cus_123" }),
        },
    )
    .expect("enqueue should work");

    let leased = service::lease_run(
        &store,
        LeaseRunRequest {
            runner_id: "runner_1".to_string(),
        },
    )
    .expect("lease should work")
    .expect("run should be leased");

    let retried = service::complete_attempt(
        &store,
        &leased.attempt_id,
        CompleteAttemptRequest {
            runner_id: "runner_1".to_string(),
            status: AttemptStatus::Failed,
            failure_reason: Some("transient failure".to_string()),
        },
    )
    .expect("complete should work");

    assert_eq!(retried.status, RunStatus::Queued);
    assert_eq!(retried.attempt, 1);

    let reloaded = service::get_run(&store, &run.id)
        .expect("get run should work")
        .expect("run should exist");
    assert_eq!(reloaded.status, RunStatus::Queued);
}

#[test]
fn rate_limit_blocks_second_lease_within_the_same_window() {
    let store = MemoryStore::default();
    store
        .insert_project(ProjectRecord {
            id: "proj_1".to_string(),
            name: "Acme".to_string(),
            slug: "acme".to_string(),
            api_key_hash: "hash".to_string(),
        })
        .expect("project insert should work");

    service::register_deploy(
        &store,
        RegisterDeployRequest {
            project_id: "proj_1".to_string(),
            version: "v1".to_string(),
            bundle_url: "s3://gum/bundles/v1.tar.gz".to_string(),
            bundle_sha256: "abc123".to_string(),
            sdk_language: "python".to_string(),
            entrypoint: "jobs.py".to_string(),
            jobs: vec![RegisteredJob {
                id: "job_sync_customer".to_string(),
                name: "sync_customer".to_string(),
                handler_ref: "jobs:sync_customer".to_string(),
                trigger_mode: "manual".to_string(),
                schedule_expr: None,
                retries: 1,
                timeout_secs: 300,
                rate_limit_spec: Some("1/h".to_string()),
                concurrency_limit: None,
            }],
        },
    )
    .expect("register deploy should work");

    service::enqueue_run(
        &store,
        "proj_1",
        "job_sync_customer",
        EnqueueRunRequest {
            input: json!({ "customer_id": "cus_123" }),
        },
    )
    .expect("first enqueue should work");

    service::enqueue_run(
        &store,
        "proj_1",
        "job_sync_customer",
        EnqueueRunRequest {
            input: json!({ "customer_id": "cus_456" }),
        },
    )
    .expect("second enqueue should work");

    let first_leased = service::lease_run(
        &store,
        LeaseRunRequest {
            runner_id: "runner_1".to_string(),
        },
    )
    .expect("first lease should work");
    assert!(first_leased.is_some(), "first queued run should lease");

    let second_leased = service::lease_run(
        &store,
        LeaseRunRequest {
            runner_id: "runner_2".to_string(),
        },
    )
    .expect("second lease should work");
    assert!(
        second_leased.is_none(),
        "second run should stay queued inside the same rate-limit window"
    );
}

#[test]
fn scheduled_jobs_tick_into_normal_queued_runs_without_duplicates() {
    let store = MemoryStore::default();
    store
        .insert_project(ProjectRecord {
            id: "proj_1".to_string(),
            name: "Acme".to_string(),
            slug: "acme".to_string(),
            api_key_hash: "hash".to_string(),
        })
        .expect("project insert should work");

    service::register_deploy(
        &store,
        RegisterDeployRequest {
            project_id: "proj_1".to_string(),
            version: "v1".to_string(),
            bundle_url: "s3://gum/bundles/v1.tar.gz".to_string(),
            bundle_sha256: "abc123".to_string(),
            sdk_language: "python".to_string(),
            entrypoint: "jobs.py".to_string(),
            jobs: vec![RegisteredJob {
                id: "job_send_followup".to_string(),
                name: "send_followup".to_string(),
                handler_ref: "jobs:send_followup".to_string(),
                trigger_mode: "both".to_string(),
                schedule_expr: Some("20m".to_string()),
                retries: 5,
                timeout_secs: 300,
                rate_limit_spec: None,
                concurrency_limit: None,
            }],
        },
    )
    .expect("register deploy should work");

    let job = store
        .get_job("job_send_followup")
        .expect("job lookup should work")
        .expect("job should exist");
    let first_due = job.created_at_epoch_ms + (20 * 60 * 1000);

    let created = service::tick_schedules(&store, first_due)
        .expect("scheduler tick should work");
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].status, RunStatus::Queued);

    let created_again = service::tick_schedules(&store, first_due)
        .expect("second scheduler tick should work");
    assert_eq!(created_again.len(), 0);

    let leased = service::lease_run(
        &store,
        LeaseRunRequest {
            runner_id: "runner_1".to_string(),
        },
    )
    .expect("lease should work")
    .expect("scheduled run should be leased");
    assert_eq!(leased.job_id, "job_send_followup");

    let runs = store
        .tick_schedules(first_due + (20 * 60 * 1000))
        .expect("next schedule tick should work");
    assert_eq!(runs.len(), 1);
}
