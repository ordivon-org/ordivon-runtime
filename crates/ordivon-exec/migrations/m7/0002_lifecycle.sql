CREATE TABLE m7_lifecycle_events (
    event_id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    operator_id TEXT,
    job_id TEXT REFERENCES jobs(job_id) ON DELETE RESTRICT,
    attempt_id TEXT REFERENCES attempts(attempt_id) ON DELETE RESTRICT,
    detail_json TEXT NOT NULL,
    detail_digest TEXT NOT NULL,
    observed_at_ms INTEGER NOT NULL CHECK (observed_at_ms >= 0)
) STRICT;

CREATE TABLE m7_investigation_holds (
    hold_id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES jobs(job_id) ON DELETE RESTRICT,
    operator_id TEXT NOT NULL,
    reason_digest TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    released_at_ms INTEGER CHECK (released_at_ms IS NULL OR released_at_ms >= 0)
) STRICT;

CREATE INDEX idx_m7_active_holds
ON m7_investigation_holds(job_id, released_at_ms);

CREATE TABLE m7_gc_plans (
    plan_id TEXT PRIMARY KEY,
    policy_digest TEXT NOT NULL,
    plan_digest TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('planned','executing','completed','failed')),
    item_count INTEGER NOT NULL CHECK (item_count >= 0),
    byte_length INTEGER NOT NULL CHECK (byte_length >= 0),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    finished_at_ms INTEGER CHECK (finished_at_ms IS NULL OR finished_at_ms >= 0)
) STRICT;

CREATE TABLE m7_gc_items (
    plan_id TEXT NOT NULL REFERENCES m7_gc_plans(plan_id) ON DELETE RESTRICT,
    attempt_id TEXT NOT NULL REFERENCES attempts(attempt_id) ON DELETE RESTRICT,
    bundle_path TEXT NOT NULL,
    bundle_digest TEXT NOT NULL,
    byte_length INTEGER NOT NULL CHECK (byte_length >= 0),
    state TEXT NOT NULL CHECK (state IN ('planned','staged','deleted','skipped')),
    PRIMARY KEY (plan_id, attempt_id)
) STRICT;

CREATE TABLE m7_artifact_tombstones (
    artifact_id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES jobs(job_id) ON DELETE RESTRICT,
    attempt_id TEXT NOT NULL REFERENCES attempts(attempt_id) ON DELETE RESTRICT,
    kind TEXT NOT NULL,
    digest TEXT NOT NULL,
    byte_length INTEGER NOT NULL CHECK (byte_length >= 0),
    gc_plan_id TEXT NOT NULL REFERENCES m7_gc_plans(plan_id) ON DELETE RESTRICT,
    deleted_at_ms INTEGER NOT NULL CHECK (deleted_at_ms >= 0)
) STRICT;

CREATE TABLE m7_backups (
    backup_id TEXT PRIMARY KEY,
    backup_path TEXT NOT NULL UNIQUE,
    registry_digest TEXT NOT NULL,
    manifest_digest TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('created','verified','restored','failed')),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    restored_at_ms INTEGER CHECK (restored_at_ms IS NULL OR restored_at_ms >= 0)
) STRICT;

CREATE TABLE m7_orphan_remediations (
    remediation_id TEXT PRIMARY KEY,
    attempt_id TEXT NOT NULL REFERENCES attempts(attempt_id) ON DELETE RESTRICT,
    operator_id TEXT NOT NULL,
    expected_evidence_digest TEXT NOT NULL,
    observed_evidence_digest TEXT NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('inspect','terminate','release')),
    outcome TEXT NOT NULL CHECK (outcome IN ('observed','denied','released','failed')),
    detail_digest TEXT NOT NULL,
    observed_at_ms INTEGER NOT NULL CHECK (observed_at_ms >= 0)
) STRICT;

CREATE INDEX idx_m7_gc_plan_state ON m7_gc_plans(state, created_at_ms);
CREATE INDEX idx_m7_orphan_attempt ON m7_orphan_remediations(attempt_id, observed_at_ms);

CREATE TRIGGER m7_lifecycle_events_no_update
BEFORE UPDATE ON m7_lifecycle_events
BEGIN
    SELECT RAISE(ABORT, 'm7_lifecycle_events are append-only');
END;

CREATE TRIGGER m7_lifecycle_events_no_delete
BEFORE DELETE ON m7_lifecycle_events
BEGIN
    SELECT RAISE(ABORT, 'm7_lifecycle_events are append-only');
END;
