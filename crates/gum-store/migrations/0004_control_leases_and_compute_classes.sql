ALTER TABLE jobs
    ADD COLUMN IF NOT EXISTS compute_class TEXT;

CREATE TABLE IF NOT EXISTS control_leases (
    name TEXT PRIMARY KEY,
    holder_id TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS control_leases_expires_at_idx ON control_leases(expires_at);

ALTER TABLE runners
    ADD COLUMN IF NOT EXISTS compute_class TEXT NOT NULL DEFAULT 'standard';

ALTER TABLE runners
    ADD COLUMN IF NOT EXISTS max_concurrent_leases INTEGER NOT NULL DEFAULT 1;
