use rusqlite::{
    params, Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use super::{
    AdmissionOutcomeM6, ArtifactRecordM6, ArtifactRegistrationM6, AttemptRecordM6, AttemptState,
    ConditionUpdateM6, CreatedAdmissionM6, JobDesiredState, JobListCursorM6, JobListRequestM6,
    JobListResultM6, JobProjectionM6, JobRecordM6, JobResolution, M6Error, M6ErrorCode, M6Result,
    M6SubmitRequest, M6TerminationIntent, PlanKind, ReservationRecordM6, ReservationState,
    RunnerIdentityM6, TerminalCommitM6, M6_SCHEMA_VERSION, MAX_M6_LIST_LIMIT,
};

const MIGRATION_VERSION: i64 = 1;
const MIGRATION_NAME: &str = "0001_initial";
const MIGRATION_SQL: &str = include_str!("../../migrations/m6/0001_initial.sql");
pub const M6_MIGRATION_CHECKSUM: &str =
    "sha256:73b1462cdfe91640af266eb55c953a32149fccac53c0bada3b11c9e84fa1a78f";

#[derive(Clone, Debug)]
pub struct M6RegistryConfig {
    pub db_path: PathBuf,
    pub store_root: PathBuf,
    pub busy_timeout_ms: u64,
}

#[derive(Clone, Debug)]
pub struct M6Registry {
    config: M6RegistryConfig,
}

impl M6RegistryConfig {
    pub fn validate(&self) -> M6Result<()> {
        if !self.db_path.is_absolute() {
            return Err(M6Error::invalid("database path must be absolute", "dbPath"));
        }
        if !self.store_root.is_absolute() {
            return Err(M6Error::invalid("store root must be absolute", "storeRoot"));
        }
        if self.busy_timeout_ms == 0 || self.busy_timeout_ms > 60_000 {
            return Err(M6Error::invalid(
                "busy timeout must be in 1..=60000",
                "busyTimeoutMs",
            ));
        }
        Ok(())
    }

    pub fn attempts_root(&self) -> PathBuf {
        self.store_root.join("attempts")
    }

    pub fn attempt_path(&self, attempt_id: &str) -> PathBuf {
        self.attempts_root().join(attempt_id)
    }
}

impl M6Registry {
    pub fn initialize(config: M6RegistryConfig) -> M6Result<Self> {
        config.validate()?;
        create_private_directory(&config.store_root)?;
        create_private_directory(&config.attempts_root())?;
        if let Some(parent) = config.db_path.parent() {
            create_private_directory(parent)?;
        }
        let registry = Self { config };
        let mut connection = registry.open_connection()?;
        registry.apply_migrations(&mut connection)?;
        registry.validate_database(&connection)?;
        set_private_file(&registry.config.db_path)?;
        Ok(registry)
    }

    pub fn config(&self) -> &M6RegistryConfig {
        &self.config
    }

    fn open_connection(&self) -> M6Result<Connection> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection = Connection::open_with_flags(&self.config.db_path, flags)
            .map_err(|error| M6Error::from_sql(error, "cannot open M6 registry"))?;
        connection
            .busy_timeout(Duration::from_millis(self.config.busy_timeout_ms))
            .map_err(|error| M6Error::from_sql(error, "cannot set registry busy timeout"))?;
        connection
            .pragma_update(None, "foreign_keys", true)
            .map_err(|error| M6Error::from_sql(error, "cannot enable foreign keys"))?;
        connection
            .pragma_update(None, "trusted_schema", false)
            .map_err(|error| M6Error::from_sql(error, "cannot disable trusted schema"))?;
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(|error| M6Error::from_sql(error, "cannot set synchronous mode"))?;
        let mode: String = connection
            .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
            .map_err(|error| M6Error::from_sql(error, "cannot enable WAL mode"))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(M6Error::new(
                M6ErrorCode::RegistryUnavailable,
                format!("SQLite refused WAL mode and returned {mode}"),
                None,
                false,
            ));
        }
        Ok(connection)
    }

    fn apply_migrations(&self, connection: &mut Connection) -> M6Result<()> {
        let has_table: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations')",
                [],
                |row| row.get(0),
            )
            .map_err(|error| M6Error::from_sql(error, "cannot inspect schema migrations"))?;
        if !has_table {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|error| M6Error::from_sql(error, "cannot start migration transaction"))?;
            transaction
                .execute_batch(MIGRATION_SQL)
                .map_err(|error| M6Error::from_sql(error, "cannot apply initial migration"))?;
            transaction
                .execute(
                    "INSERT INTO schema_migrations(version,name,checksum,applied_at_ms) VALUES(?1,?2,?3,?4)",
                    params![MIGRATION_VERSION, MIGRATION_NAME, M6_MIGRATION_CHECKSUM, now_ms()?],
                )
                .map_err(|error| M6Error::from_sql(error, "cannot record initial migration"))?;
            transaction
                .commit()
                .map_err(|error| M6Error::from_sql(error, "cannot commit initial migration"))?;
            return Ok(());
        }

        let max_version: Option<i64> = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .map_err(|error| M6Error::from_sql(error, "cannot read migration version"))?;
        let Some(max_version) = max_version else {
            return Err(M6Error::new(
                M6ErrorCode::RegistryCorrupt,
                "schema_migrations is empty",
                None,
                false,
            ));
        };
        if max_version > MIGRATION_VERSION {
            return Err(M6Error::new(
                M6ErrorCode::SchemaVersionUnsupported,
                format!(
                    "registry schema {max_version} is newer than supported {MIGRATION_VERSION}"
                ),
                None,
                false,
            ));
        }
        let checksum: String = connection
            .query_row(
                "SELECT checksum FROM schema_migrations WHERE version=?1",
                [MIGRATION_VERSION],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| M6Error::from_sql(error, "cannot read migration checksum"))?
            .ok_or_else(|| {
                M6Error::new(
                    M6ErrorCode::RegistryCorrupt,
                    "required migration is missing",
                    None,
                    false,
                )
            })?;
        if checksum != M6_MIGRATION_CHECKSUM {
            return Err(M6Error::new(
                M6ErrorCode::MigrationChecksumMismatch,
                "initial migration checksum does not match the compiled migration",
                None,
                false,
            ));
        }
        Ok(())
    }

    fn validate_database(&self, connection: &Connection) -> M6Result<()> {
        let quick: String = connection
            .query_row("PRAGMA quick_check(20)", [], |row| row.get(0))
            .map_err(|error| M6Error::from_sql(error, "registry quick_check failed"))?;
        if quick != "ok" {
            return Err(M6Error::new(
                M6ErrorCode::RegistryCorrupt,
                format!("registry quick_check returned {quick}"),
                None,
                false,
            ));
        }
        let foreign_key_problem: Option<String> = connection
            .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
            .optional()
            .map_err(|error| M6Error::from_sql(error, "foreign key check failed"))?;
        if let Some(table) = foreign_key_problem {
            return Err(M6Error::new(
                M6ErrorCode::RegistryCorrupt,
                format!("foreign key violation in {table}"),
                None,
                false,
            ));
        }
        Ok(())
    }

    pub fn submit(&self, request: &M6SubmitRequest) -> M6Result<AdmissionOutcomeM6> {
        validate_submit(request)?;
        let created_at_ms = now_ms()?;
        let plan_json = serde_json::to_string(&request.plan).map_err(|error| {
            M6Error::new(
                M6ErrorCode::InvalidRequest,
                format!("cannot serialize execution plan: {error}"),
                Some("plan"),
                false,
            )
        })?;
        let request_json = serde_json::to_vec(request).map_err(|error| {
            M6Error::new(
                M6ErrorCode::InvalidRequest,
                format!("cannot serialize submit request: {error}"),
                None,
                false,
            )
        })?;
        let request_digest = sha256_bytes(&request_json);
        let plan_digest = sha256_bytes(plan_json.as_bytes());
        let workspace_snapshot_json = serde_json::json!({
            "workspaceId": request.plan.workspace_id,
            "workspacePath": request.plan.workspace_path,
            "sourceRevision": request.plan.source_revision,
        })
        .to_string();
        let operation_digest = sha256_bytes(
            format!(
                "m6-operation-v1\0{request_digest}\0{plan_digest}\0{}\0{}",
                request.plan.policy_digest, request.plan.authority_ref
            )
            .as_bytes(),
        );
        let job_id = format!("job-{}", Uuid::now_v7());
        let attempt_id = format!("attempt-{}", Uuid::now_v7());
        let reservation_id = format!("reservation-{}", Uuid::now_v7());
        let launch_token =
            sha256_bytes(format!("m6-launch-v1\0{attempt_id}\0{operation_digest}").as_bytes());
        let launch_token_digest = sha256_bytes(launch_token.as_bytes());
        let unit_name = format!("ordivon-m6-{attempt_id}.service");
        let bundle_path = self
            .config
            .attempt_path(&attempt_id)
            .to_string_lossy()
            .into_owned();

        let job = JobRecordM6 {
            job_id: job_id.clone(),
            principal: request.plan.principal.clone(),
            client_request_id: request.client_request_id.clone(),
            request_digest: request_digest.clone(),
            operation_digest: operation_digest.clone(),
            workspace_id: request.plan.workspace_id.clone(),
            workspace_snapshot_json: workspace_snapshot_json.clone(),
            plan_kind: request.plan.plan_kind,
            execution_plan_json: plan_json.clone(),
            execution_plan_digest: plan_digest.clone(),
            policy_id: request.plan.policy_id.clone(),
            policy_version: request.plan.policy_version.clone(),
            policy_digest: request.plan.policy_digest.clone(),
            authority_ref: request.plan.authority_ref.clone(),
            profile_id: request.plan.profile_id.clone(),
            created_at_ms,
            desired_state: JobDesiredState::Run,
            resolution: None,
            current_attempt_id: Some(attempt_id.clone()),
            row_version: 0,
        };
        let attempt = AttemptRecordM6 {
            attempt_id: attempt_id.clone(),
            job_id: job_id.clone(),
            attempt_number: 1,
            state: AttemptState::Accepted,
            termination_intent: M6TerminationIntent::Natural,
            launch_token_digest: launch_token_digest.clone(),
            bundle_path: bundle_path.clone(),
            bundle_digest: None,
            boot_id: None,
            unit_name: unit_name.clone(),
            invocation_id: None,
            control_group: None,
            main_pid: None,
            process_start_identity: None,
            runner_start_digest: None,
            result_digest: None,
            exit_code: None,
            infrastructure_error_digest: None,
            created_at_ms,
            started_at_ms: None,
            finished_at_ms: None,
            row_version: 0,
        };
        let reservation = ReservationRecordM6 {
            reservation_id: reservation_id.clone(),
            attempt_id: attempt_id.clone(),
            profile_id: request.plan.profile_id.clone(),
            global_limit: request.global_limit,
            profile_limit: request.profile_limit,
            state: ReservationState::Active,
            acquired_at_ms: created_at_ms,
            released_at_ms: None,
            release_reason: None,
        };

        let mut connection = self.open_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| M6Error::from_sql(error, "cannot begin admission transaction"))?;

        if let Some((existing_digest, existing_job_id)) = transaction
            .query_row(
                "SELECT operation_digest, job_id FROM idempotency_keys WHERE principal=?1 AND client_request_id=?2",
                params![request.plan.principal, request.client_request_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|error| M6Error::from_sql(error, "cannot check idempotency key"))?
        {
            if existing_digest != operation_digest {
                return Err(M6Error::new(
                    M6ErrorCode::IdempotencyConflict,
                    "clientRequestId is already bound to a different operation",
                    Some("clientRequestId"),
                    false,
                ));
            }
            let existing = load_job(&transaction, &existing_job_id)?;
            transaction
                .commit()
                .map_err(|error| M6Error::from_sql(error, "cannot close replay transaction"))?;
            return Ok(AdmissionOutcomeM6::Existing {
                job: Box::new(existing),
            });
        }

        let global_active: u32 = transaction
            .query_row(
                "SELECT COUNT(*) FROM concurrency_reservations WHERE state IN ('active','held_orphaned')",
                [],
                |row| row.get(0),
            )
            .map_err(|error| M6Error::from_sql(error, "cannot count global reservations"))?;
        if global_active >= request.global_limit {
            return Err(M6Error::new(
                M6ErrorCode::ConcurrencyLimit,
                "global execution concurrency limit reached",
                Some("globalLimit"),
                true,
            ));
        }
        if let (Some(profile_id), Some(profile_limit)) =
            (&request.plan.profile_id, request.profile_limit)
        {
            let profile_active: u32 = transaction
                .query_row(
                    "SELECT COUNT(*) FROM concurrency_reservations WHERE profile_id=?1 AND state IN ('active','held_orphaned')",
                    [profile_id],
                    |row| row.get(0),
                )
                .map_err(|error| M6Error::from_sql(error, "cannot count profile reservations"))?;
            if profile_active >= profile_limit {
                return Err(M6Error::new(
                    M6ErrorCode::ConcurrencyLimit,
                    format!("profile {profile_id} concurrency limit reached"),
                    Some("profileLimit"),
                    true,
                ));
            }
        }

        transaction
            .execute(
                "INSERT INTO jobs(job_id,principal,client_request_id,request_digest,operation_digest,workspace_id,workspace_snapshot_json,plan_kind,execution_plan_json,execution_plan_digest,policy_id,policy_version,policy_digest,authority_ref,profile_id,created_at_ms,desired_state,resolution,current_attempt_id,row_version) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,NULL,?18,0)",
                params![
                    job.job_id,
                    job.principal,
                    job.client_request_id,
                    job.request_digest,
                    job.operation_digest,
                    job.workspace_id,
                    job.workspace_snapshot_json,
                    job.plan_kind.as_db(),
                    job.execution_plan_json,
                    job.execution_plan_digest,
                    job.policy_id,
                    job.policy_version,
                    job.policy_digest,
                    job.authority_ref,
                    job.profile_id,
                    created_at_ms,
                    job.desired_state.as_db(),
                    attempt.attempt_id,
                ],
            )
            .map_err(|error| M6Error::from_sql(error, "cannot insert Job"))?;
        transaction
            .execute(
                "INSERT INTO attempts(attempt_id,job_id,attempt_number,state,termination_intent,launch_token_digest,bundle_path,bundle_digest,boot_id,unit_name,invocation_id,control_group,main_pid,process_start_identity,runner_start_digest,result_digest,exit_code,infrastructure_error_digest,created_at_ms,started_at_ms,finished_at_ms,row_version) VALUES(?1,?2,1,?3,?4,?5,?6,NULL,NULL,?7,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,?8,NULL,NULL,0)",
                params![
                    attempt.attempt_id,
                    attempt.job_id,
                    attempt.state.as_db(),
                    attempt.termination_intent.as_db(),
                    attempt.launch_token_digest,
                    attempt.bundle_path,
                    attempt.unit_name,
                    created_at_ms,
                ],
            )
            .map_err(|error| M6Error::from_sql(error, "cannot insert Attempt"))?;
        transaction
            .execute(
                "INSERT INTO idempotency_keys(principal,client_request_id,operation_digest,job_id,created_at_ms) VALUES(?1,?2,?3,?4,?5)",
                params![
                    request.plan.principal,
                    request.client_request_id,
                    operation_digest,
                    job_id,
                    created_at_ms,
                ],
            )
            .map_err(|error| M6Error::from_sql(error, "cannot insert idempotency key"))?;

        transaction
            .execute(
                "INSERT INTO concurrency_reservations(reservation_id,attempt_id,profile_id,global_limit,profile_limit,state,acquired_at_ms,released_at_ms,release_reason) VALUES(?1,?2,?3,?4,?5,?6,?7,NULL,NULL)",
                params![
                    reservation.reservation_id,
                    reservation.attempt_id,
                    reservation.profile_id,
                    reservation.global_limit,
                    reservation.profile_limit,
                    reservation.state.as_db(),
                    reservation.acquired_at_ms,
                ],
            )
            .map_err(|error| M6Error::from_sql(error, "cannot reserve execution capacity"))?;

        append_event(
            &transaction,
            &job_id,
            Some(&attempt_id),
            "REQUEST_RECEIVED",
            "SYSTEM_DERIVED",
            None,
            None,
            "REQUEST_ACCEPTED",
            serde_json::json!({"requestDigest": request_digest}),
            created_at_ms,
        )?;
        append_event(
            &transaction,
            &job_id,
            Some(&attempt_id),
            "AUTHORIZATION_ALLOWED",
            "SYSTEM_DERIVED",
            None,
            None,
            "POLICY_AND_AUTHORITY_BOUND",
            serde_json::json!({
                "policyDigest": request.plan.policy_digest,
                "authorityRef": request.plan.authority_ref,
            }),
            created_at_ms,
        )?;
        append_event(
            &transaction,
            &job_id,
            Some(&attempt_id),
            "JOB_RECORD_CREATED",
            "SYSTEM_DERIVED",
            None,
            None,
            "JOB_CREATED",
            serde_json::json!({"operationDigest": operation_digest}),
            created_at_ms,
        )?;
        append_event(
            &transaction,
            &job_id,
            Some(&attempt_id),
            "ATTEMPT_CREATED",
            "SYSTEM_DERIVED",
            None,
            Some(AttemptState::Accepted),
            "ATTEMPT_ACCEPTED",
            serde_json::json!({"attemptNumber": 1}),
            created_at_ms,
        )?;

        upsert_condition(
            &transaction,
            &attempt_id,
            &ConditionUpdateM6 {
                condition_type: "reservation_held".to_string(),
                status: "true".to_string(),
                reason_code: "CAPACITY_RESERVED".to_string(),
                evidence_digest: sha256_bytes(reservation_id.as_bytes()),
                observed_at_ms: created_at_ms,
            },
        )?;
        transaction
            .commit()
            .map_err(|error| M6Error::from_sql(error, "cannot commit admission"))?;
        Ok(AdmissionOutcomeM6::Created(Box::new(CreatedAdmissionM6 {
            job,
            attempt,
            reservation,
            launch_token,
        })))
    }

    pub fn get_job(&self, job_id: &str) -> M6Result<JobRecordM6> {
        let connection = self.open_connection()?;
        load_job(&connection, job_id)
    }

    pub fn get_attempt(&self, attempt_id: &str) -> M6Result<AttemptRecordM6> {
        let connection = self.open_connection()?;
        load_attempt(&connection, attempt_id)
    }

    pub fn get_current_attempt(&self, job_id: &str) -> M6Result<Option<AttemptRecordM6>> {
        let connection = self.open_connection()?;
        let attempt_id: Option<String> = connection
            .query_row(
                "SELECT current_attempt_id FROM jobs WHERE job_id=?1",
                [job_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| M6Error::from_sql(error, "cannot read current Attempt"))?
            .ok_or_else(|| {
                M6Error::new(
                    M6ErrorCode::JobNotFound,
                    "Job not found",
                    Some("jobId"),
                    false,
                )
            })?;
        attempt_id
            .map(|attempt_id| load_attempt(&connection, &attempt_id))
            .transpose()
    }

    pub fn get_reservation(&self, attempt_id: &str) -> M6Result<ReservationRecordM6> {
        let connection = self.open_connection()?;
        load_reservation(&connection, attempt_id)
    }

    pub fn project_job(&self, job_id: &str) -> M6Result<JobProjectionM6> {
        let job = self.get_job(job_id)?;
        let attempt = job
            .current_attempt_id
            .as_deref()
            .map(|attempt_id| self.get_attempt(attempt_id))
            .transpose()?;
        Ok(project_job(&job, attempt.as_ref()))
    }

    pub fn list_jobs(&self, request: &JobListRequestM6) -> M6Result<JobListResultM6> {
        if request.limit == 0 || request.limit > MAX_M6_LIST_LIMIT {
            return Err(M6Error::invalid(
                format!("limit must be in 1..={MAX_M6_LIST_LIMIT}"),
                "limit",
            ));
        }
        let connection = self.open_connection()?;
        let fetch_limit = request.limit + 1;
        let mut jobs = Vec::new();
        if let Some(cursor) = &request.cursor {
            let mut statement = connection
                .prepare(
                    "SELECT job_id,principal,client_request_id,request_digest,operation_digest,workspace_id,workspace_snapshot_json,plan_kind,execution_plan_json,execution_plan_digest,policy_id,policy_version,policy_digest,authority_ref,profile_id,created_at_ms,desired_state,resolution,current_attempt_id,row_version FROM jobs WHERE created_at_ms>?1 OR (created_at_ms=?1 AND job_id>?2) ORDER BY created_at_ms,job_id LIMIT ?3",
                )
                .map_err(|error| M6Error::from_sql(error, "cannot prepare Job list"))?;
            let rows = statement
                .query_map(
                    params![cursor.created_at_ms, cursor.job_id, fetch_limit],
                    raw_job_from_row,
                )
                .map_err(|error| M6Error::from_sql(error, "cannot query Job list"))?;
            for row in rows {
                jobs.push(
                    row.map_err(|error| M6Error::from_sql(error, "cannot decode Job row"))?
                        .into_record()?,
                );
            }
        } else {
            let mut statement = connection
                .prepare(
                    "SELECT job_id,principal,client_request_id,request_digest,operation_digest,workspace_id,workspace_snapshot_json,plan_kind,execution_plan_json,execution_plan_digest,policy_id,policy_version,policy_digest,authority_ref,profile_id,created_at_ms,desired_state,resolution,current_attempt_id,row_version FROM jobs ORDER BY created_at_ms,job_id LIMIT ?1",
                )
                .map_err(|error| M6Error::from_sql(error, "cannot prepare Job list"))?;
            let rows = statement
                .query_map([fetch_limit], raw_job_from_row)
                .map_err(|error| M6Error::from_sql(error, "cannot query Job list"))?;
            for row in rows {
                jobs.push(
                    row.map_err(|error| M6Error::from_sql(error, "cannot decode Job row"))?
                        .into_record()?,
                );
            }
        }

        let has_more = jobs.len() > request.limit as usize;
        jobs.truncate(request.limit as usize);
        let next_cursor = if has_more {
            jobs.last().map(|job| JobListCursorM6 {
                created_at_ms: job.created_at_ms,
                job_id: job.job_id.clone(),
            })
        } else {
            None
        };
        let mut projections = Vec::with_capacity(jobs.len());
        for job in jobs {
            let attempt = job
                .current_attempt_id
                .as_deref()
                .map(|attempt_id| load_attempt(&connection, attempt_id))
                .transpose()?;
            projections.push(project_job(&job, attempt.as_ref()));
        }
        Ok(JobListResultM6 {
            jobs: projections,
            next_cursor,
        })
    }

    pub fn list_artifacts(&self, job_id: &str) -> M6Result<Vec<ArtifactRecordM6>> {
        let connection = self.open_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT artifact_id,job_id,attempt_id,kind,relative_path,digest,media_type,byte_length,truncated,created_at_ms FROM artifacts WHERE job_id=?1 ORDER BY created_at_ms,artifact_id",
            )
            .map_err(|error| M6Error::from_sql(error, "cannot prepare Artifact query"))?;
        let rows = statement
            .query_map([job_id], |row| {
                Ok(ArtifactRecordM6 {
                    artifact_id: row.get(0)?,
                    job_id: row.get(1)?,
                    attempt_id: row.get(2)?,
                    kind: row.get(3)?,
                    relative_path: row.get(4)?,
                    digest: row.get(5)?,
                    media_type: row.get(6)?,
                    byte_length: row.get(7)?,
                    truncated: row.get::<_, i64>(8)? != 0,
                    created_at_ms: row.get(9)?,
                })
            })
            .map_err(|error| M6Error::from_sql(error, "cannot query Artifacts"))?;
        rows.map(|row| row.map_err(|error| M6Error::from_sql(error, "cannot decode Artifact")))
            .collect()
    }

    pub fn execution_plan(&self, job_id: &str) -> M6Result<super::M6ExecutionPlan> {
        let job = self.get_job(job_id)?;
        serde_json::from_str(&job.execution_plan_json).map_err(|error| {
            M6Error::new(
                M6ErrorCode::RegistryCorrupt,
                format!("stored execution plan is invalid: {error}"),
                Some("executionPlan"),
                false,
            )
        })
    }

    pub fn launch_token(&self, attempt_id: &str) -> M6Result<String> {
        let attempt = self.get_attempt(attempt_id)?;
        let job = self.get_job(&attempt.job_id)?;
        let token = sha256_bytes(
            format!(
                "m6-launch-v1\0{}\0{}",
                attempt.attempt_id, job.operation_digest
            )
            .as_bytes(),
        );
        if sha256_bytes(token.as_bytes()) != attempt.launch_token_digest {
            return Err(M6Error::new(
                M6ErrorCode::RegistryCorrupt,
                "stored launch-token digest is inconsistent",
                Some("launchTokenDigest"),
                false,
            ));
        }
        Ok(token)
    }

    pub fn mark_bundle_ready(
        &self,
        attempt_id: &str,
        expected_row_version: u64,
        bundle_digest: &str,
        observed_at_ms: u64,
    ) -> M6Result<AttemptRecordM6> {
        validate_digest(bundle_digest, "bundleDigest")?;
        let mut connection = self.open_connection()?;
        let transaction = immediate(&mut connection, "bundle-ready transaction")?;
        let attempt = load_attempt(&transaction, attempt_id)?;
        if attempt.state != AttemptState::Accepted || attempt.row_version != expected_row_version {
            return Err(state_conflict(
                "Attempt is not the expected accepted version",
            ));
        }
        let changed = transaction
            .execute(
                "UPDATE attempts SET bundle_digest=?1,row_version=row_version+1 WHERE attempt_id=?2 AND state='accepted' AND row_version=?3",
                params![bundle_digest, attempt_id, expected_row_version],
            )
            .map_err(|error| M6Error::from_sql(error, "cannot bind bundle identity"))?;
        if changed != 1 {
            return Err(state_conflict("Attempt changed while binding bundle"));
        }
        upsert_condition(
            &transaction,
            attempt_id,
            &ConditionUpdateM6 {
                condition_type: "bundle_ready".to_string(),
                status: "true".to_string(),
                reason_code: "BUNDLE_COMMITTED".to_string(),
                evidence_digest: bundle_digest.to_string(),
                observed_at_ms,
            },
        )?;
        append_event(
            &transaction,
            &attempt.job_id,
            Some(attempt_id),
            "BUNDLE_READY",
            "SYSTEM_OBSERVED",
            Some(AttemptState::Accepted),
            Some(AttemptState::Accepted),
            "BUNDLE_COMMITTED",
            serde_json::json!({"bundleDigest": bundle_digest}),
            observed_at_ms,
        )?;
        transaction
            .commit()
            .map_err(|error| M6Error::from_sql(error, "cannot commit bundle identity"))?;
        self.get_attempt(attempt_id)
    }

    pub fn mark_dispatch_issued(
        &self,
        attempt_id: &str,
        expected_row_version: u64,
        observed_at_ms: u64,
    ) -> M6Result<AttemptRecordM6> {
        let mut connection = self.open_connection()?;
        let transaction = immediate(&mut connection, "dispatch-intent transaction")?;
        let attempt = load_attempt(&transaction, attempt_id)?;
        if attempt.state != AttemptState::Accepted
            || attempt.row_version != expected_row_version
            || attempt.bundle_digest.is_none()
        {
            return Err(state_conflict(
                "Attempt must be accepted with a committed bundle",
            ));
        }
        let changed = transaction
            .execute(
                "UPDATE attempts SET state='starting',row_version=row_version+1 WHERE attempt_id=?1 AND state='accepted' AND row_version=?2 AND bundle_digest IS NOT NULL",
                params![attempt_id, expected_row_version],
            )
            .map_err(|error| M6Error::from_sql(error, "cannot persist dispatch intent"))?;
        if changed != 1 {
            return Err(state_conflict("Attempt changed before dispatch intent"));
        }
        let evidence = attempt.bundle_digest.clone().unwrap_or_default();
        upsert_condition(
            &transaction,
            attempt_id,
            &ConditionUpdateM6 {
                condition_type: "dispatch_issued".to_string(),
                status: "true".to_string(),
                reason_code: "AT_MOST_ONCE_BOUNDARY_COMMITTED".to_string(),
                evidence_digest: evidence.clone(),
                observed_at_ms,
            },
        )?;
        append_event(
            &transaction,
            &attempt.job_id,
            Some(attempt_id),
            "DISPATCH_ISSUED",
            "SYSTEM_DERIVED",
            Some(AttemptState::Accepted),
            Some(AttemptState::Starting),
            "AT_MOST_ONCE_BOUNDARY_COMMITTED",
            serde_json::json!({"bundleDigest": evidence}),
            observed_at_ms,
        )?;
        transaction
            .commit()
            .map_err(|error| M6Error::from_sql(error, "cannot commit dispatch intent"))?;
        self.get_attempt(attempt_id)
    }

    pub fn bind_running(
        &self,
        attempt_id: &str,
        expected_row_version: u64,
        identity: &RunnerIdentityM6,
    ) -> M6Result<AttemptRecordM6> {
        validate_runner_identity(identity)?;
        let mut connection = self.open_connection()?;
        let transaction = immediate(&mut connection, "runner-bind transaction")?;
        let attempt = load_attempt(&transaction, attempt_id)?;
        if !matches!(
            attempt.state,
            AttemptState::Starting | AttemptState::Recovering
        ) || attempt.row_version != expected_row_version
            || attempt.unit_name != identity.unit_name
        {
            return Err(state_conflict(
                "Attempt is not bindable to this Runner identity",
            ));
        }

        let changed = transaction
            .execute(
                "UPDATE attempts SET state='running',boot_id=?1,invocation_id=?2,control_group=?3,main_pid=?4,process_start_identity=?5,runner_start_digest=?6,started_at_ms=COALESCE(started_at_ms,?7),row_version=row_version+1 WHERE attempt_id=?8 AND row_version=?9 AND state IN ('starting','recovering')",
                params![
                    identity.boot_id,
                    identity.invocation_id,
                    identity.control_group,
                    identity.main_pid,
                    identity.process_start_identity,
                    identity.runner_start_digest,
                    identity.observed_at_ms,
                    attempt_id,
                    expected_row_version,
                ],
            )
            .map_err(|error| M6Error::from_sql(error, "cannot bind Runner identity"))?;
        if changed != 1 {
            return Err(state_conflict("Attempt changed while binding Runner"));
        }
        upsert_condition(
            &transaction,
            attempt_id,
            &ConditionUpdateM6 {
                condition_type: "runner_bound".to_string(),
                status: "true".to_string(),
                reason_code: "RUNNER_IDENTITY_MATCHED".to_string(),
                evidence_digest: identity.runner_start_digest.clone(),
                observed_at_ms: identity.observed_at_ms,
            },
        )?;
        append_event(
            &transaction,
            &attempt.job_id,
            Some(attempt_id),
            "RUNNER_BOUND",
            "SYSTEM_OBSERVED",
            Some(attempt.state),
            Some(AttemptState::Running),
            "RUNNER_IDENTITY_MATCHED",
            serde_json::json!({
                "bootId": identity.boot_id,
                "unitName": identity.unit_name,
                "invocationId": identity.invocation_id,
                "controlGroup": identity.control_group,
                "mainPid": identity.main_pid,
                "processStartIdentity": identity.process_start_identity,
            }),
            identity.observed_at_ms,
        )?;
        transaction
            .commit()
            .map_err(|error| M6Error::from_sql(error, "cannot commit Runner identity"))?;
        self.get_attempt(attempt_id)
    }

    pub fn request_cancel(&self, job_id: &str, observed_at_ms: u64) -> M6Result<JobProjectionM6> {
        let mut connection = self.open_connection()?;
        let transaction = immediate(&mut connection, "cancel-intent transaction")?;
        let job = load_job(&transaction, job_id)?;
        if job.resolution.is_some() {
            let attempt = job
                .current_attempt_id
                .as_deref()
                .map(|attempt_id| load_attempt(&transaction, attempt_id))
                .transpose()?;
            transaction
                .commit()
                .map_err(|error| M6Error::from_sql(error, "cannot close terminal cancel replay"))?;
            return Ok(project_job(&job, attempt.as_ref()));
        }
        let attempt_id = job.current_attempt_id.clone().ok_or_else(|| {
            M6Error::new(
                M6ErrorCode::RegistryCorrupt,
                "unresolved Job has no current Attempt",
                Some("currentAttemptId"),
                false,
            )
        })?;
        let attempt = load_attempt(&transaction, &attempt_id)?;
        if attempt.state.is_terminal() {
            return Err(M6Error::new(
                M6ErrorCode::ReconciliationRequired,
                "Attempt is terminal but Job is unresolved",
                Some("jobId"),
                false,
            ));
        }
        transaction
            .execute(
                "UPDATE jobs SET desired_state='cancelled',row_version=row_version+1 WHERE job_id=?1 AND resolution IS NULL",
                [job_id],
            )
            .map_err(|error| M6Error::from_sql(error, "cannot persist cancel intent"))?;
        if attempt.state == AttemptState::Accepted {
            let result_digest = sha256_bytes(
                format!("m6-cancel-before-dispatch\0{job_id}\0{attempt_id}").as_bytes(),
            );
            transaction
                .execute(
                    "UPDATE attempts SET state='cancelled',termination_intent='stop_requested',result_digest=?1,finished_at_ms=?2,row_version=row_version+1 WHERE attempt_id=?3 AND state='accepted'",
                    params![result_digest, observed_at_ms, attempt_id],
                )
                .map_err(|error| M6Error::from_sql(error, "cannot cancel accepted Attempt"))?;
            release_reservation(
                &transaction,
                &attempt_id,
                observed_at_ms,
                "CANCELLED_BEFORE_DISPATCH",
            )?;
            transaction
                .execute(
                    "UPDATE jobs SET resolution='cancelled',current_attempt_id=NULL,row_version=row_version+1 WHERE job_id=?1 AND resolution IS NULL",
                    [job_id],
                )
                .map_err(|error| M6Error::from_sql(error, "cannot resolve cancelled Job"))?;
            append_event(
                &transaction,
                job_id,
                Some(&attempt_id),
                "STOP_REQUESTED",
                "SYSTEM_DERIVED",
                Some(AttemptState::Accepted),
                Some(AttemptState::Cancelled),
                "CANCELLED_BEFORE_DISPATCH",
                serde_json::json!({}),
                observed_at_ms,
            )?;
        } else if attempt.state != AttemptState::Stopping {
            transaction
                .execute(
                    "UPDATE attempts SET state='stopping',termination_intent='stop_requested',row_version=row_version+1 WHERE attempt_id=?1 AND state IN ('starting','running','recovering')",
                    [&attempt_id],
                )
                .map_err(|error| M6Error::from_sql(error, "cannot move Attempt to stopping"))?;
            append_event(
                &transaction,
                job_id,
                Some(&attempt_id),
                "STOP_REQUESTED",
                "SYSTEM_DERIVED",
                Some(attempt.state),
                Some(AttemptState::Stopping),
                "CANCEL_INTENT_COMMITTED",
                serde_json::json!({}),
                observed_at_ms,
            )?;
        }
        transaction
            .commit()
            .map_err(|error| M6Error::from_sql(error, "cannot commit cancel intent"))?;
        self.project_job(job_id)
    }

    pub fn commit_terminal(&self, request: &TerminalCommitM6) -> M6Result<JobProjectionM6> {
        if !request.state.is_terminal() {
            return Err(M6Error::invalid(
                "terminal commit requires a terminal Attempt state",
                "state",
            ));
        }
        validate_digest(&request.result_digest, "resultDigest")?;
        for artifact in &request.artifacts {
            validate_artifact_registration(artifact)?;
        }
        let mut connection = self.open_connection()?;
        let transaction = immediate(&mut connection, "terminal transaction")?;
        let attempt = load_attempt(&transaction, &request.attempt_id)?;
        let job = load_job(&transaction, &attempt.job_id)?;
        if attempt.state.is_terminal() {
            if attempt.result_digest.as_deref() == Some(request.result_digest.as_str())
                && attempt.state == request.state
            {
                transaction
                    .commit()
                    .map_err(|error| M6Error::from_sql(error, "cannot close terminal replay"))?;
                return self.project_job(&job.job_id);
            }
            return Err(M6Error::new(
                M6ErrorCode::ResultIdentityConflict,
                "Attempt already has a different terminal result",
                Some("resultDigest"),
                false,
            ));
        }
        if attempt.row_version != request.expected_row_version {
            return Err(state_conflict(
                "Attempt row version changed before terminal commit",
            ));
        }
        if job.resolution.is_some() {
            return Err(M6Error::new(
                M6ErrorCode::JobAlreadyResolved,
                "Job is already resolved",
                Some("jobId"),
                false,
            ));
        }

        let changed = transaction
            .execute(
                "UPDATE attempts SET state=?1,result_digest=?2,exit_code=?3,infrastructure_error_digest=?4,finished_at_ms=?5,row_version=row_version+1 WHERE attempt_id=?6 AND row_version=?7 AND state NOT IN ('succeeded','failed','timed_out','cancelled','lost','orphaned')",
                params![
                    request.state.as_db(),
                    request.result_digest,
                    request.exit_code,
                    request.infrastructure_error_digest,
                    request.finished_at_ms,
                    request.attempt_id,
                    request.expected_row_version,
                ],
            )
            .map_err(|error| M6Error::from_sql(error, "cannot commit terminal Attempt"))?;
        if changed != 1 {
            return Err(state_conflict("Attempt changed during terminal commit"));
        }
        for artifact in &request.artifacts {
            let inserted = transaction.execute(
                "INSERT INTO artifacts(artifact_id,job_id,attempt_id,kind,relative_path,digest,media_type,byte_length,truncated,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    artifact.artifact_id,
                    attempt.job_id,
                    attempt.attempt_id,
                    artifact.kind,
                    artifact.relative_path,
                    artifact.digest,
                    artifact.media_type,
                    artifact.byte_length,
                    i64::from(artifact.truncated),
                    request.finished_at_ms,
                ],
            );
            if let Err(error) = inserted {
                return Err(M6Error::new(
                    M6ErrorCode::ArtifactIdentityConflict,
                    format!("cannot register Artifact {}: {error}", artifact.artifact_id),
                    Some("artifacts"),
                    false,
                ));
            }
        }
        upsert_condition(
            &transaction,
            &attempt.attempt_id,
            &ConditionUpdateM6 {
                condition_type: "result_available".to_string(),
                status: "true".to_string(),
                reason_code: request.reason_code.clone(),
                evidence_digest: request.result_digest.clone(),
                observed_at_ms: request.finished_at_ms,
            },
        )?;
        if request.state == AttemptState::Orphaned {
            hold_orphaned_reservation(
                &transaction,
                &attempt.attempt_id,
                request.finished_at_ms,
                &request.reason_code,
            )?;
        } else {
            release_reservation(
                &transaction,
                &attempt.attempt_id,
                request.finished_at_ms,
                &request.reason_code,
            )?;
        }

        let resolution = resolution_for_state(request.state)?;
        transaction
            .execute(
                "UPDATE jobs SET resolution=?1,current_attempt_id=NULL,row_version=row_version+1 WHERE job_id=?2 AND resolution IS NULL",
                params![resolution.as_db(), attempt.job_id],
            )
            .map_err(|error| M6Error::from_sql(error, "cannot resolve Job"))?;
        append_event(
            &transaction,
            &attempt.job_id,
            Some(&attempt.attempt_id),
            "PROCESS_EXITED",
            "SYSTEM_OBSERVED",
            Some(attempt.state),
            Some(request.state),
            &request.reason_code,
            serde_json::json!({
                "resultDigest": request.result_digest,
                "exitCode": request.exit_code,
            }),
            request.finished_at_ms,
        )?;
        append_event(
            &transaction,
            &attempt.job_id,
            Some(&attempt.attempt_id),
            "JOB_TERMINAL",
            "SYSTEM_DERIVED",
            Some(attempt.state),
            Some(request.state),
            &request.reason_code,
            serde_json::json!({"resolution": resolution.as_db()}),
            request.finished_at_ms,
        )?;
        transaction
            .commit()
            .map_err(|error| M6Error::from_sql(error, "cannot commit terminal transaction"))?;
        self.project_job(&attempt.job_id)
    }

    pub fn list_nonterminal_attempts(&self) -> M6Result<Vec<AttemptRecordM6>> {
        let connection = self.open_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT attempt_id,job_id,attempt_number,state,termination_intent,launch_token_digest,bundle_path,bundle_digest,boot_id,unit_name,invocation_id,control_group,main_pid,process_start_identity,runner_start_digest,result_digest,exit_code,infrastructure_error_digest,created_at_ms,started_at_ms,finished_at_ms,row_version FROM attempts WHERE state NOT IN ('succeeded','failed','timed_out','cancelled','lost','orphaned') ORDER BY created_at_ms,attempt_id",
            )
            .map_err(|error| M6Error::from_sql(error, "cannot prepare reconciliation scan"))?;
        let rows = statement
            .query_map([], raw_attempt_from_row)
            .map_err(|error| M6Error::from_sql(error, "cannot scan nonterminal Attempts"))?;
        rows.map(|row| {
            row.map_err(|error| M6Error::from_sql(error, "cannot decode Attempt row"))?
                .into_record()
        })
        .collect()
    }

    pub fn active_reservation_count(&self) -> M6Result<u32> {
        let connection = self.open_connection()?;
        connection
            .query_row(
                "SELECT COUNT(*) FROM concurrency_reservations WHERE state IN ('active','held_orphaned')",
                [],
                |row| row.get(0),
            )
            .map_err(|error| M6Error::from_sql(error, "cannot count active reservations"))
    }
}

