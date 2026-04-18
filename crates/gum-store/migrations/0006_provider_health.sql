CREATE TABLE IF NOT EXISTS provider_targets (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    probe_kind TEXT NOT NULL,
    probe_config_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS provider_checks (
    id TEXT PRIMARY KEY,
    provider_target_id TEXT NOT NULL REFERENCES provider_targets(id),
    status TEXT NOT NULL,
    latency_ms INTEGER,
    error_class TEXT,
    status_code INTEGER,
    checked_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS provider_checks_target_checked_idx
    ON provider_checks(provider_target_id, checked_at DESC);

CREATE TABLE IF NOT EXISTS provider_health (
    provider_target_id TEXT PRIMARY KEY REFERENCES provider_targets(id),
    state TEXT NOT NULL,
    reason TEXT,
    last_changed_at TIMESTAMPTZ NOT NULL,
    last_success_at TIMESTAMPTZ,
    last_failure_at TIMESTAMPTZ,
    degraded_score INTEGER NOT NULL DEFAULT 0,
    down_score INTEGER NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
