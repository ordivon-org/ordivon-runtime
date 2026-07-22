CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    checksum TEXT NOT NULL,
    applied_at_ms INTEGER NOT NULL CHECK (applied_at_ms >= 0)
) STRICT;

CREATE TABLE jobs (
    job_id TEXT PRIMARY KEY,
    principal TEXT NOT NULL,
    client_request_id TEXT NOT NULL,
    request_digest TEXT NOT NULL,
    operation_digest TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    workspace_snapshot_json TEXT NOT NULL,
    execution_plan_json TEXT NOT NULL,
    execution_plan_digest TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    desired_state TEXT NOT NULL CHECK (desired_state IN ('run','cancelled')),
    resolution TEXT CHECK (resolution IS NULL OR resolution IN ('succeeded','failed','timed_out','cancelled','lost','orphaned')),
    current_attempt_id TEXT REFERENCES attempts(attempt_id) DEFERRABLE INITIALLY DEFERRED,
    row_version INTEGER NOT NULL DEFAULT 0 CHECK (row_version >= 0),
    UNIQUE (principal, client_request_id)
) STRICT;

CREATE TABLE attempts (
    attempt_id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES jobs(job_id) ON DELETE RESTRICT,
    attempt_number INTEGER NOT NULL CHECK (attempt_number >= 1),
    state TEXT NOT NULL CHECK (state IN ('accepted','starting','running','stopping','recovering','succeeded','failed','timed_out','cancelled','lost','orphaned')),
    termination_intent TEXT NOT NULL CHECK (termination_intent IN ('natural','stop_requested','deadline_exceeded')),
    launch_token_digest TEXT NOT NULL,
    bundle_path TEXT NOT NULL,
    bundle_digest TEXT,
    boot_id TEXT,
    unit_name TEXT NOT NULL UNIQUE,
    invocation_id TEXT,
    control_group TEXT,
    main_pid INTEGER CHECK (main_pid IS NULL OR main_pid > 0),
    process_start_identity TEXT,
    runner_start_digest TEXT,
    result_digest TEXT,
    exit_code INTEGER,
    infrastructure_error_digest TEXT,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    started_at_ms INTEGER CHECK (started_at_ms IS NULL OR started_at_ms >= 0),
    finished_at_ms INTEGER CHECK (finished_at_ms IS NULL OR finished_at_ms >= 0),
    row_version INTEGER NOT NULL DEFAULT 0 CHECK (row_version >= 0),
    UNIQUE (job_id, attempt_number)
) STRICT;

CREATE TABLE idempotency_keys (
    principal TEXT NOT NULL,
    client_request_id TEXT NOT NULL,
    operation_digest TEXT NOT NULL,
    job_id TEXT NOT NULL UNIQUE REFERENCES jobs(job_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    PRIMARY KEY (principal, client_request_id)
) STRICT;

CREATE TABLE concurrency_reservations (
    reservation_id TEXT PRIMARY KEY,
    attempt_id TEXT NOT NULL UNIQUE REFERENCES attempts(attempt_id) ON DELETE RESTRICT,
    global_limit INTEGER NOT NULL CHECK (global_limit > 0),
    state TEXT NOT NULL CHECK (state IN ('active','held_orphaned','released')),
    acquired_at_ms INTEGER NOT NULL CHECK (acquired_at_ms >= 0),
    released_at_ms INTEGER CHECK (released_at_ms IS NULL OR released_at_ms >= 0),
    release_reason TEXT,
    CHECK ((state = 'released') = (released_at_ms IS NOT NULL))
) STRICT;

CREATE TABLE job_events (
    event_id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES jobs(job_id) ON DELETE RESTRICT,
    attempt_id TEXT REFERENCES attempts(attempt_id) ON DELETE RESTRICT,
    event_sequence INTEGER NOT NULL CHECK (event_sequence >= 1),
    event_type TEXT NOT NULL,
    origin TEXT NOT NULL CHECK (origin IN ('SYSTEM_OBSERVED','SYSTEM_DERIVED')),
    previous_state TEXT,
    new_state TEXT,
    reason_code TEXT NOT NULL,
    detail_json TEXT NOT NULL,
    detail_digest TEXT NOT NULL,
    observed_at_ms INTEGER NOT NULL CHECK (observed_at_ms >= 0),
    UNIQUE (job_id, event_sequence)
) STRICT;

CREATE TABLE artifacts (
    artifact_id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES jobs(job_id) ON DELETE RESTRICT,
    attempt_id TEXT NOT NULL REFERENCES attempts(attempt_id) ON DELETE RESTRICT,
    kind TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    digest TEXT NOT NULL,
    media_type TEXT NOT NULL,
    byte_length INTEGER NOT NULL CHECK (byte_length >= 0),
    truncated INTEGER NOT NULL CHECK (truncated IN (0,1)),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    UNIQUE (attempt_id, kind, relative_path)
) STRICT;

CREATE TABLE attempt_conditions (
    attempt_id TEXT NOT NULL REFERENCES attempts(attempt_id) ON DELETE RESTRICT,
    condition_type TEXT NOT NULL CHECK (condition_type IN ('bundle_ready','dispatch_issued','runner_bound','result_available','recovery_required','reservation_held')),
    status TEXT NOT NULL,
    reason_code TEXT NOT NULL,
    evidence_digest TEXT NOT NULL,
    observed_at_ms INTEGER NOT NULL CHECK (observed_at_ms >= 0),
    PRIMARY KEY (attempt_id, condition_type)
) STRICT;

CREATE INDEX idx_jobs_created ON jobs(created_at_ms, job_id);
CREATE INDEX idx_attempts_nonterminal ON attempts(state, created_at_ms);
CREATE INDEX idx_events_job_sequence ON job_events(job_id, event_sequence);
CREATE INDEX idx_reservations_active ON concurrency_reservations(state);

CREATE TRIGGER job_events_no_update
BEFORE UPDATE ON job_events
BEGIN
    SELECT RAISE(ABORT, 'job_events are append-only');
END;

CREATE TRIGGER job_events_no_delete
BEFORE DELETE ON job_events
BEGIN
    SELECT RAISE(ABORT, 'job_events are append-only');
END;

CREATE TRIGGER attempts_terminal_no_reopen
BEFORE UPDATE OF state ON attempts
WHEN OLD.state IN ('succeeded','failed','timed_out','cancelled','lost','orphaned')
     AND NEW.state <> OLD.state
BEGIN
    SELECT RAISE(ABORT, 'terminal attempt cannot reopen');
END;

CREATE TRIGGER jobs_resolution_immutable
BEFORE UPDATE OF resolution ON jobs
WHEN OLD.resolution IS NOT NULL AND NEW.resolution IS NOT OLD.resolution
BEGIN
    SELECT RAISE(ABORT, 'job resolution is immutable');
END;