fn immediate<'a>(connection: &'a mut Connection, context: &str) -> M6Result<Transaction<'a>> {
    connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| M6Error::from_sql(error, &format!("cannot begin {context}")))
}

struct RawJob {
    job_id: String,
    principal: String,
    client_request_id: String,
    request_digest: String,
    operation_digest: String,
    workspace_id: String,
    workspace_snapshot_json: String,
    plan_kind: String,
    execution_plan_json: String,
    execution_plan_digest: String,
    policy_id: String,
    policy_version: String,
    policy_digest: String,
    authority_ref: String,
    profile_id: Option<String>,
    created_at_ms: u64,
    desired_state: String,
    resolution: Option<String>,
    current_attempt_id: Option<String>,
    row_version: u64,
}

impl RawJob {
    fn into_record(self) -> M6Result<JobRecordM6> {
        Ok(JobRecordM6 {
            job_id: self.job_id,
            principal: self.principal,
            client_request_id: self.client_request_id,
            request_digest: self.request_digest,
            operation_digest: self.operation_digest,
            workspace_id: self.workspace_id,
            workspace_snapshot_json: self.workspace_snapshot_json,
            plan_kind: PlanKind::parse(&self.plan_kind)?,
            execution_plan_json: self.execution_plan_json,
            execution_plan_digest: self.execution_plan_digest,
            policy_id: self.policy_id,
            policy_version: self.policy_version,
            policy_digest: self.policy_digest,
            authority_ref: self.authority_ref,
            profile_id: self.profile_id,
            created_at_ms: self.created_at_ms,
            desired_state: JobDesiredState::parse(&self.desired_state)?,
            resolution: self
                .resolution
                .as_deref()
                .map(JobResolution::parse)
                .transpose()?,
            current_attempt_id: self.current_attempt_id,
            row_version: self.row_version,
        })
    }
}

