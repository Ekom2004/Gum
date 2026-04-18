use gum_api::routes::{
    AppendLogRequest, CancelRunRequest, CompleteAttemptRequest, EnqueueRunRequest, LeaseRunRequest,
    RegisterDeployRequest, RegisterRunnerRequest, RegisteredJob, RunnerHeartbeatRequest,
};
use gum_api::service;
use gum_store::memory::MemoryStore;
use gum_store::models::{ProjectRecord, ProviderCheckStatus, ProviderHealthState};
use gum_store::queries::{
    GumStore, RecordProviderCheckParams, SetProviderHealthParams, UpsertProviderTargetParams,
};
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
            lease_ttl_secs: 30,
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
            failure_class: None,
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
            lease_ttl_secs: 30,
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
            failure_class: Some("provider_5xx".to_string()),
        },
    )
    .expect("complete should work");

    assert_eq!(retried.status, RunStatus::Queued);
    assert_eq!(retried.attempt, 1);

    let reloaded = service::get_run(&store, &run.id)
        .expect("get run should work")
        .expect("run should exist");
    assert_eq!(reloaded.status, RunStatus::Queued);
    assert_eq!(reloaded.failure_class.as_deref(), Some("provider_5xx"));
    assert!(reloaded.retry_after_epoch_ms.is_some());
}

#[test]
fn provider_down_blocks_retry_until_recovery() {
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
                id: "job_generate_summary".to_string(),
                name: "generate_summary".to_string(),
                handler_ref: "jobs:generate_summary".to_string(),
                trigger_mode: "manual".to_string(),
                schedule_expr: None,
                retries: 5,
                timeout_secs: 300,
                rate_limit_spec: Some("openai:60/m".to_string()),
                concurrency_limit: Some(5),
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
    store
        .upsert_provider_target(UpsertProviderTargetParams {
            id: "provider_openai".to_string(),
            name: "OpenAI".to_string(),
            slug: "openai".to_string(),
            probe_kind: "http".to_string(),
            probe_config_json: json!({"url": "https://api.openai.com/v1/models"}),
            enabled: true,
        })
        .expect("provider target should be stored");
    store
        .set_provider_health(SetProviderHealthParams {
            provider_target_id: "provider_openai".to_string(),
            state: ProviderHealthState::Down,
            reason: Some("probe failures".to_string()),
            last_changed_at_epoch_ms: 1_000,
            last_success_at_epoch_ms: None,
            last_failure_at_epoch_ms: Some(1_000),
            degraded_score: 3,
            down_score: 3,
        })
        .expect("provider health should be stored");

    let _run = service::enqueue_run(
        &store,
        "proj_1",
        "job_generate_summary",
        EnqueueRunRequest {
            input: json!({"doc_id": "doc_123"}),
        },
    )
    .expect("enqueue should work");
    let leased = service::lease_run(
        &store,
        LeaseRunRequest {
            runner_id: "runner_1".to_string(),
            lease_ttl_secs: 30,
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
            failure_reason: Some("upstream unavailable".to_string()),
            failure_class: Some("provider_5xx".to_string()),
        },
    )
    .expect("completion should work");

    assert_eq!(retried.status, RunStatus::Queued);
    assert_eq!(
        retried.failure_class.as_deref(),
        Some("blocked_by_downstream")
    );
    assert_eq!(retried.waiting_for_provider_slug.as_deref(), Some("openai"));
    assert!(retried.retry_after_epoch_ms.is_some());

    let lease_attempt = service::lease_run(
        &store,
        LeaseRunRequest {
            runner_id: "runner_1".to_string(),
            lease_ttl_secs: 30,
        },
    )
    .expect("lease should work");
    assert!(
        lease_attempt.is_none(),
        "blocked retries should not lease immediately"
    );
}

