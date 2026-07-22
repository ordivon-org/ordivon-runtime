DROP TRIGGER attempts_terminal_no_reopen;

CREATE TRIGGER attempts_terminal_no_reopen
BEFORE UPDATE OF state ON attempts
WHEN OLD.state IN ('succeeded','failed','timed_out','cancelled','lost','orphaned')
     AND NEW.state <> OLD.state
     AND NOT (
         OLD.state = 'orphaned'
         AND NEW.state IN ('succeeded','failed','timed_out','cancelled')
     )
BEGIN
    SELECT RAISE(ABORT, 'terminal attempt cannot reopen');
END;

DROP TRIGGER jobs_resolution_immutable;

CREATE TRIGGER jobs_resolution_immutable
BEFORE UPDATE OF resolution ON jobs
WHEN OLD.resolution IS NOT NULL
     AND NEW.resolution IS NOT OLD.resolution
     AND NOT (
         OLD.resolution = 'orphaned'
         AND NEW.resolution IN ('succeeded','failed','timed_out','cancelled')
     )
BEGIN
    SELECT RAISE(ABORT, 'job resolution is immutable');
END;