fn raw_job_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawJob> {
    Ok(RawJob {
        job_id: row.get(0)?,
        principal: row.get(1)?,
        client_request_id: row.get(2)?,
        request_digest: row.get(3)?,
        operation_digest: row.get(4)?,
        workspace_id: row.get(5)?,
        workspace_snapshot_json: row.get(6)?,
        plan_kind: row.get(7)?,
        execution_plan_json: row.get(8)?,
        execution_plan_digest: row.get(9)?,
        policy_id: row.get(10)?,
        policy_version: row.get(11)?,
        policy_digest: row.get(12)?,
        authority_ref: row.get(13)?,
        profile_id: row.get(14)?,
        created_at_ms: row.get(15)?,
        desired_state: row.get(16)?,
        resolution: row.get(17)?,
        current_attempt_id: row.get(18)?,
        row_version: row.get(19)?,
    })
}

fn load_job(connection: &Connection, job_id: &str) -> M6Result<JobRecordM6> {
    connection
        .query_row(
            "SELECT job_id,principal,client_request_id,request_digest,operation_digest,workspace_id,workspace_snapshot_json,plan_kind,execution_plan_json,execution_plan_digest,policy_id,policy_version,policy_digest,authority_ref,profile_id,created_at_ms,desired_state,resolution,current_attempt_id,row_version FROM jobs WHERE job_id=?1",
            [job_id],
            raw_job_from_row,
        )
        .optional()
        .map_err(|error| M6Error::from_sql(error, "cannot load Job"))?
        .ok_or_else(|| {
            M6Error::new(M6ErrorCode::JobNotFound, "Job not found", Some("jobId"), false)
        })?
        .into_record()
}