#[test]
fn user_code_failures_do_not_consume_retry_budget_as_requeues() {
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
                id: "job_process_webhook".to_string(),
                name: "process_webhook".to_string(),
                handler_ref: "jobs:process_webhook".to_string(),
                trigger_mode: "manual".to_string(),
                schedule_expr: None,
                retries: 5,
                timeout_secs: 300,
                rate_limit_spec: None,
                concurrency_limit: Some(5),
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
        "job_process_webhook",
        EnqueueRunRequest {
            input: json!({"event_id": "evt_123"}),
        },
    )
    .expect("enqueue should work");
    let leased = service::lease_run(
        &store,
        LeaseRunRequest {
            runner_id: "runner_1".to_string(),
            lease_ttl_secs: 30,
        },
    )
    .expect("lease should work")
    .expect("run should be leased");

    let failed = service::complete_attempt(
        &store,
        &leased.attempt_id,
        CompleteAttemptRequest {
            runner_id: "runner_1".to_string(),
            status: AttemptStatus::Failed,
            failure_reason: Some("invalid payload".to_string()),
            failure_class: Some("user_code_error".to_string()),
        },
    )
    .expect("completion should work");

    assert_eq!(failed.status, RunStatus::Failed);
    assert_eq!(failed.failure_class.as_deref(), Some("user_code_error"));
    assert!(failed.retry_after_epoch_ms.is_none());

    let lease_attempt = service::lease_run(
        &store,
        LeaseRunRequest {
            runner_id: "runner_1".to_string(),
            lease_ttl_secs: 30,
        },
    )
    .expect("lease should work");
    assert!(
        lease_attempt.is_none(),
        "terminal failures should not requeue"
    );
    let reloaded = service::get_run(&store, &run.id)
        .expect("run lookup should work")
        .expect("run should exist");
    assert_eq!(reloaded.status, RunStatus::Failed);
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
    .expect("register runner 1 should work");
    service::register_runner(
        &store,
        RegisterRunnerRequest {
            runner_id: "runner_2".to_string(),
            compute_class: "standard".to_string(),
            max_concurrent_leases: 1,
            heartbeat_timeout_secs: 30,
        },
    )
    .expect("register runner 2 should work");

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
            lease_ttl_secs: 30,
        },
    )
    .expect("first lease should work");
    assert!(first_leased.is_some(), "first queued run should lease");

    let second_leased = service::lease_run(
        &store,
        LeaseRunRequest {
            runner_id: "runner_2".to_string(),
            lease_ttl_secs: 30,
        },
    )
    .expect("second lease should work");
    assert!(
        second_leased.is_none(),
        "second run should stay queued inside the same rate-limit window"
    );
}

#[test]
fn canceling_a_queued_run_marks_it_canceled() {
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
                id: "job_export".to_string(),
                name: "export".to_string(),
                handler_ref: "jobs:export".to_string(),
                trigger_mode: "manual".to_string(),
                schedule_expr: None,
                retries: 0,
                timeout_secs: 300,
                rate_limit_spec: None,
                concurrency_limit: None,
                compute_class: None,
            }],
        },
    )
    .expect("register deploy should work");

    let run = service::enqueue_run(
        &store,
        "proj_1",
        "job_export",
        EnqueueRunRequest {
            input: json!({ "workspace_id": "ws_123" }),
        },
    )
    .expect("enqueue should work");

    let canceled = service::cancel_run(&store, &run.id, CancelRunRequest { reason: None })
        .expect("cancel should work");

    assert_eq!(canceled.status, RunStatus::Canceled);
    assert_eq!(canceled.failure_reason.as_deref(), Some("canceled"));
}

