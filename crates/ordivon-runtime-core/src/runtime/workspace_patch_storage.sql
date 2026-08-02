CREATE TABLE IF NOT EXISTS workspace_patch_operations (
    operation_id TEXT PRIMARY KEY,
    principal TEXT NOT NULL,
    client_request_id TEXT NOT NULL,
    request_digest TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    plan_json TEXT NOT NULL,
    max_diff_bytes INTEGER NOT NULL CHECK(max_diff_bytes > 0),
    state TEXT NOT NULL CHECK(state IN ('prepared','committed','unknown')),
    result_json TEXT,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    UNIQUE(principal, client_request_id),
    CHECK((state = 'committed' AND result_json IS NOT NULL) OR (state != 'committed' AND result_json IS NULL))
);

CREATE INDEX IF NOT EXISTS idx_workspace_patch_operations_workspace
ON workspace_patch_operations(workspace_id, created_at_ms, operation_id);