struct RawAttempt {
    attempt_id: String,
    job_id: String,
    attempt_number: u32,
    state: String,
    termination_intent: String,
    launch_token_digest: String,
    bundle_path: String,
    bundle_digest: Option<String>,
    boot_id: Option<String>,
    unit_name: String,
    invocation_id: Option<String>,
    control_group: Option<String>,
    main_pid: Option<u32>,
    process_start_identity: Option<String>,
    runner_start_digest: Option<String>,
    result_digest: Option<String>,
    exit_code: Option<i32>,
    infrastructure_error_digest: Option<String>,
    created_at_ms: u64,
    started_at_ms: Option<u64>,
    finished_at_ms: Option<u64>,
    row_version: u64,
}

impl RawAttempt {
    fn into_record(self) -> M6Result<AttemptRecordM6> {
        Ok(AttemptRecordM6 {
            attempt_id: self.attempt_id,
            job_id: self.job_id,
            attempt_number: self.attempt_number,
            state: AttemptState::parse(&self.state)?,
            termination_intent: M6TerminationIntent::parse(&self.termination_intent)?,
            launch_token_digest: self.launch_token_digest,
            bundle_path: self.bundle_path,
            bundle_digest: self.bundle_digest,
            boot_id: self.boot_id,
            unit_name: self.unit_name,
            invocation_id: self.invocation_id,
            control_group: self.control_group,
            main_pid: self.main_pid,
            process_start_identity: self.process_start_identity,
            runner_start_digest: self.runner_start_digest,
            result_digest: self.result_digest,
            exit_code: self.exit_code,
            infrastructure_error_digest: self.infrastructure_error_digest,
            created_at_ms: self.created_at_ms,
            started_at_ms: self.started_at_ms,
            finished_at_ms: self.finished_at_ms,
            row_version: self.row_version,
        })
    }
}