#[test]
fn canceling_a_running_run_requests_revocation_and_requires_canceled_completion() {
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
                id: "job_export".to_string(),
                name: "export".to_string(),
                handler_ref: "jobs:export".to_string(),
                trigger_mode: "manual".to_string(),
                schedule_expr: None,
                retries: 0,
                timeout_secs: 300,
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
        "job_export",
        EnqueueRunRequest {
            input: json!({ "workspace_id": "ws_123" }),
        },
    )
    .expect("enqueue should work");

    let leased = service::lease_run(
        &store,
        LeaseRunRequest {
            runner_id: "runner_1".to_string(),
            lease_ttl_secs: 30,
        },
    )
    .expect("lease should work")
    .expect("run should be leased");

    let cancel_response = service::cancel_run(&store, &run.id, CancelRunRequest { reason: None })
        .expect("cancel should work");
    assert_eq!(cancel_response.status, RunStatus::Running);
    assert_eq!(
        cancel_response.failure_reason.as_deref(),
        Some("cancel requested")
    );

    let lease_state = service::get_lease_state(&store, &leased.lease_id)
        .expect("lease state should load")
        .expect("lease should exist");
    assert!(lease_state.cancel_requested);

    let failure = service::complete_attempt(
        &store,
        &leased.attempt_id,
        CompleteAttemptRequest {
            runner_id: "runner_1".to_string(),
            status: AttemptStatus::Failed,
            failure_reason: Some("should not be allowed".to_string()),
            failure_class: Some("user_code_error".to_string()),
        },
    );
    assert!(failure
        .expect_err("non-canceled completion should be rejected")
        .contains("cancel requested"));

    let canceled = service::complete_attempt(
        &store,
        &leased.attempt_id,
        CompleteAttemptRequest {
            runner_id: "runner_1".to_string(),
            status: AttemptStatus::Canceled,
            failure_reason: Some("job canceled".to_string()),
            failure_class: None,
        },
    )
    .expect("canceled completion should work");
    assert_eq!(canceled.status, RunStatus::Canceled);
    assert_eq!(canceled.failure_reason.as_deref(), Some("job canceled"));
}

#[test]
fn admin_views_expose_runs_runners_and_leases() {
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
                id: "job_export".to_string(),
                name: "export".to_string(),
                handler_ref: "jobs:export".to_string(),
                trigger_mode: "manual".to_string(),
                schedule_expr: None,
                retries: 0,
                timeout_secs: 300,
                rate_limit_spec: None,
                concurrency_limit: None,
                compute_class: Some("high-mem".to_string()),
            }],
        },
    )
    .expect("register deploy should work");
    service::register_runner(
        &store,
        RegisterRunnerRequest {
            runner_id: "runner_1".to_string(),
            compute_class: "high-mem".to_string(),
            max_concurrent_leases: 2,
            heartbeat_timeout_secs: 30,
        },
    )
    .expect("register runner should work");

    let run = service::enqueue_run(
        &store,
        "proj_1",
        "job_export",
        EnqueueRunRequest {
            input: json!({ "workspace_id": "ws_123" }),
        },
    )
    .expect("enqueue should work");

    let leased = service::lease_run(
        &store,
        LeaseRunRequest {
            runner_id: "runner_1".to_string(),
            lease_ttl_secs: 30,
        },
    )
    .expect("lease should work")
    .expect("run should be leased");

    let runs = service::list_runs(&store, 50).expect("runs should list");
    assert_eq!(runs.runs.len(), 1);
    assert_eq!(runs.runs[0].id, run.id);

    let runners = service::list_runners(&store).expect("runners should list");
    assert_eq!(runners.runners.len(), 1);
    assert_eq!(runners.runners[0].id, "runner_1");
    assert_eq!(runners.runners[0].active_lease_count, 1);

    let leases = service::list_leases(&store).expect("leases should list");
    assert_eq!(leases.leases.len(), 1);
    assert_eq!(leases.leases[0].lease_id, leased.lease_id);
    assert_eq!(leases.leases[0].runner_id, "runner_1");
    assert!(!leases.leases[0].cancel_requested);
}

