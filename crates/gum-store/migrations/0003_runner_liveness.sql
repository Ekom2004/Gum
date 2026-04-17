CREATE TABLE IF NOT EXISTS runners (
    id TEXT PRIMARY KEY,
    heartbeat_timeout_secs INTEGER NOT NULL,
    last_heartbeat_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS runners_last_heartbeat_idx ON runners(last_heartbeat_at);