fn raw_attempt_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawAttempt> {
    Ok(RawAttempt {
        attempt_id: row.get(0)?,
        job_id: row.get(1)?,
        attempt_number: row.get(2)?,
        state: row.get(3)?,
        termination_intent: row.get(4)?,
        launch_token_digest: row.get(5)?,
        bundle_path: row.get(6)?,
        bundle_digest: row.get(7)?,
        boot_id: row.get(8)?,
        unit_name: row.get(9)?,
        invocation_id: row.get(10)?,
        control_group: row.get(11)?,
        main_pid: row.get(12)?,
        process_start_identity: row.get(13)?,
        runner_start_digest: row.get(14)?,
        result_digest: row.get(15)?,
        exit_code: row.get(16)?,
        infrastructure_error_digest: row.get(17)?,
        created_at_ms: row.get(18)?,
        started_at_ms: row.get(19)?,
        finished_at_ms: row.get(20)?,
        row_version: row.get(21)?,
    })
}

fn load_attempt(connection: &Connection, attempt_id: &str) -> M6Result<AttemptRecordM6> {
    connection
        .query_row(
            "SELECT attempt_id,job_id,attempt_number,state,termination_intent,launch_token_digest,bundle_path,bundle_digest,boot_id,unit_name,invocation_id,control_group,main_pid,process_start_identity,runner_start_digest,result_digest,exit_code,infrastructure_error_digest,created_at_ms,started_at_ms,finished_at_ms,row_version FROM attempts WHERE attempt_id=?1",
            [attempt_id],
            raw_attempt_from_row,
        )
        .optional()
        .map_err(|error| M6Error::from_sql(error, "cannot load Attempt"))?
        .ok_or_else(|| {
            M6Error::new(
                M6ErrorCode::AttemptNotFound,
                "Attempt not found",
                Some("attemptId"),
                false,
            )
        })?
        .into_record()
}

