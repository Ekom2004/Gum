-- A scheduled fire time should materialize at most once per job, even if
-- multiple scheduler ticks overlap or the scheduler restarts mid-catch-up.
CREATE UNIQUE INDEX IF NOT EXISTS runs_job_scheduled_unique_idx
    ON runs(job_id, scheduled_at)
    WHERE trigger_type = 'schedule';
