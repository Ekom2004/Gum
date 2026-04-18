ALTER TABLE runs
    ADD COLUMN IF NOT EXISTS failure_class TEXT,
    ADD COLUMN IF NOT EXISTS retry_after_epoch_ms BIGINT,
    ADD COLUMN IF NOT EXISTS waiting_for_provider_slug TEXT;

ALTER TABLE attempts
    ADD COLUMN IF NOT EXISTS failure_class TEXT;

CREATE INDEX IF NOT EXISTS runs_status_retry_after_idx
    ON runs(status, retry_after_epoch_ms);
