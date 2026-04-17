ALTER TABLE attempts
    ADD COLUMN IF NOT EXISTS cancel_requested_at TIMESTAMPTZ;

ALTER TABLE leases
    ADD COLUMN IF NOT EXISTS revoke_requested_at TIMESTAMPTZ;
