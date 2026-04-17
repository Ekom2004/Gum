CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    api_key_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS deploys (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    version TEXT NOT NULL,
    bundle_url TEXT NOT NULL,
    bundle_sha256 TEXT NOT NULL,
    sdk_language TEXT NOT NULL,
    entrypoint TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    deploy_id TEXT NOT NULL REFERENCES deploys(id),
    name TEXT NOT NULL,
    handler_ref TEXT NOT NULL,
    trigger_mode TEXT NOT NULL,
    schedule_expr TEXT,
    retries INTEGER NOT NULL DEFAULT 0,
    timeout_secs INTEGER NOT NULL,
    rate_limit_spec TEXT,
    concurrency_limit INTEGER,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS jobs_project_name_idx ON jobs(project_id, name);

CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    job_id TEXT NOT NULL REFERENCES jobs(id),
    deploy_id TEXT NOT NULL REFERENCES deploys(id),
    trigger_type TEXT NOT NULL,
    status TEXT NOT NULL,
    input_json JSONB NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL,
    scheduled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    failure_reason TEXT,
    replay_of_run_id TEXT REFERENCES runs(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS runs_job_status_idx ON runs(job_id, status);

CREATE TABLE IF NOT EXISTS attempts (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(id),
    attempt_number INTEGER NOT NULL,
    status TEXT NOT NULL,
    lease_id TEXT,
    runner_id TEXT,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    failure_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS attempts_run_attempt_number_idx ON attempts(run_id, attempt_number);

CREATE TABLE IF NOT EXISTS leases (
    id TEXT PRIMARY KEY,
    attempt_id TEXT NOT NULL REFERENCES attempts(id),
    runner_id TEXT NOT NULL,
    leased_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    acked_at TIMESTAMPTZ,
    released_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS leases_attempt_id_idx ON leases(attempt_id);
CREATE INDEX IF NOT EXISTS leases_runner_id_idx ON leases(runner_id);

CREATE TABLE IF NOT EXISTS logs (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(id),
    attempt_id TEXT NOT NULL REFERENCES attempts(id),
    ts TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    stream TEXT NOT NULL,
    message TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS logs_run_ts_idx ON logs(run_id, ts);