#[test]
fn provider_health_can_be_recorded_and_listed() {
    let store = MemoryStore::default();
    store
        .insert_project(ProjectRecord {
            id: "proj_1".to_string(),
            name: "Acme".to_string(),
            slug: "acme".to_string(),
            api_key_hash: "hash".to_string(),
        })
        .expect("project insert should work");

    let target = store
        .upsert_provider_target(UpsertProviderTargetParams {
            id: "provider_openai".to_string(),
            name: "OpenAI".to_string(),
            slug: "openai".to_string(),
            probe_kind: "http".to_string(),
            probe_config_json: json!({
                "method": "POST",
                "path": "/v1/chat/completions"
            }),
            enabled: true,
        })
        .expect("provider target should upsert");
    assert_eq!(target.slug, "openai");

    let check = store
        .record_provider_check(RecordProviderCheckParams {
            provider_target_id: target.id.clone(),
            status: ProviderCheckStatus::Failure,
            latency_ms: Some(1_200),
            error_class: Some("provider_timeout".to_string()),
            status_code: None,
            checked_at_epoch_ms: 1_710_000_000_000,
        })
        .expect("provider check should record");
    assert_eq!(check.error_class.as_deref(), Some("provider_timeout"));

    store
        .set_provider_health(SetProviderHealthParams {
            provider_target_id: target.id.clone(),
            state: ProviderHealthState::Degraded,
            reason: Some("probe timeout rate elevated".to_string()),
            last_changed_at_epoch_ms: 1_710_000_000_000,
            last_success_at_epoch_ms: Some(1_709_999_940_000),
            last_failure_at_epoch_ms: Some(1_710_000_000_000),
            degraded_score: 3,
            down_score: 0,
        })
        .expect("provider health should set");

    let listed = service::list_provider_health(&store).expect("provider health should list");
    assert_eq!(listed.providers.len(), 1);
    let provider = &listed.providers[0];
    assert_eq!(provider.provider_slug, "openai");
    assert_eq!(provider.state, "degraded");
    assert_eq!(
        provider.reason.as_deref(),
        Some("probe timeout rate elevated")
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

    let job = store
        .get_job("job_send_followup")
        .expect("job lookup should work")
        .expect("job should exist");
    let first_due = job.created_at_epoch_ms + (20 * 60 * 1000);

    let created = service::tick_schedules(&store, first_due).expect("scheduler tick should work");
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].status, RunStatus::Queued);

    let created_again =
        service::tick_schedules(&store, first_due).expect("second scheduler tick should work");
    assert_eq!(created_again.len(), 0);

    let leased = service::lease_run(
        &store,
        LeaseRunRequest {
            runner_id: "runner_1".to_string(),
            lease_ttl_secs: 30,
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

#[test]
fn expired_lease_is_recovered_and_heartbeat_keeps_active_lease_alive() {
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
                id: "job_export_workspace".to_string(),
                name: "export_workspace".to_string(),
                handler_ref: "jobs:export_workspace".to_string(),
                trigger_mode: "manual".to_string(),
                schedule_expr: None,
                retries: 1,
                timeout_secs: 7_200,
                rate_limit_spec: None,
                concurrency_limit: Some(1),
                compute_class: Some("high-mem".to_string()),
            }],
        },
    )
    .expect("register deploy should work");

    service::register_runner(
        &store,
        RegisterRunnerRequest {
            runner_id: "runner_1".to_string(),
            compute_class: "high-mem".to_string(),
            max_concurrent_leases: 1,
            heartbeat_timeout_secs: 2,
        },
    )
    .expect("register runner should work");
    service::register_runner(
        &store,
        RegisterRunnerRequest {
            runner_id: "runner_2".to_string(),
            compute_class: "high-mem".to_string(),
            max_concurrent_leases: 1,
            heartbeat_timeout_secs: 2,
        },
    )
    .expect("register second runner should work");

    let run = service::enqueue_run(
        &store,
        "proj_1",
        "job_export_workspace",
        EnqueueRunRequest {
            input: json!({ "workspace_id": "ws_123" }),
        },
    )
    .expect("enqueue should work");

    let leased = service::lease_run(
        &store,
        LeaseRunRequest {
            runner_id: "runner_1".to_string(),
            lease_ttl_secs: 2,
        },
    )
    .expect("lease should work")
    .expect("run should be leased");

    service::heartbeat_runner(
        &store,
        RunnerHeartbeatRequest {
            runner_id: "runner_1".to_string(),
            compute_class: "high-mem".to_string(),
            max_concurrent_leases: 1,
            heartbeat_timeout_secs: 2,
            lease_ttl_secs: 2,
            active_lease_ids: vec![leased.lease_id.clone()],
        },
    )
    .expect("heartbeat should work");

    std::thread::sleep(std::time::Duration::from_millis(1_200));
    let still_running = store
        .recover_lost_attempts(now_epoch_ms())
        .expect("recovery should work before renewed lease expires");
    assert!(
        still_running.is_empty(),
        "heartbeat should keep the lease alive"
    );

    std::thread::sleep(std::time::Duration::from_millis(1_200));
    let recovered = store
        .recover_lost_attempts(now_epoch_ms())
        .expect("recovery should work after lease expiry");
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].status, RunStatus::Queued);

    let reloaded = service::get_run(&store, &run.id)
        .expect("get run should work")
        .expect("run should exist");
    assert_eq!(reloaded.status, RunStatus::Queued);
    assert_eq!(reloaded.attempt, 1);

    let completion_error = service::complete_attempt(
        &store,
        &leased.attempt_id,
        CompleteAttemptRequest {
            runner_id: "runner_1".to_string(),
            status: AttemptStatus::Succeeded,
            failure_reason: None,
            failure_class: None,
        },
    )
    .expect_err("recovered attempts should not be completable");
    assert!(
        completion_error.contains("already finished"),
        "stale runner should be fenced after recovery"
    );

    let leased_again = service::lease_run(
        &store,
        LeaseRunRequest {
            runner_id: "runner_2".to_string(),
            lease_ttl_secs: 2,
        },
    )
    .expect("second lease should work")
    .expect("recovered run should be leaseable again");
    assert_eq!(leased_again.run_id, run.id);
    assert_ne!(leased_again.attempt_id, leased.attempt_id);
}

