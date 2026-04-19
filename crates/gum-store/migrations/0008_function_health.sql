CREATE TABLE IF NOT EXISTS function_health (
    job_id TEXT PRIMARY KEY REFERENCES jobs(id),
    state TEXT NOT NULL,
    consecutive_infra_failures INTEGER NOT NULL DEFAULT 0,
    reason TEXT,
    hold_until TIMESTAMPTZ,
    last_changed_at TIMESTAMPTZ NOT NULL,
    last_success_at TIMESTAMPTZ,
    last_failure_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS function_health_state_hold_idx
    ON function_health(state, hold_until);
