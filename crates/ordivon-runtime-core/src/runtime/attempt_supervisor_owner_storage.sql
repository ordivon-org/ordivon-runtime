CREATE TABLE IF NOT EXISTS attempt_supervisor_owners (
    attempt_id TEXT PRIMARY KEY NOT NULL REFERENCES attempts(attempt_id) ON DELETE CASCADE,
    owner_json TEXT NOT NULL,
    owner_digest TEXT NOT NULL
);
