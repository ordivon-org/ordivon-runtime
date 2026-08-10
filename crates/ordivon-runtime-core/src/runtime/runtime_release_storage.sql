CREATE TABLE IF NOT EXISTS job_runtime_release_effects (
    job_id TEXT PRIMARY KEY REFERENCES jobs(job_id) ON DELETE CASCADE,
    effect_id TEXT NOT NULL UNIQUE,
    contract TEXT NOT NULL,
    request_digest TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    commit_revision TEXT NOT NULL,
    candidate_manifest_digest TEXT NOT NULL,
    expected_tool_count INTEGER NOT NULL CHECK(expected_tool_count >= 0),
    receipt_path TEXT NOT NULL,
    binding_digest TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_runtime_release_effect_request
    ON job_runtime_release_effects(request_digest, job_id);
