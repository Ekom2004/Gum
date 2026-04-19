ALTER TABLE jobs
ADD COLUMN IF NOT EXISTS key_field TEXT;

CREATE TABLE IF NOT EXISTS run_keys (
    project_id TEXT NOT NULL REFERENCES projects(id),
    job_id TEXT NOT NULL REFERENCES jobs(id),
    key_value TEXT NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (project_id, job_id, key_value)
);

CREATE INDEX IF NOT EXISTS run_keys_run_id_idx ON run_keys(run_id);
CREATE INDEX IF NOT EXISTS run_keys_expires_at_idx ON run_keys(expires_at);
