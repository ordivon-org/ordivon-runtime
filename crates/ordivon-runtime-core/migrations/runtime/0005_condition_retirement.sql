ALTER TABLE attempts ADD COLUMN recovery_required INTEGER CHECK (recovery_required IS NULL OR recovery_required IN (0,1));
ALTER TABLE attempts ADD COLUMN recovery_reason_code TEXT;
ALTER TABLE attempts ADD COLUMN recovery_evidence_digest TEXT;
ALTER TABLE attempts ADD COLUMN recovery_observed_at_ms INTEGER CHECK (recovery_observed_at_ms IS NULL OR recovery_observed_at_ms >= 0);

UPDATE attempts
SET recovery_required = (
        SELECT CASE c.status WHEN 'true' THEN 1 WHEN 'false' THEN 0 END
        FROM attempt_conditions c
        WHERE c.attempt_id=attempts.attempt_id AND c.condition_type='recovery_required'
    ),
    recovery_reason_code = (
        SELECT c.reason_code FROM attempt_conditions c
        WHERE c.attempt_id=attempts.attempt_id AND c.condition_type='recovery_required'
    ),
    recovery_evidence_digest = (
        SELECT c.evidence_digest FROM attempt_conditions c
        WHERE c.attempt_id=attempts.attempt_id AND c.condition_type='recovery_required'
    ),
    recovery_observed_at_ms = (
        SELECT c.observed_at_ms FROM attempt_conditions c
        WHERE c.attempt_id=attempts.attempt_id AND c.condition_type='recovery_required'
    )
WHERE EXISTS (
    SELECT 1 FROM attempt_conditions c
    WHERE c.attempt_id=attempts.attempt_id AND c.condition_type='recovery_required'
);

CREATE TRIGGER attempts_recovery_fields_consistent_insert
BEFORE INSERT ON attempts
WHEN NOT (
    (NEW.recovery_required IS NULL AND NEW.recovery_reason_code IS NULL AND NEW.recovery_evidence_digest IS NULL AND NEW.recovery_observed_at_ms IS NULL)
    OR
    (NEW.recovery_required IS NOT NULL AND NEW.recovery_reason_code IS NOT NULL AND NEW.recovery_evidence_digest IS NOT NULL AND NEW.recovery_observed_at_ms IS NOT NULL)
)
BEGIN
    SELECT RAISE(ABORT, 'attempt recovery fields must be all null or all present');
END;

CREATE TRIGGER attempts_recovery_fields_consistent_update
BEFORE UPDATE OF recovery_required,recovery_reason_code,recovery_evidence_digest,recovery_observed_at_ms ON attempts
WHEN NOT (
    (NEW.recovery_required IS NULL AND NEW.recovery_reason_code IS NULL AND NEW.recovery_evidence_digest IS NULL AND NEW.recovery_observed_at_ms IS NULL)
    OR
    (NEW.recovery_required IS NOT NULL AND NEW.recovery_reason_code IS NOT NULL AND NEW.recovery_evidence_digest IS NOT NULL AND NEW.recovery_observed_at_ms IS NOT NULL)
)
BEGIN
    SELECT RAISE(ABORT, 'attempt recovery fields must be all null or all present');
END;

ALTER TABLE concurrency_reservations ADD COLUMN state_observed_at_ms INTEGER CHECK (state_observed_at_ms IS NULL OR state_observed_at_ms >= 0);

UPDATE concurrency_reservations
SET state_observed_at_ms = CASE
    WHEN state='active' THEN acquired_at_ms
    WHEN state='released' THEN released_at_ms
    WHEN state='held_orphaned' THEN COALESCE(
        (SELECT c.observed_at_ms FROM attempt_conditions c
         WHERE c.attempt_id=concurrency_reservations.attempt_id
           AND c.condition_type='reservation_held'),
        acquired_at_ms
    )
END;

CREATE TRIGGER reservations_state_observed_present_insert
BEFORE INSERT ON concurrency_reservations
WHEN NEW.state_observed_at_ms IS NULL
BEGIN
    SELECT RAISE(ABORT, 'reservation state_observed_at_ms is required');
END;

CREATE TRIGGER reservations_state_observed_present_update
BEFORE UPDATE OF state,state_observed_at_ms ON concurrency_reservations
WHEN NEW.state_observed_at_ms IS NULL
BEGIN
    SELECT RAISE(ABORT, 'reservation state_observed_at_ms is required');
END;

DROP TABLE attempt_conditions;