struct RawReservation {
    reservation_id: String,
    attempt_id: String,
    profile_id: Option<String>,
    global_limit: u32,
    profile_limit: Option<u32>,
    state: String,
    acquired_at_ms: u64,
    released_at_ms: Option<u64>,
    release_reason: Option<String>,
}

fn load_reservation(connection: &Connection, attempt_id: &str) -> M6Result<ReservationRecordM6> {
    let raw = connection
        .query_row(
            "SELECT reservation_id,attempt_id,profile_id,global_limit,profile_limit,state,acquired_at_ms,released_at_ms,release_reason FROM concurrency_reservations WHERE attempt_id=?1",
            [attempt_id],
            |row| {
                Ok(RawReservation {
                    reservation_id: row.get(0)?,
                    attempt_id: row.get(1)?,
                    profile_id: row.get(2)?,
                    global_limit: row.get(3)?,
                    profile_limit: row.get(4)?,
                    state: row.get(5)?,
                    acquired_at_ms: row.get(6)?,
                    released_at_ms: row.get(7)?,
                    release_reason: row.get(8)?,
                })
            },
        )
        .optional()
        .map_err(|error| M6Error::from_sql(error, "cannot load reservation"))?
        .ok_or_else(|| {
            M6Error::new(
                M6ErrorCode::ReservationStateConflict,
                "Attempt has no reservation",
                Some("attemptId"),
                false,
            )
        })?;
    Ok(ReservationRecordM6 {
        reservation_id: raw.reservation_id,
        attempt_id: raw.attempt_id,
        profile_id: raw.profile_id,
        global_limit: raw.global_limit,
        profile_limit: raw.profile_limit,
        state: ReservationState::parse(&raw.state)?,
        acquired_at_ms: raw.acquired_at_ms,
        released_at_ms: raw.released_at_ms,
        release_reason: raw.release_reason,
    })
}

