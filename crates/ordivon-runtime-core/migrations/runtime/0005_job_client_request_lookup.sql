CREATE INDEX idx_jobs_client_request_id_created
ON jobs(client_request_id, created_at_ms, job_id);
