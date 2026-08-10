CREATE TABLE IF NOT EXISTS job_execution_providers (
    job_id TEXT PRIMARY KEY NOT NULL REFERENCES jobs(job_id) ON DELETE CASCADE,
    snapshot_json TEXT NOT NULL,
    snapshot_digest TEXT NOT NULL
);