#[allow(clippy::too_many_arguments)]
fn append_event(
    transaction: &Transaction<'_>,
    job_id: &str,
    attempt_id: Option<&str>,
    event_type: &str,
    origin: &str,
    previous_state: Option<AttemptState>,
    new_state: Option<AttemptState>,
    reason_code: &str,
    detail: serde_json::Value,
    observed_at_ms: u64,
) -> M6Result<()> {
    let sequence: u64 = transaction
        .query_row(
            "SELECT COALESCE(MAX(event_sequence),0)+1 FROM job_events WHERE job_id=?1",
            [job_id],
            |row| row.get(0),
        )
        .map_err(|error| M6Error::from_sql(error, "cannot allocate event sequence"))?;
    let detail_json = serde_json::to_string(&detail).map_err(|error| {
        M6Error::new(
            M6ErrorCode::RegistryUnavailable,
            format!("cannot serialize event detail: {error}"),
            None,
            false,
        )
    })?;
    let detail_digest = sha256_bytes(detail_json.as_bytes());
    let event_id = format!("event-{}", Uuid::now_v7());
    transaction
        .execute(
            "INSERT INTO job_events(event_id,job_id,attempt_id,event_sequence,event_type,origin,previous_state,new_state,reason_code,detail_json,detail_digest,observed_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![
                event_id,
                job_id,
                attempt_id,
                sequence,
                event_type,
                origin,
                previous_state.map(AttemptState::as_db),
                new_state.map(AttemptState::as_db),
                reason_code,
                detail_json,
                detail_digest,
                observed_at_ms,
            ],
        )
        .map_err(|error| M6Error::from_sql(error, "cannot append Job event"))?;
    Ok(())
}

fn upsert_condition(
    transaction: &Transaction<'_>,
    attempt_id: &str,
    condition: &ConditionUpdateM6,
) -> M6Result<()> {
    transaction
        .execute(
            "INSERT INTO attempt_conditions(attempt_id,condition_type,status,reason_code,evidence_digest,observed_at_ms) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(attempt_id,condition_type) DO UPDATE SET status=excluded.status,reason_code=excluded.reason_code,evidence_digest=excluded.evidence_digest,observed_at_ms=excluded.observed_at_ms",
            params![
                attempt_id,
                condition.condition_type,
                condition.status,
                condition.reason_code,
                condition.evidence_digest,
                condition.observed_at_ms,
            ],
        )
        .map_err(|error| M6Error::from_sql(error, "cannot update Attempt condition"))?;
    Ok(())
}

