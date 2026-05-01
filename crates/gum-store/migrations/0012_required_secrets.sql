ALTER TABLE jobs
    ADD COLUMN IF NOT EXISTS required_secret_names TEXT[] NOT NULL DEFAULT '{}';