#[test]
fn compute_class_and_runner_capacity_drive_placement() {
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
            jobs: vec![
                RegisteredJob {
                    id: "job_export_workspace".to_string(),
                    name: "export_workspace".to_string(),
                    handler_ref: "jobs:export_workspace".to_string(),
                    trigger_mode: "manual".to_string(),
                    schedule_expr: None,
                    retries: 1,
                    timeout_secs: 7_200,
                    rate_limit_spec: None,
                    concurrency_limit: None,
                    compute_class: Some("high-mem".to_string()),
                },
                RegisteredJob {
                    id: "job_sync_customer".to_string(),
                    name: "sync_customer".to_string(),
                    handler_ref: "jobs:sync_customer".to_string(),
                    trigger_mode: "manual".to_string(),
                    schedule_expr: None,
                    retries: 1,
                    timeout_secs: 300,
                    rate_limit_spec: None,
                    concurrency_limit: None,
                    compute_class: None,
                },
            ],
        },
    )
    .expect("register deploy should work");

    service::register_runner(
        &store,
        RegisterRunnerRequest {
            runner_id: "runner_standard".to_string(),
            compute_class: "standard".to_string(),
            max_concurrent_leases: 1,
            heartbeat_timeout_secs: 30,
        },
    )
    .expect("register standard runner should work");
    service::register_runner(
        &store,
        RegisterRunnerRequest {
            runner_id: "runner_high_mem".to_string(),
            compute_class: "high-mem".to_string(),
            max_concurrent_leases: 1,
            heartbeat_timeout_secs: 30,
        },
    )
    .expect("register high-mem runner should work");

    service::enqueue_run(
        &store,
        "proj_1",
        "job_export_workspace",
        EnqueueRunRequest {
            input: json!({ "workspace_id": "ws_123" }),
        },
    )
    .expect("enqueue export should work");

    let standard_cannot_take_export = service::lease_run(
        &store,
        LeaseRunRequest {
            runner_id: "runner_standard".to_string(),
            lease_ttl_secs: 30,
        },
    )
    .expect("standard runner lease attempt should work");
    assert!(
        standard_cannot_take_export.is_none(),
        "standard runner should not lease a high-mem job"
    );

    let high_mem = service::lease_run(
        &store,
        LeaseRunRequest {
            runner_id: "runner_high_mem".to_string(),
            lease_ttl_secs: 30,
        },
    )
    .expect("high-mem lease should work")
    .expect("high-mem runner should get the export job");
    assert_eq!(high_mem.job_id, "job_export_workspace");

    service::enqueue_run(
        &store,
        "proj_1",
        "job_sync_customer",
        EnqueueRunRequest {
            input: json!({ "customer_id": "cus_123" }),
        },
    )
    .expect("enqueue sync should work");
    service::enqueue_run(
        &store,
        "proj_1",
        "job_sync_customer",
        EnqueueRunRequest {
            input: json!({ "customer_id": "cus_456" }),
        },
    )
    .expect("enqueue second sync should work");

    let standard_first = service::lease_run(
        &store,
        LeaseRunRequest {
            runner_id: "runner_standard".to_string(),
            lease_ttl_secs: 30,
        },
    )
    .expect("standard runner should lease")
    .expect("standard runner should get the generic job");
    assert_eq!(standard_first.job_id, "job_sync_customer");

    let standard_second = service::lease_run(
        &store,
        LeaseRunRequest {
            runner_id: "runner_standard".to_string(),
            lease_ttl_secs: 30,
        },
    )
    .expect("second standard lease should work");
    assert!(
        standard_second.is_none(),
        "runner capacity should prevent a second concurrent lease"
    );
}

