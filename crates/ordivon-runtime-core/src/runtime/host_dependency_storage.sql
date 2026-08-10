CREATE TABLE IF NOT EXISTS job_host_dependencies (
    job_id TEXT PRIMARY KEY REFERENCES jobs(job_id) ON DELETE CASCADE,
    bindings_json TEXT NOT NULL,
    bindings_digest TEXT NOT NULL
);