fn release_reservation(
    transaction: &Transaction<'_>,
    attempt_id: &str,
    released_at_ms: u64,
    reason: &str,
) -> M6Result<()> {
    let changed = transaction
        .execute(
            "UPDATE concurrency_reservations SET state='released',released_at_ms=?1,release_reason=?2 WHERE attempt_id=?3 AND state IN ('active','held_orphaned')",
            params![released_at_ms, reason, attempt_id],
        )
        .map_err(|error| M6Error::from_sql(error, "cannot release reservation"))?;
    if changed == 0 {
        let current: String = transaction
            .query_row(
                "SELECT state FROM concurrency_reservations WHERE attempt_id=?1",
                [attempt_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| M6Error::from_sql(error, "cannot inspect reservation state"))?
            .ok_or_else(|| {
                M6Error::new(
                    M6ErrorCode::ReservationStateConflict,
                    "reservation is missing",
                    Some("attemptId"),
                    false,
                )
            })?;
        if current != "released" {
            return Err(M6Error::new(
                M6ErrorCode::ReservationStateConflict,
                format!("reservation cannot be released from {current}"),
                Some("attemptId"),
                false,
            ));
        }
    }
    Ok(())
}

fn hold_orphaned_reservation(
    transaction: &Transaction<'_>,
    attempt_id: &str,
    observed_at_ms: u64,
    reason: &str,
) -> M6Result<()> {
    let changed = transaction
        .execute(
            "UPDATE concurrency_reservations SET state='held_orphaned',released_at_ms=NULL,release_reason=?1 WHERE attempt_id=?2 AND state IN ('active','held_orphaned')",
            params![reason, attempt_id],
        )
        .map_err(|error| M6Error::from_sql(error, "cannot hold orphaned reservation"))?;
    if changed != 1 {
        return Err(M6Error::new(
            M6ErrorCode::ReservationStateConflict,
            "orphaned Attempt has no active reservation",
            Some("attemptId"),
            false,
        ));
    }
    upsert_condition(
        transaction,
        attempt_id,
        &ConditionUpdateM6 {
            condition_type: "reservation_held".to_string(),
            status: "held_orphaned".to_string(),
            reason_code: reason.to_string(),
            evidence_digest: sha256_bytes(
                format!("m6-orphaned-reservation\0{attempt_id}\0{observed_at_ms}").as_bytes(),
            ),
            observed_at_ms,
        },
    )
}

fn resolution_for_state(state: AttemptState) -> M6Result<JobResolution> {
    match state {
        AttemptState::Succeeded => Ok(JobResolution::Succeeded),
        AttemptState::Failed => Ok(JobResolution::Failed),
        AttemptState::TimedOut => Ok(JobResolution::TimedOut),
        AttemptState::Cancelled => Ok(JobResolution::Cancelled),
        AttemptState::Lost => Ok(JobResolution::Lost),
        AttemptState::Orphaned => Ok(JobResolution::Orphaned),
        _ => Err(M6Error::invalid("Attempt state is not terminal", "state")),
    }
}

fn project_job(job: &JobRecordM6, attempt: Option<&AttemptRecordM6>) -> JobProjectionM6 {
    let status = if let Some(resolution) = job.resolution {
        resolution.as_db().to_string()
    } else if let Some(attempt) = attempt {
        match attempt.state {
            AttemptState::Accepted | AttemptState::Starting => "queued".to_string(),
            AttemptState::Running | AttemptState::Stopping | AttemptState::Recovering => {
                "working".to_string()
            }
            terminal => terminal.as_db().to_string(),
        }
    } else {
        "unknown".to_string()
    };
    JobProjectionM6 {
        job_id: job.job_id.clone(),
        status,
        attempt_id: attempt.map(|attempt| attempt.attempt_id.clone()),
        exit_code: attempt.and_then(|attempt| attempt.exit_code),
        result_available: job.resolution.is_some(),
        artifacts_available: attempt.is_some_and(|attempt| attempt.result_digest.is_some()),
        poll_after_ms: job.resolution.is_none().then_some(250),
    }
}

fn validate_submit(request: &M6SubmitRequest) -> M6Result<()> {
    if request.schema_version != M6_SCHEMA_VERSION
        || request.plan.schema_version != M6_SCHEMA_VERSION
    {
        return Err(M6Error::invalid(
            "unsupported M6 schema version",
            "schemaVersion",
        ));
    }
    validate_identifier(&request.client_request_id, "clientRequestId")?;
    validate_identifier(&request.plan.principal, "plan.principal")?;
    validate_identifier(&request.plan.authority_ref, "plan.authorityRef")?;
    validate_identifier(&request.plan.workspace_id, "plan.workspaceId")?;
    validate_identifier(&request.plan.policy_id, "plan.policyId")?;
    validate_identifier(&request.plan.policy_version, "plan.policyVersion")?;
    if let Some(profile_id) = &request.plan.profile_id {
        validate_identifier(profile_id, "plan.profileId")?;
    }
    if request.global_limit == 0 {
        return Err(M6Error::invalid(
            "globalLimit must be positive",
            "globalLimit",
        ));
    }
    if request.profile_limit == Some(0) {
        return Err(M6Error::invalid(
            "profileLimit must be positive",
            "profileLimit",
        ));
    }
    if request.plan.profile_id.is_some() != request.profile_limit.is_some() {
        return Err(M6Error::invalid(
            "profileId and profileLimit must appear together",
            "profileLimit",
        ));
    }

    for (path, field) in [
        (&request.plan.workspace_path, "plan.workspacePath"),
        (&request.plan.executable, "plan.executable"),
        (&request.plan.cwd, "plan.cwd"),
    ] {
        if !Path::new(path).is_absolute() || path.as_bytes().contains(&0) {
            return Err(M6Error::invalid(
                format!("{field} must be an absolute NUL-free path"),
                field,
            ));
        }
    }
    if !Path::new(&request.plan.cwd).starts_with(&request.plan.workspace_path) {
        return Err(M6Error::invalid(
            "plan.cwd must remain inside workspacePath",
            "plan.cwd",
        ));
    }
    validate_digest(&request.plan.executable_digest, "plan.executableDigest")?;
    validate_digest(&request.plan.policy_digest, "plan.policyDigest")?;
    if request.plan.source_revision.is_empty() || request.plan.source_revision.len() > 256 {
        return Err(M6Error::invalid(
            "sourceRevision must be non-empty and bounded",
            "plan.sourceRevision",
        ));
    }
    if request.plan.timeout_ms == 0
        || request.plan.stdout_limit_bytes == 0
        || request.plan.stderr_limit_bytes == 0
    {
        return Err(M6Error::invalid(
            "runtime and output limits must be positive",
            "plan",
        ));
    }
    if request.plan.args.len() > 128 || request.plan.env.len() > 64 {
        return Err(M6Error::invalid(
            "execution args or environment exceed M6 bounds",
            "plan",
        ));
    }
    if request
        .plan
        .args
        .iter()
        .chain(request.plan.env.keys())
        .chain(request.plan.env.values())
        .any(|value| value.as_bytes().contains(&0) || value.len() > 16 * 1024)
    {
        return Err(M6Error::invalid(
            "execution args or environment contain invalid values",
            "plan",
        ));
    }
    Ok(())
}

fn validate_runner_identity(identity: &RunnerIdentityM6) -> M6Result<()> {
    for (value, field) in [
        (&identity.boot_id, "bootId"),
        (&identity.unit_name, "unitName"),
        (&identity.invocation_id, "invocationId"),
        (&identity.process_start_identity, "processStartIdentity"),
    ] {
        validate_identifier(value, field)?;
    }
    if !identity.unit_name.ends_with(".service") || identity.main_pid == 0 {
        return Err(M6Error::invalid(
            "invalid Runner unit or PID",
            "runnerIdentity",
        ));
    }
    if !Path::new(&identity.control_group).is_absolute() {
        return Err(M6Error::invalid(
            "controlGroup must be absolute",
            "controlGroup",
        ));
    }
    validate_digest(&identity.runner_start_digest, "runnerStartDigest")
}

fn validate_artifact_registration(artifact: &ArtifactRegistrationM6) -> M6Result<()> {
    validate_identifier(&artifact.artifact_id, "artifactId")?;
    validate_identifier(&artifact.kind, "artifact.kind")?;
    validate_digest(&artifact.digest, "artifact.digest")?;
    if artifact.relative_path.is_empty()
        || Path::new(&artifact.relative_path).is_absolute()
        || artifact
            .relative_path
            .split('/')
            .any(|segment| segment == "..")
    {
        return Err(M6Error::invalid(
            "Artifact path must be a bounded relative path",
            "artifact.relativePath",
        ));
    }
    if artifact.media_type.is_empty() || artifact.media_type.len() > 256 {
        return Err(M6Error::invalid(
            "Artifact mediaType must be non-empty and bounded",
            "artifact.mediaType",
        ));
    }
    Ok(())
}

fn validate_identifier(value: &str, field: &str) -> M6Result<()> {
    if value.trim().is_empty()
        || value.len() > 256
        || value.as_bytes().contains(&0)
        || value.chars().any(char::is_control)
    {
        return Err(M6Error::invalid(
            format!("{field} must be non-empty, bounded, and control-free"),
            field,
        ));
    }
    Ok(())
}

fn validate_digest(value: &str, field: &str) -> M6Result<()> {
    let valid = value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()));
    if !valid {
        return Err(M6Error::invalid(
            format!("{field} must be a SHA-256 digest"),
            field,
        ));
    }
    Ok(())
}

fn state_conflict(message: impl Into<String>) -> M6Error {
    M6Error::new(
        M6ErrorCode::AttemptStateConflict,
        message,
        Some("attemptId"),
        false,
    )
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn now_ms() -> M6Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            M6Error::new(
                M6ErrorCode::RegistryUnavailable,
                format!("system clock precedes Unix epoch: {error}"),
                None,
                false,
            )
        })?
        .as_millis()
        .try_into()
        .map_err(|_| {
            M6Error::new(
                M6ErrorCode::RegistryUnavailable,
                "current time does not fit u64 milliseconds",
                None,
                false,
            )
        })
}

fn create_private_directory(path: &Path) -> M6Result<()> {
    fs::create_dir_all(path).map_err(|error| {
        M6Error::new(
            M6ErrorCode::IoError,
            format!("cannot create {}: {error}", path.display()),
            Some("storeRoot"),
            false,
        )
    })?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
        M6Error::new(
            M6ErrorCode::IoError,
            format!("cannot protect {}: {error}", path.display()),
            Some("storeRoot"),
            false,
        )
    })
}

fn set_private_file(path: &Path) -> M6Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|error| {
        M6Error::new(
            M6ErrorCode::IoError,
            format!("cannot protect {}: {error}", path.display()),
            Some("dbPath"),
            false,
        )
    })
}