#[test]
fn control_lease_fences_scheduler_work() {
    let store = MemoryStore::default();
    let now = now_epoch_ms();

    let first = store
        .try_acquire_control_lease(gum_store::queries::ControlLeaseParams {
            lease_name: "scheduler".to_string(),
            holder_id: "instance_a".to_string(),
            ttl_secs: 5,
            now_epoch_ms: now,
        })
        .expect("first control lease should work");
    assert!(first, "first scheduler instance should acquire leadership");

    let second = store
        .try_acquire_control_lease(gum_store::queries::ControlLeaseParams {
            lease_name: "scheduler".to_string(),
            holder_id: "instance_b".to_string(),
            ttl_secs: 5,
            now_epoch_ms: now + 1_000,
        })
        .expect("second control lease attempt should work");
    assert!(
        !second,
        "second scheduler instance should be fenced while lease is live"
    );

    let renewed = store
        .try_acquire_control_lease(gum_store::queries::ControlLeaseParams {
            lease_name: "scheduler".to_string(),
            holder_id: "instance_a".to_string(),
            ttl_secs: 5,
            now_epoch_ms: now + 2_000,
        })
        .expect("same holder renewal should work");
    assert!(
        renewed,
        "current leader should be able to renew its own lease"
    );

    let takeover = store
        .try_acquire_control_lease(gum_store::queries::ControlLeaseParams {
            lease_name: "scheduler".to_string(),
            holder_id: "instance_b".to_string(),
            ttl_secs: 5,
            now_epoch_ms: now + 8_000,
        })
        .expect("expired control lease should be re-acquirable");
    assert!(takeover, "leadership should transfer after expiry");
}

fn now_epoch_ms() -> i64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_millis() as i64,
        Err(_) => 0,
    }
}
