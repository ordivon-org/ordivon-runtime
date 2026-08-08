use rusqlite::{
    params, Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use super::repair::{AdminRepairAudit, AdminRepairOperation};
use super::{
    operation_request_identity_digest_from_plan, AdmissionOutcome, ArtifactRegistration,
    AttemptRecord, AttemptState, AttemptTerminationIntent, ConditionUpdate, CreatedAdmission,
    JobDesiredState, JobProjection, JobResolution, ReservationRecord, ReservationState,
    RunnerIdentity, RuntimeArtifactRecord, RuntimeDeliveryDisposition, RuntimeError,
    RuntimeErrorCode, RuntimeExecutionPlan, RuntimeInvariantViolation, RuntimeJobListCursor,
    RuntimeJobListRequest, RuntimeJobListResult, RuntimeJobRecord, RuntimeJobSummary,
    RuntimeResult, SubmitRequest, TerminalCommit, MAX_RUNTIME_LIST_LIMIT, RUNTIME_SCHEMA_VERSION,
};

const MIGRATION_V1: i64 = 1;
const MIGRATION_V1_NAME: &str = "0001_runtime";
const MIGRATION_V1_SQL: &str = include_str!("../../migrations/runtime/0001_runtime.sql");
pub const RUNTIME_MIGRATION_CHECKSUM: &str =
    "sha256:9c5e0ccf94b0c3efa9b671a9300cfe00e4539d0c880e6a8df8982df9fa8826ac";
const MIGRATION_V2: i64 = 2;
const MIGRATION_V2_NAME: &str = "0002_orphan_recovery";
const MIGRATION_V2_SQL: &str = include_str!("../../migrations/runtime/0002_orphan_recovery.sql");
pub const RUNTIME_ORPHAN_RECOVERY_MIGRATION_CHECKSUM: &str =
    "sha256:08361881c9f589254e5e9fad089fcbf756bd8613352e995437fb7a616e9ce500";
const MIGRATION_V3: i64 = 3;
const MIGRATION_V3_NAME: &str = "0003_terminal_repair";
const MIGRATION_V3_SQL: &str = include_str!("../../migrations/runtime/0003_terminal_repair.sql");
pub const RUNTIME_TERMINAL_REPAIR_MIGRATION_CHECKSUM: &str =
    "sha256:464c9b769dacd10f7302d7a371f5b36a7553eda0b0b112bae35b901d00a67f0d";
const MIGRATION_V4: i64 = 4;
const MIGRATION_V4_NAME: &str = "0004_orphan_reclaim";
const MIGRATION_V4_SQL: &str = include_str!("../../migrations/runtime/0004_orphan_reclaim.sql");
pub const RUNTIME_ORPHAN_RECLAIM_MIGRATION_CHECKSUM: &str =
    "sha256:b76afbfaf70645b60456b08ad257e5ac2be1f63499f24a555cbf0157791e19ad";
pub(crate) const MAX_MIGRATION_VERSION: i64 = 4;
const WORKSPACE_PATCH_STORAGE_SQL: &str = include_str!("workspace_patch_storage.sql");
const WORKSPACE_PATCH_TABLE: &str = "workspace_patch_operations";
const WORKSPACE_PATCH_INDEX: &str = "idx_workspace_patch_operations_workspace";
const JOB_CLIENT_REQUEST_LOOKUP_INDEX: &str = "idx_jobs_client_request_id_created";
const JOB_CLIENT_REQUEST_LOOKUP_INDEX_SQL: &str =
    "CREATE INDEX IF NOT EXISTS idx_jobs_client_request_id_created ON jobs(client_request_id, created_at_ms, job_id)";
const JOB_WORKSPACE_LOOKUP_INDEX: &str = "idx_jobs_workspace_created";
const JOB_WORKSPACE_LOOKUP_INDEX_SQL: &str =
    "CREATE INDEX IF NOT EXISTS idx_jobs_workspace_created ON jobs(workspace_id, created_at_ms, job_id)";
const WORKSPACE_EXECUTION_LIMIT: u32 = 1;

#[derive(Clone, Debug)]
pub struct RegistryConfig {
    pub db_path: PathBuf,
    pub store_root: PathBuf,
    pub busy_timeout_ms: u64,
}

#[derive(Clone, Debug)]
pub struct Registry {
    config: RegistryConfig,
}

#[derive(Clone, Debug)]
pub(crate) struct JobSnapshot {
    pub job: RuntimeJobRecord,
    pub attempt: Option<AttemptRecord>,
    pub projection: JobProjection,
}

impl RegistryConfig {
    pub fn validate(&self) -> RuntimeResult<()> {
        if !self.db_path.is_absolute() {
            return Err(RuntimeError::invalid(
                "database path must be absolute",
                "dbPath",
            ));
        }
        if !self.store_root.is_absolute() {
            return Err(RuntimeError::invalid(
                "store root must be absolute",
                "storeRoot",
            ));
        }
        if self.busy_timeout_ms == 0 {
            return Err(RuntimeError::invalid(
                "busy timeout must be positive",
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

fn capacity_holders(
    transaction: &Transaction<'_>,
    workspace_id: Option<&str>,
    limit: u32,
) -> RuntimeResult<(Vec<String>, Vec<String>)> {
    let mut statement = transaction
        .prepare(
            "SELECT DISTINCT j.job_id,j.workspace_id FROM concurrency_reservations r JOIN attempts a ON a.attempt_id=r.attempt_id JOIN jobs j ON j.job_id=a.job_id WHERE r.state IN ('active','held_orphaned') AND (?1 IS NULL OR j.workspace_id=?1) ORDER BY r.acquired_at_ms ASC,j.job_id ASC LIMIT ?2",
        )
        .map_err(|error| RuntimeError::from_sql(error, "cannot prepare capacity-holder query"))?;
    let rows = statement
        .query_map(params![workspace_id, limit], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| RuntimeError::from_sql(error, "cannot query capacity holders"))?;
    let mut job_ids = Vec::new();
    let mut workspace_ids = Vec::new();
    for row in rows {
        let (job_id, holder_workspace_id) =
            row.map_err(|error| RuntimeError::from_sql(error, "cannot decode capacity holder"))?;
        job_ids.push(job_id);
        if !workspace_ids.contains(&holder_workspace_id) {
            workspace_ids.push(holder_workspace_id);
        }
    }
    Ok((job_ids, workspace_ids))
}

impl Registry {
    pub fn initialize(config: RegistryConfig) -> RuntimeResult<Self> {
        config.validate()?;
        create_private_directory(&config.store_root)?;
        create_private_directory(&config.attempts_root())?;
        if let Some(parent) = config.db_path.parent() {
            create_private_directory(parent)?;
        }
        let registry = Self { config };
        let mut connection = registry.open_connection()?;
        registry.ensure_wal_mode(&connection)?;
        registry.apply_migrations(&mut connection)?;
        registry.ensure_query_indexes(&mut connection)?;
        registry.ensure_workspace_patch_storage(&mut connection)?;
        registry.validate_database(&connection)?;
        set_private_file(&registry.config.db_path)?;
        Ok(registry)
    }

    pub fn config(&self) -> &RegistryConfig {
        &self.config
    }

    pub(crate) fn open_connection(&self) -> RuntimeResult<Connection> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection = Connection::open_with_flags(&self.config.db_path, flags)
            .map_err(|error| RuntimeError::from_sql(error, "cannot open runtime registry"))?;
        connection
            .busy_timeout(Duration::from_millis(self.config.busy_timeout_ms))
            .map_err(|error| RuntimeError::from_sql(error, "cannot set registry busy timeout"))?;
        connection
            .pragma_update(None, "foreign_keys", true)
            .map_err(|error| RuntimeError::from_sql(error, "cannot enable foreign keys"))?;
        connection
            .pragma_update(None, "trusted_schema", false)
            .map_err(|error| RuntimeError::from_sql(error, "cannot disable trusted schema"))?;
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(|error| RuntimeError::from_sql(error, "cannot set synchronous mode"))?;
        Ok(connection)
    }

    fn ensure_wal_mode(&self, connection: &Connection) -> RuntimeResult<()> {
        let mode: String = connection
            .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
            .map_err(|error| RuntimeError::from_sql(error, "cannot enable WAL mode"))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryUnavailable,
                format!("SQLite refused WAL mode and returned {mode}"),
                None,
                false,
            ));
        }
        Ok(())
    }

    fn apply_migrations(&self, connection: &mut Connection) -> RuntimeResult<()> {
        let has_table: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations')",
                [],
                |row| row.get(0),
            )
            .map_err(|error| RuntimeError::from_sql(error, "cannot inspect schema migrations"))?;
        if !has_table {
            let transaction = immediate(connection, "initial migration")?;
            transaction
                .execute_batch(MIGRATION_V1_SQL)
                .map_err(|error| RuntimeError::from_sql(error, "cannot apply initial migration"))?;
            transaction
                .execute(
                    "INSERT INTO schema_migrations(version,name,checksum,applied_at_ms) VALUES(?1,?2,?3,?4)",
                    params![MIGRATION_V1, MIGRATION_V1_NAME, RUNTIME_MIGRATION_CHECKSUM, now_ms()?],
                )
                .map_err(|error| RuntimeError::from_sql(error, "cannot record initial migration"))?;
            transaction.commit().map_err(|error| {
                RuntimeError::from_sql(error, "cannot commit initial migration")
            })?;
        }

        let max_version: Option<i64> = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .map_err(|error| RuntimeError::from_sql(error, "cannot read migration version"))?;
        let Some(max_version) = max_version else {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "schema_migrations is empty",
                None,
                false,
            ));
        };
        if max_version > MAX_MIGRATION_VERSION {
            return Err(RuntimeError::new(
                RuntimeErrorCode::SchemaVersionUnsupported,
                format!(
                    "registry schema {max_version} is newer than supported {MAX_MIGRATION_VERSION}"
                ),
                None,
                false,
            ));
        }
        validate_migration_checksum(
            connection,
            MIGRATION_V1,
            RUNTIME_MIGRATION_CHECKSUM,
            "initial migration",
        )?;
        if max_version < MIGRATION_V2 {
            let transaction = immediate(connection, "orphan-recovery migration")?;
            transaction
                .execute_batch(MIGRATION_V2_SQL)
                .map_err(|error| {
                    RuntimeError::from_sql(error, "cannot apply orphan-recovery migration")
                })?;
            transaction
                .execute(
                    "INSERT INTO schema_migrations(version,name,checksum,applied_at_ms) VALUES(?1,?2,?3,?4)",
                    params![
                        MIGRATION_V2,
                        MIGRATION_V2_NAME,
                        RUNTIME_ORPHAN_RECOVERY_MIGRATION_CHECKSUM,
                        now_ms()?
                    ],
                )
                .map_err(|error| RuntimeError::from_sql(error, "cannot record orphan-recovery migration"))?;
            transaction.commit().map_err(|error| {
                RuntimeError::from_sql(error, "cannot commit orphan-recovery migration")
            })?;
        }
        validate_migration_checksum(
            connection,
            MIGRATION_V2,
            RUNTIME_ORPHAN_RECOVERY_MIGRATION_CHECKSUM,
            "orphan-recovery migration",
        )?;
        if max_version < MIGRATION_V3 {
            let transaction = immediate(connection, "terminal-repair migration")?;
            transaction
                .execute_batch(MIGRATION_V3_SQL)
                .map_err(|error| {
                    RuntimeError::from_sql(error, "cannot apply terminal-repair migration")
                })?;
            transaction
                .execute(
                    "INSERT INTO schema_migrations(version,name,checksum,applied_at_ms) VALUES(?1,?2,?3,?4)",
                    params![
                        MIGRATION_V3,
                        MIGRATION_V3_NAME,
                        RUNTIME_TERMINAL_REPAIR_MIGRATION_CHECKSUM,
                        now_ms()?
                    ],
                )
                .map_err(|error| {
                    RuntimeError::from_sql(error, "cannot record terminal-repair migration")
                })?;
            transaction.commit().map_err(|error| {
                RuntimeError::from_sql(error, "cannot commit terminal-repair migration")
            })?;
        }
        validate_migration_checksum(
            connection,
            MIGRATION_V3,
            RUNTIME_TERMINAL_REPAIR_MIGRATION_CHECKSUM,
            "terminal-repair migration",
        )?;
        if max_version < MIGRATION_V4 {
            let transaction = immediate(connection, "orphan-reclaim migration")?;
            transaction
                .execute_batch(MIGRATION_V4_SQL)
                .map_err(|error| {
                    RuntimeError::from_sql(error, "cannot apply orphan-reclaim migration")
                })?;
            transaction
                .execute(
                    "INSERT INTO schema_migrations(version,name,checksum,applied_at_ms) VALUES(?1,?2,?3,?4)",
                    params![
                        MIGRATION_V4,
                        MIGRATION_V4_NAME,
                        RUNTIME_ORPHAN_RECLAIM_MIGRATION_CHECKSUM,
                        now_ms()?
                    ],
                )
                .map_err(|error| {
                    RuntimeError::from_sql(error, "cannot record orphan-reclaim migration")
                })?;
            transaction.commit().map_err(|error| {
                RuntimeError::from_sql(error, "cannot commit orphan-reclaim migration")
            })?;
        }
        validate_migration_checksum(
            connection,
            MIGRATION_V4,
            RUNTIME_ORPHAN_RECLAIM_MIGRATION_CHECKSUM,
            "orphan-reclaim migration",
        )?;
        Ok(())
    }

    fn ensure_query_indexes(&self, connection: &mut Connection) -> RuntimeResult<()> {
        let transaction = immediate(connection, "Runtime query index maintenance")?;
        for (sql, error_context) in [
            (
                JOB_CLIENT_REQUEST_LOOKUP_INDEX_SQL,
                "cannot ensure Job client request lookup index",
            ),
            (
                JOB_WORKSPACE_LOOKUP_INDEX_SQL,
                "cannot ensure Job Workspace lookup index",
            ),
        ] {
            transaction
                .execute(sql, [])
                .map_err(|error| RuntimeError::from_sql(error, error_context))?;
        }
        transaction.commit().map_err(|error| {
            RuntimeError::from_sql(error, "cannot commit Runtime query index maintenance")
        })?;
        for (index, verify_context, missing_message) in [
            (
                JOB_CLIENT_REQUEST_LOOKUP_INDEX,
                "cannot verify Job client request lookup index",
                "Job client request lookup index is missing after maintenance",
            ),
            (
                JOB_WORKSPACE_LOOKUP_INDEX,
                "cannot verify Job Workspace lookup index",
                "Job Workspace lookup index is missing after maintenance",
            ),
        ] {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name=?1)",
                    [index],
                    |row| row.get(0),
                )
                .map_err(|error| RuntimeError::from_sql(error, verify_context))?;
            if !exists {
                return Err(RuntimeError::new(
                    RuntimeErrorCode::RegistryCorrupt,
                    missing_message,
                    None,
                    false,
                ));
            }
        }
        Ok(())
    }

    fn ensure_workspace_patch_storage(&self, connection: &mut Connection) -> RuntimeResult<()> {
        let transaction = immediate(connection, "Workspace Patch storage maintenance")?;
        transaction
            .execute_batch(WORKSPACE_PATCH_STORAGE_SQL)
            .map_err(|error| {
                RuntimeError::from_sql(error, "cannot ensure Workspace Patch storage")
            })?;
        transaction.commit().map_err(|error| {
            RuntimeError::from_sql(error, "cannot commit Workspace Patch storage maintenance")
        })?;
        for (kind, name) in [
            ("table", WORKSPACE_PATCH_TABLE),
            ("index", WORKSPACE_PATCH_INDEX),
        ] {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type=?1 AND name=?2)",
                    params![kind, name],
                    |row| row.get(0),
                )
                .map_err(|error| {
                    RuntimeError::from_sql(error, "cannot verify Workspace Patch storage")
                })?;
            if !exists {
                return Err(RuntimeError::new(
                    RuntimeErrorCode::RegistryCorrupt,
                    format!("Workspace Patch {kind} {name} is missing after maintenance"),
                    None,
                    false,
                ));
            }
        }
        Ok(())
    }

    fn validate_database(&self, connection: &Connection) -> RuntimeResult<()> {
        let quick: String = connection
            .query_row("PRAGMA quick_check(20)", [], |row| row.get(0))
            .map_err(|error| RuntimeError::from_sql(error, "registry quick_check failed"))?;
        if quick != "ok" {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                format!("registry quick_check returned {quick}"),
                None,
                false,
            ));
        }
        let foreign_key_problem: Option<String> = connection
            .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
            .optional()
            .map_err(|error| RuntimeError::from_sql(error, "foreign key check failed"))?;
        if let Some(table) = foreign_key_problem {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                format!("foreign key violation in {table}"),
                None,
                false,
            ));
        }
        Ok(())
    }

    pub fn find_idempotent_job(
        &self,
        principal: &str,
        client_request_id: &str,
        request_identity_digest: &str,
    ) -> RuntimeResult<Option<RuntimeJobRecord>> {
        validate_identifier(principal, "principal")?;
        validate_identifier(client_request_id, "clientRequestId")?;
        validate_request_identity_digest(request_identity_digest)?;
        let connection = self.open_connection()?;
        let job_id: Option<String> = connection
            .query_row(
                "SELECT job_id FROM idempotency_keys WHERE principal=?1 AND client_request_id=?2",
                params![principal, client_request_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| RuntimeError::from_sql(error, "cannot find idempotent Job"))?;
        let Some(job_id) = job_id else {
            return Ok(None);
        };
        let job = load_job(&connection, &job_id)?;
        if job_request_identity_digest(&job)? != request_identity_digest {
            return Err(idempotency_conflict());
        }
        Ok(Some(job))
    }

    pub fn submit(&self, request: &SubmitRequest) -> RuntimeResult<AdmissionOutcome> {
        validate_submit(request)?;
        let created_at_ms = now_ms()?;
        let plan_json = serde_json::to_string(&request.plan).map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::InvalidRequest,
                format!("cannot serialize execution plan: {error}"),
                Some("plan"),
                false,
            )
        })?;
        let request_json = serde_json::to_vec(request).map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::InvalidRequest,
                format!("cannot serialize submit request: {error}"),
                None,
                false,
            )
        })?;
        let legacy_request_digest = sha256_bytes(&request_json);
        let request_digest = request
            .request_identity_digest
            .clone()
            .unwrap_or(legacy_request_digest);
        let plan_digest = sha256_bytes(plan_json.as_bytes());
        let workspace_snapshot_json = serde_json::json!({
            "workspaceId": request.plan.workspace_id,
            "workspacePath": request.plan.workspace_path,
            "sourceRevision": request.plan.source_revision,
            "workspaceSourceDigest": request.plan.workspace_source_digest,
        })
        .to_string();
        let operation_digest = sha256_bytes(
            format!("runtime-operation-v3\0{request_digest}\0{plan_digest}").as_bytes(),
        );
        let job_id = format!("job-{}", Uuid::now_v7());
        let attempt_id = format!("attempt-{}", Uuid::now_v7());
        let reservation_id = format!("reservation-{}", Uuid::now_v7());
        let launch_token =
            sha256_bytes(format!("runtime-launch-v1\0{attempt_id}\0{operation_digest}").as_bytes());
        let launch_token_digest = sha256_bytes(launch_token.as_bytes());
        let unit_name = format!("ordivon-{attempt_id}.service");
        let bundle_path = self
            .config
            .attempt_path(&attempt_id)
            .to_string_lossy()
            .into_owned();

        let job = RuntimeJobRecord {
            job_id: job_id.clone(),
            principal: request.plan.principal.clone(),
            client_request_id: request.client_request_id.clone(),
            request_digest: request_digest.clone(),
            operation_digest: operation_digest.clone(),
            workspace_id: request.plan.workspace_id.clone(),
            workspace_snapshot_json: workspace_snapshot_json.clone(),
            execution_plan_json: plan_json.clone(),
            execution_plan_digest: plan_digest.clone(),
            created_at_ms,
            desired_state: JobDesiredState::Run,
            resolution: None,
            current_attempt_id: Some(attempt_id.clone()),
            row_version: 0,
        };
        let attempt = AttemptRecord {
            attempt_id: attempt_id.clone(),
            job_id: job_id.clone(),
            attempt_number: 1,
            state: AttemptState::Accepted,
            termination_intent: AttemptTerminationIntent::Natural,
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
        let reservation = ReservationRecord {
            reservation_id: reservation_id.clone(),
            attempt_id: attempt_id.clone(),
            global_limit: request.global_limit,
            state: ReservationState::Active,
            acquired_at_ms: created_at_ms,
            released_at_ms: None,
            release_reason: None,
        };

        let mut connection = self.open_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| RuntimeError::from_sql(error, "cannot begin admission transaction"))?;

        if let Some((existing_operation_digest, existing_job_id)) = transaction
            .query_row(
                "SELECT operation_digest, job_id FROM idempotency_keys WHERE principal=?1 AND client_request_id=?2",
                params![request.plan.principal, request.client_request_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|error| RuntimeError::from_sql(error, "cannot check idempotency key"))?
        {
            let existing = load_job(&transaction, &existing_job_id)?;
            let matches = if let Some(request_identity_digest) =
                request.request_identity_digest.as_deref()
            {
                job_request_identity_digest(&existing)? == request_identity_digest
            } else {
                existing_operation_digest == operation_digest
            };
            if !matches {
                return Err(idempotency_conflict());
            }
            transaction
                .commit()
                .map_err(|error| RuntimeError::from_sql(error, "cannot close replay transaction"))?;
            return Ok(AdmissionOutcome::Existing {
                job: Box::new(existing),
            });
        }

        let workspace_active: u32 = transaction
            .query_row(
                "SELECT COUNT(*) FROM concurrency_reservations r JOIN attempts a ON a.attempt_id=r.attempt_id JOIN jobs j ON j.job_id=a.job_id WHERE r.state IN ('active','held_orphaned') AND j.workspace_id=?1",
                [&request.plan.workspace_id],
                |row| row.get(0),
            )
            .map_err(|error| RuntimeError::from_sql(error, "cannot count workspace reservations"))?;
        if workspace_active >= WORKSPACE_EXECUTION_LIMIT {
            let (holder_job_ids, holder_workspace_ids) = capacity_holders(
                &transaction,
                Some(&request.plan.workspace_id),
                WORKSPACE_EXECUTION_LIMIT,
            )?;
            return Err(RuntimeError::concurrency(
                format!(
                    "workspace execution concurrency limit reached (active={workspace_active}, limit={WORKSPACE_EXECUTION_LIMIT})"
                ),
                "workspaceId",
                super::RuntimeCapacity {
                    scope: "workspace".to_string(),
                    active: workspace_active,
                    limit: WORKSPACE_EXECUTION_LIMIT,
                    workspace_id: Some(request.plan.workspace_id.clone()),
                    holder_job_ids,
                    holder_workspace_ids,
                    holders_truncated: workspace_active > WORKSPACE_EXECUTION_LIMIT,
                },
            ));
        }

        let global_active: u32 = transaction
            .query_row(
                "SELECT COUNT(*) FROM concurrency_reservations WHERE state IN ('active','held_orphaned')",
                [],
                |row| row.get(0),
            )
            .map_err(|error| RuntimeError::from_sql(error, "cannot count global reservations"))?;
        if global_active >= request.global_limit {
            let holder_limit = request.global_limit.min(16);
            let (holder_job_ids, holder_workspace_ids) =
                capacity_holders(&transaction, None, holder_limit)?;
            return Err(RuntimeError::concurrency(
                format!(
                    "global execution concurrency limit reached (active={global_active}, limit={})",
                    request.global_limit
                ),
                "globalLimit",
                super::RuntimeCapacity {
                    scope: "global".to_string(),
                    active: global_active,
                    limit: request.global_limit,
                    workspace_id: None,
                    holder_job_ids,
                    holder_workspace_ids,
                    holders_truncated: global_active > holder_limit,
                },
            ));
        }
        transaction
            .execute(
                "INSERT INTO jobs(job_id,principal,client_request_id,request_digest,operation_digest,workspace_id,workspace_snapshot_json,execution_plan_json,execution_plan_digest,created_at_ms,desired_state,resolution,current_attempt_id,row_version) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,NULL,?12,0)",
                params![
                    job.job_id,
                    job.principal,
                    job.client_request_id,
                    job.request_digest,
                    job.operation_digest,
                    job.workspace_id,
                    job.workspace_snapshot_json,
                    job.execution_plan_json,
                    job.execution_plan_digest,
                    created_at_ms,
                    job.desired_state.as_db(),
                    attempt.attempt_id,
                ],
            )
            .map_err(|error| RuntimeError::from_sql(error, "cannot insert Job"))?;
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
            .map_err(|error| RuntimeError::from_sql(error, "cannot insert Attempt"))?;
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
            .map_err(|error| RuntimeError::from_sql(error, "cannot insert idempotency key"))?;

        transaction
            .execute(
                "INSERT INTO concurrency_reservations(reservation_id,attempt_id,global_limit,state,acquired_at_ms,released_at_ms,release_reason) VALUES(?1,?2,?3,?4,?5,NULL,NULL)",
                params![
                    reservation.reservation_id,
                    reservation.attempt_id,
                    reservation.global_limit,
                    reservation.state.as_db(),
                    reservation.acquired_at_ms,
                ],
            )
            .map_err(|error| RuntimeError::from_sql(error, "cannot reserve execution capacity"))?;

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
            &ConditionUpdate {
                condition_type: "reservation_held".to_string(),
                status: "true".to_string(),
                reason_code: "CAPACITY_RESERVED".to_string(),
                evidence_digest: sha256_bytes(reservation_id.as_bytes()),
                observed_at_ms: created_at_ms,
            },
        )?;
        transaction
            .commit()
            .map_err(|error| RuntimeError::from_sql(error, "cannot commit admission"))?;
        Ok(AdmissionOutcome::Created(Box::new(CreatedAdmission {
            job,
            attempt,
            reservation,
            launch_token,
        })))
    }

    pub fn get_job(&self, job_id: &str) -> RuntimeResult<RuntimeJobRecord> {
        let connection = self.open_connection()?;
        load_job(&connection, job_id)
    }

    pub fn get_attempt(&self, attempt_id: &str) -> RuntimeResult<AttemptRecord> {
        let connection = self.open_connection()?;
        load_attempt(&connection, attempt_id)
    }

    pub fn get_current_attempt(&self, job_id: &str) -> RuntimeResult<Option<AttemptRecord>> {
        let connection = self.open_connection()?;
        let attempt_id: Option<String> = connection
            .query_row(
                "SELECT current_attempt_id FROM jobs WHERE job_id=?1",
                [job_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| RuntimeError::from_sql(error, "cannot read current Attempt"))?
            .ok_or_else(|| {
                RuntimeError::new(
                    RuntimeErrorCode::JobNotFound,
                    "Job not found",
                    Some("jobId"),
                    false,
                )
            })?;
        attempt_id
            .map(|attempt_id| load_attempt(&connection, &attempt_id))
            .transpose()
    }

    pub fn get_reservation(&self, attempt_id: &str) -> RuntimeResult<ReservationRecord> {
        let connection = self.open_connection()?;
        load_reservation(&connection, attempt_id)
    }

    pub fn get_latest_attempt(&self, job_id: &str) -> RuntimeResult<Option<AttemptRecord>> {
        let connection = self.open_connection()?;
        let attempt_id: Option<String> = connection
            .query_row(
                "SELECT attempt_id FROM attempts WHERE job_id=?1 ORDER BY attempt_number DESC LIMIT 1",
                [job_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| RuntimeError::from_sql(error, "cannot find latest Attempt"))?;
        attempt_id
            .map(|attempt_id| load_attempt(&connection, &attempt_id))
            .transpose()
    }

    pub fn get_artifact(
        &self,
        job_id: &str,
        artifact_id: &str,
    ) -> RuntimeResult<RuntimeArtifactRecord> {
        let connection = self.open_connection()?;
        connection
            .query_row(
                "SELECT artifact_id,job_id,attempt_id,kind,relative_path,digest,media_type,byte_length,truncated,created_at_ms FROM artifacts WHERE job_id=?1 AND artifact_id=?2",
                params![job_id, artifact_id],
                |row| {
                    Ok(RuntimeArtifactRecord {
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
                },
            )
            .optional()
            .map_err(|error| RuntimeError::from_sql(error, "cannot load Artifact"))?
            .ok_or_else(|| RuntimeError::new(
                RuntimeErrorCode::ArtifactIdentityConflict,
                "Artifact not found for Job",
                Some("artifactId"),
                false,
            ))
    }

    pub(crate) fn job_snapshot(&self, job_id: &str) -> RuntimeResult<JobSnapshot> {
        let connection = self.open_connection()?;
        load_job_snapshot(&connection, job_id)
    }

    pub fn project_job(&self, job_id: &str) -> RuntimeResult<JobProjection> {
        Ok(self.job_snapshot(job_id)?.projection)
    }

    pub fn active_job_ids_for_workspace(&self, workspace_id: &str) -> RuntimeResult<Vec<String>> {
        let connection = self.open_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT DISTINCT jobs.job_id FROM jobs LEFT JOIN attempts ON attempts.job_id=jobs.job_id LEFT JOIN concurrency_reservations ON concurrency_reservations.attempt_id=attempts.attempt_id WHERE jobs.workspace_id=?1 AND (jobs.resolution IS NULL OR concurrency_reservations.state IN ('active','held_orphaned')) ORDER BY jobs.created_at_ms DESC,jobs.job_id DESC",
            )
            .map_err(|error| RuntimeError::from_sql(error, "cannot prepare active Workspace Job query"))?;
        let rows = statement
            .query_map([workspace_id], |row| row.get::<_, String>(0))
            .map_err(|error| RuntimeError::from_sql(error, "cannot query active Workspace Jobs"))?;
        rows.map(|row| {
            row.map_err(|error| RuntimeError::from_sql(error, "cannot decode active Workspace Job"))
        })
        .collect()
    }

    pub fn list_workspace_reconciliation_attempts(
        &self,
        workspace_id: &str,
        limit: u32,
    ) -> RuntimeResult<Vec<AttemptRecord>> {
        if limit == 0 {
            return Err(RuntimeError::invalid("limit must be positive", "limit"));
        }
        let connection = self.open_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT DISTINCT a.attempt_id,a.job_id,a.attempt_number,a.state,a.termination_intent,a.launch_token_digest,a.bundle_path,a.bundle_digest,a.boot_id,a.unit_name,a.invocation_id,a.control_group,a.main_pid,a.process_start_identity,a.runner_start_digest,a.result_digest,a.exit_code,a.infrastructure_error_digest,a.created_at_ms,a.started_at_ms,a.finished_at_ms,a.row_version FROM attempts a JOIN jobs j ON j.job_id=a.job_id JOIN concurrency_reservations r ON r.attempt_id=a.attempt_id WHERE j.workspace_id=?1 AND (a.attempt_id=j.current_attempt_id OR r.state IN ('active','held_orphaned')) ORDER BY a.created_at_ms DESC,a.attempt_id DESC LIMIT ?2",
            )
            .map_err(|error| RuntimeError::from_sql(error, "cannot prepare Workspace reconciliation scan"))?;
        let rows = statement
            .query_map(params![workspace_id, limit], raw_attempt_from_row)
            .map_err(|error| RuntimeError::from_sql(error, "cannot scan Workspace Attempts"))?;
        rows.map(|row| {
            row.map_err(|error| RuntimeError::from_sql(error, "cannot decode Workspace Attempt"))?
                .into_record()
        })
        .collect()
    }

    pub fn list_jobs(
        &self,
        request: &RuntimeJobListRequest,
    ) -> RuntimeResult<RuntimeJobListResult> {
        if request.limit == 0 || request.limit > MAX_RUNTIME_LIST_LIMIT {
            return Err(RuntimeError::invalid(
                format!("limit must be in 1..={MAX_RUNTIME_LIST_LIMIT}"),
                "limit",
            ));
        }
        if let Some(client_request_id) = request.client_request_id.as_deref() {
            validate_identifier(client_request_id, "clientRequestId")?;
        }
        if let Some(workspace_id) = request.workspace_id.as_deref() {
            validate_identifier(workspace_id, "workspaceId")?;
        }
        let connection = self.open_connection()?;
        let transaction = connection.unchecked_transaction().map_err(|error| {
            RuntimeError::from_sql(error, "cannot begin Job list read snapshot")
        })?;
        let fetch_limit = request.limit + 1;
        let mut jobs = Vec::new();
        match (
            &request.client_request_id,
            &request.workspace_id,
            &request.cursor,
        ) {
            (Some(client_request_id), Some(workspace_id), Some(cursor)) => {
                let mut statement = transaction
                    .prepare(
                        "SELECT job_id,principal,client_request_id,request_digest,operation_digest,workspace_id,workspace_snapshot_json,execution_plan_json,execution_plan_digest,created_at_ms,desired_state,resolution,current_attempt_id,row_version FROM jobs WHERE client_request_id=?1 AND workspace_id=?2 AND (created_at_ms<?3 OR (created_at_ms=?3 AND job_id<?4)) ORDER BY created_at_ms DESC,job_id DESC LIMIT ?5",
                    )
                    .map_err(|error| RuntimeError::from_sql(error, "cannot prepare identity-bounded Job list"))?;
                let rows = statement
                    .query_map(
                        params![
                            client_request_id,
                            workspace_id,
                            cursor.created_at_ms,
                            cursor.job_id,
                            fetch_limit
                        ],
                        raw_job_from_row,
                    )
                    .map_err(|error| {
                        RuntimeError::from_sql(error, "cannot query identity-bounded Job list")
                    })?;
                for row in rows {
                    jobs.push(
                        row.map_err(|error| {
                            RuntimeError::from_sql(error, "cannot decode Job row")
                        })?
                        .into_record()?,
                    );
                }
            }
            (Some(client_request_id), Some(workspace_id), None) => {
                let mut statement = transaction
                    .prepare(
                        "SELECT job_id,principal,client_request_id,request_digest,operation_digest,workspace_id,workspace_snapshot_json,execution_plan_json,execution_plan_digest,created_at_ms,desired_state,resolution,current_attempt_id,row_version FROM jobs WHERE client_request_id=?1 AND workspace_id=?2 ORDER BY created_at_ms DESC,job_id DESC LIMIT ?3",
                    )
                    .map_err(|error| RuntimeError::from_sql(error, "cannot prepare identity-bounded Job list"))?;
                let rows = statement
                    .query_map(
                        params![client_request_id, workspace_id, fetch_limit],
                        raw_job_from_row,
                    )
                    .map_err(|error| {
                        RuntimeError::from_sql(error, "cannot query identity-bounded Job list")
                    })?;
                for row in rows {
                    jobs.push(
                        row.map_err(|error| {
                            RuntimeError::from_sql(error, "cannot decode Job row")
                        })?
                        .into_record()?,
                    );
                }
            }
            (Some(client_request_id), None, Some(cursor)) => {
                let mut statement = transaction
                    .prepare(
                        "SELECT job_id,principal,client_request_id,request_digest,operation_digest,workspace_id,workspace_snapshot_json,execution_plan_json,execution_plan_digest,created_at_ms,desired_state,resolution,current_attempt_id,row_version FROM jobs WHERE client_request_id=?1 AND (created_at_ms<?2 OR (created_at_ms=?2 AND job_id<?3)) ORDER BY created_at_ms DESC,job_id DESC LIMIT ?4",
                    )
                    .map_err(|error| RuntimeError::from_sql(error, "cannot prepare filtered Job list"))?;
                let rows = statement
                    .query_map(
                        params![
                            client_request_id,
                            cursor.created_at_ms,
                            cursor.job_id,
                            fetch_limit
                        ],
                        raw_job_from_row,
                    )
                    .map_err(|error| {
                        RuntimeError::from_sql(error, "cannot query filtered Job list")
                    })?;
                for row in rows {
                    jobs.push(
                        row.map_err(|error| {
                            RuntimeError::from_sql(error, "cannot decode Job row")
                        })?
                        .into_record()?,
                    );
                }
            }
            (Some(client_request_id), None, None) => {
                let mut statement = transaction
                    .prepare(
                        "SELECT job_id,principal,client_request_id,request_digest,operation_digest,workspace_id,workspace_snapshot_json,execution_plan_json,execution_plan_digest,created_at_ms,desired_state,resolution,current_attempt_id,row_version FROM jobs WHERE client_request_id=?1 ORDER BY created_at_ms DESC,job_id DESC LIMIT ?2",
                    )
                    .map_err(|error| RuntimeError::from_sql(error, "cannot prepare filtered Job list"))?;
                let rows = statement
                    .query_map(params![client_request_id, fetch_limit], raw_job_from_row)
                    .map_err(|error| {
                        RuntimeError::from_sql(error, "cannot query filtered Job list")
                    })?;
                for row in rows {
                    jobs.push(
                        row.map_err(|error| {
                            RuntimeError::from_sql(error, "cannot decode Job row")
                        })?
                        .into_record()?,
                    );
                }
            }
            (None, Some(workspace_id), Some(cursor)) => {
                let mut statement = transaction
                    .prepare(
                        "SELECT job_id,principal,client_request_id,request_digest,operation_digest,workspace_id,workspace_snapshot_json,execution_plan_json,execution_plan_digest,created_at_ms,desired_state,resolution,current_attempt_id,row_version FROM jobs WHERE workspace_id=?1 AND (created_at_ms<?2 OR (created_at_ms=?2 AND job_id<?3)) ORDER BY created_at_ms DESC,job_id DESC LIMIT ?4",
                    )
                    .map_err(|error| RuntimeError::from_sql(error, "cannot prepare Workspace Job list"))?;
                let rows = statement
                    .query_map(
                        params![
                            workspace_id,
                            cursor.created_at_ms,
                            cursor.job_id,
                            fetch_limit
                        ],
                        raw_job_from_row,
                    )
                    .map_err(|error| {
                        RuntimeError::from_sql(error, "cannot query Workspace Job list")
                    })?;
                for row in rows {
                    jobs.push(
                        row.map_err(|error| {
                            RuntimeError::from_sql(error, "cannot decode Job row")
                        })?
                        .into_record()?,
                    );
                }
            }
            (None, Some(workspace_id), None) => {
                let mut statement = transaction
                    .prepare(
                        "SELECT job_id,principal,client_request_id,request_digest,operation_digest,workspace_id,workspace_snapshot_json,execution_plan_json,execution_plan_digest,created_at_ms,desired_state,resolution,current_attempt_id,row_version FROM jobs WHERE workspace_id=?1 ORDER BY created_at_ms DESC,job_id DESC LIMIT ?2",
                    )
                    .map_err(|error| RuntimeError::from_sql(error, "cannot prepare Workspace Job list"))?;
                let rows = statement
                    .query_map(params![workspace_id, fetch_limit], raw_job_from_row)
                    .map_err(|error| {
                        RuntimeError::from_sql(error, "cannot query Workspace Job list")
                    })?;
                for row in rows {
                    jobs.push(
                        row.map_err(|error| {
                            RuntimeError::from_sql(error, "cannot decode Job row")
                        })?
                        .into_record()?,
                    );
                }
            }
            (None, None, Some(cursor)) => {
                let mut statement = transaction
                    .prepare(
                        "SELECT job_id,principal,client_request_id,request_digest,operation_digest,workspace_id,workspace_snapshot_json,execution_plan_json,execution_plan_digest,created_at_ms,desired_state,resolution,current_attempt_id,row_version FROM jobs WHERE created_at_ms<?1 OR (created_at_ms=?1 AND job_id<?2) ORDER BY created_at_ms DESC,job_id DESC LIMIT ?3",
                    )
                    .map_err(|error| RuntimeError::from_sql(error, "cannot prepare Job list"))?;
                let rows = statement
                    .query_map(
                        params![cursor.created_at_ms, cursor.job_id, fetch_limit],
                        raw_job_from_row,
                    )
                    .map_err(|error| RuntimeError::from_sql(error, "cannot query Job list"))?;
                for row in rows {
                    jobs.push(
                        row.map_err(|error| {
                            RuntimeError::from_sql(error, "cannot decode Job row")
                        })?
                        .into_record()?,
                    );
                }
            }
            (None, None, None) => {
                let mut statement = transaction
                    .prepare(
                        "SELECT job_id,principal,client_request_id,request_digest,operation_digest,workspace_id,workspace_snapshot_json,execution_plan_json,execution_plan_digest,created_at_ms,desired_state,resolution,current_attempt_id,row_version FROM jobs ORDER BY created_at_ms DESC,job_id DESC LIMIT ?1",
                    )
                    .map_err(|error| RuntimeError::from_sql(error, "cannot prepare Job list"))?;
                let rows = statement
                    .query_map([fetch_limit], raw_job_from_row)
                    .map_err(|error| RuntimeError::from_sql(error, "cannot query Job list"))?;
                for row in rows {
                    jobs.push(
                        row.map_err(|error| {
                            RuntimeError::from_sql(error, "cannot decode Job row")
                        })?
                        .into_record()?,
                    );
                }
            }
        }

        let has_more = jobs.len() > request.limit as usize;
        jobs.truncate(request.limit as usize);
        let next_cursor = if has_more {
            jobs.last().map(|job| RuntimeJobListCursor {
                created_at_ms: job.created_at_ms,
                job_id: job.job_id.clone(),
            })
        } else {
            None
        };
        let observed_at_ms = now_ms()?;
        let mut summaries = Vec::with_capacity(jobs.len());
        for job in jobs {
            let attempt = match job.current_attempt_id.as_deref() {
                Some(attempt_id) => Some(load_attempt(&transaction, attempt_id)?),
                None => {
                    let attempt_id: Option<String> = transaction
                        .query_row(
                            "SELECT attempt_id FROM attempts WHERE job_id=?1 ORDER BY attempt_number DESC LIMIT 1",
                            [&job.job_id],
                            |row| row.get(0),
                        )
                        .optional()
                        .map_err(|error| RuntimeError::from_sql(error, "cannot find latest Attempt"))?;
                    attempt_id
                        .map(|attempt_id| load_attempt(&transaction, &attempt_id))
                        .transpose()?
                }
            };
            let recovery_condition_active = attempt
                .as_ref()
                .map(|attempt| attempt_recovery_condition_active(&transaction, &attempt.attempt_id))
                .transpose()?
                .unwrap_or(false);
            let artifact_count: u32 = transaction
                .query_row(
                    "SELECT COUNT(*) FROM artifacts WHERE job_id=?1",
                    [&job.job_id],
                    |row| row.get(0),
                )
                .map_err(|error| RuntimeError::from_sql(error, "cannot count Job Artifacts"))?;
            let execution_reason_code =
                job_execution_reason_code(&transaction, &job.job_id, job.resolution.is_some())?;
            let projection = project_job(
                &job,
                attempt.as_ref(),
                recovery_condition_active,
                artifact_count > 0,
                execution_reason_code,
            );
            let plan: RuntimeExecutionPlan = serde_json::from_str(&job.execution_plan_json)
                .map_err(|error| {
                    RuntimeError::new(
                        RuntimeErrorCode::RegistryCorrupt,
                        format!("stored execution plan is invalid: {error}"),
                        Some("executionPlan"),
                        false,
                    )
                })?;
            let executable_name = Path::new(&plan.executable)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&plan.executable)
                .to_string();
            let cwd_relative = Path::new(&plan.cwd)
                .strip_prefix(&plan.workspace_path)
                .ok()
                .and_then(|path| path.to_str())
                .filter(|path| !path.is_empty())
                .unwrap_or(".")
                .to_string();
            let started_at_ms = attempt.as_ref().and_then(|attempt| attempt.started_at_ms);
            let finished_at_ms = attempt.as_ref().and_then(|attempt| attempt.finished_at_ms);
            let duration_start = started_at_ms.unwrap_or(job.created_at_ms);
            let duration_end = finished_at_ms.unwrap_or(observed_at_ms);
            summaries.push(RuntimeJobSummary {
                job_id: job.job_id,
                status: projection.status,
                desired_state: projection.desired_state,
                attempt_id: projection.attempt_id,
                attempt_state: projection.attempt_state,
                termination_intent: projection.termination_intent,
                exit_code: projection.exit_code,
                execution_terminal: projection.execution_terminal,
                execution_disposition: projection.execution_disposition,
                execution_reason_code: projection.execution_reason_code,
                delivery_disposition: projection.delivery_disposition,
                recovery_required: projection.recovery_required,
                semantic_completion_evaluated: projection.semantic_completion_evaluated,
                client_request_id: job.client_request_id,
                workspace_id: job.workspace_id,
                source_revision: plan.source_revision,
                executable_name,
                cwd_relative,
                created_at_ms: job.created_at_ms,
                started_at_ms,
                finished_at_ms,
                duration_ms: duration_end.saturating_sub(duration_start),
                result_available: projection.result_available,
                artifacts_available: projection.artifacts_available,
                artifact_count,
                poll_after_ms: projection.poll_after_ms,
            });
        }
        transaction.commit().map_err(|error| {
            RuntimeError::from_sql(error, "cannot close Job list read snapshot")
        })?;
        Ok(RuntimeJobListResult {
            jobs: summaries,
            next_cursor,
        })
    }

    pub fn list_artifacts(&self, job_id: &str) -> RuntimeResult<Vec<RuntimeArtifactRecord>> {
        let connection = self.open_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT artifact_id,job_id,attempt_id,kind,relative_path,digest,media_type,byte_length,truncated,created_at_ms FROM artifacts WHERE job_id=?1 ORDER BY created_at_ms,artifact_id",
            )
            .map_err(|error| RuntimeError::from_sql(error, "cannot prepare Artifact query"))?;
        let rows = statement
            .query_map([job_id], |row| {
                Ok(RuntimeArtifactRecord {
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
            .map_err(|error| RuntimeError::from_sql(error, "cannot query Artifacts"))?;
        rows.map(|row| row.map_err(|error| RuntimeError::from_sql(error, "cannot decode Artifact")))
            .collect()
    }

    pub fn execution_plan(&self, job_id: &str) -> RuntimeResult<super::RuntimeExecutionPlan> {
        let job = self.get_job(job_id)?;
        serde_json::from_str(&job.execution_plan_json).map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                format!("stored execution plan is invalid: {error}"),
                Some("executionPlan"),
                false,
            )
        })
    }

    pub fn launch_token(&self, attempt_id: &str) -> RuntimeResult<String> {
        let attempt = self.get_attempt(attempt_id)?;
        let job = self.get_job(&attempt.job_id)?;
        let token = sha256_bytes(
            format!(
                "runtime-launch-v1\0{}\0{}",
                attempt.attempt_id, job.operation_digest
            )
            .as_bytes(),
        );
        if sha256_bytes(token.as_bytes()) != attempt.launch_token_digest {
            return Err(RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
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
    ) -> RuntimeResult<AttemptRecord> {
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
            .map_err(|error| RuntimeError::from_sql(error, "cannot bind bundle identity"))?;
        if changed != 1 {
            return Err(state_conflict("Attempt changed while binding bundle"));
        }
        upsert_condition(
            &transaction,
            attempt_id,
            &ConditionUpdate {
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
            .map_err(|error| RuntimeError::from_sql(error, "cannot commit bundle identity"))?;
        load_attempt(&connection, attempt_id)
    }

    pub fn mark_dispatch_issued(
        &self,
        attempt_id: &str,
        expected_row_version: u64,
        observed_at_ms: u64,
    ) -> RuntimeResult<AttemptRecord> {
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
            .map_err(|error| RuntimeError::from_sql(error, "cannot persist dispatch intent"))?;
        if changed != 1 {
            return Err(state_conflict("Attempt changed before dispatch intent"));
        }
        let evidence = attempt.bundle_digest.clone().unwrap_or_default();
        upsert_condition(
            &transaction,
            attempt_id,
            &ConditionUpdate {
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
            .map_err(|error| RuntimeError::from_sql(error, "cannot commit dispatch intent"))?;
        load_attempt(&connection, attempt_id)
    }

    pub fn bind_running(
        &self,
        attempt_id: &str,
        expected_row_version: u64,
        identity: &RunnerIdentity,
    ) -> RuntimeResult<AttemptRecord> {
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
            .map_err(|error| RuntimeError::from_sql(error, "cannot bind Runner identity"))?;
        if changed != 1 {
            return Err(state_conflict("Attempt changed while binding Runner"));
        }
        upsert_condition(
            &transaction,
            attempt_id,
            &ConditionUpdate {
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
            .map_err(|error| RuntimeError::from_sql(error, "cannot commit Runner identity"))?;
        load_attempt(&connection, attempt_id)
    }

    pub fn request_cancel(
        &self,
        job_id: &str,
        observed_at_ms: u64,
    ) -> RuntimeResult<JobProjection> {
        let mut connection = self.open_connection()?;
        let transaction = immediate(&mut connection, "cancel-intent transaction")?;
        let job = load_job(&transaction, job_id)?;
        if job.resolution.is_some() {
            if job.resolution == Some(JobResolution::Orphaned) {
                let attempt_id: String = transaction
                    .query_row(
                        "SELECT attempt_id FROM attempts WHERE job_id=?1 ORDER BY attempt_number DESC LIMIT 1",
                        [job_id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|error| RuntimeError::from_sql(error, "cannot load orphaned Job Attempt"))?
                    .ok_or_else(|| {
                        RuntimeError::new(
                            RuntimeErrorCode::RegistryCorrupt,
                            "orphaned Job has no Attempt",
                            Some("jobId"),
                            false,
                        )
                    })?;
                let attempt = load_attempt(&transaction, &attempt_id)?;
                let reservation = load_reservation(&transaction, &attempt_id)?;
                if attempt.state != AttemptState::Orphaned
                    || reservation.state != ReservationState::HeldOrphaned
                {
                    return Err(RuntimeError::new(
                        RuntimeErrorCode::ReconciliationRequired,
                        "orphaned Job is not backed by a held orphaned Attempt",
                        Some("jobId"),
                        false,
                    ));
                }
                let intent_changed = job.desired_state != JobDesiredState::Cancelled
                    || attempt.termination_intent != AttemptTerminationIntent::StopRequested;
                transaction
                    .execute(
                        "UPDATE jobs SET desired_state='cancelled',row_version=row_version+1 WHERE job_id=?1 AND resolution='orphaned' AND desired_state!='cancelled'",
                        [job_id],
                    )
                    .map_err(|error| RuntimeError::from_sql(error, "cannot persist orphan cancel intent"))?;
                transaction
                    .execute(
                        "UPDATE attempts SET termination_intent='stop_requested',row_version=row_version+1 WHERE attempt_id=?1 AND state='orphaned' AND termination_intent!='stop_requested'",
                        [&attempt_id],
                    )
                    .map_err(|error| RuntimeError::from_sql(error, "cannot persist orphan stop intent"))?;
                if intent_changed {
                    append_event(
                        &transaction,
                        job_id,
                        Some(&attempt_id),
                        "STOP_REQUESTED",
                        "SYSTEM_DERIVED",
                        Some(AttemptState::Orphaned),
                        Some(AttemptState::Orphaned),
                        "ORPHAN_CANCEL_INTENT_COMMITTED",
                        serde_json::json!({}),
                        observed_at_ms,
                    )?;
                }
                transaction.commit().map_err(|error| {
                    RuntimeError::from_sql(error, "cannot commit orphan cancel intent")
                })?;
                return Ok(load_job_snapshot(&connection, job_id)?.projection);
            }
            transaction.commit().map_err(|error| {
                RuntimeError::from_sql(error, "cannot close terminal cancel replay")
            })?;
            return Ok(load_job_snapshot(&connection, job_id)?.projection);
        }
        let attempt_id = job.current_attempt_id.clone().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                "unresolved Job has no current Attempt",
                Some("currentAttemptId"),
                false,
            )
        })?;
        let attempt = load_attempt(&transaction, &attempt_id)?;
        if attempt.state.is_terminal() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::ReconciliationRequired,
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
            .map_err(|error| RuntimeError::from_sql(error, "cannot persist cancel intent"))?;
        if attempt.state == AttemptState::Accepted {
            let result_digest = sha256_bytes(
                format!("runtime-cancel-before-dispatch\0{job_id}\0{attempt_id}").as_bytes(),
            );
            transaction
                .execute(
                    "UPDATE attempts SET state='cancelled',termination_intent='stop_requested',result_digest=?1,finished_at_ms=?2,row_version=row_version+1 WHERE attempt_id=?3 AND state='accepted'",
                    params![result_digest, observed_at_ms, attempt_id],
                )
                .map_err(|error| RuntimeError::from_sql(error, "cannot cancel accepted Attempt"))?;
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
                .map_err(|error| RuntimeError::from_sql(error, "cannot resolve cancelled Job"))?;
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
                .map_err(|error| RuntimeError::from_sql(error, "cannot move Attempt to stopping"))?;
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
            .map_err(|error| RuntimeError::from_sql(error, "cannot commit cancel intent"))?;
        Ok(load_job_snapshot(&connection, job_id)?.projection)
    }

    pub fn commit_terminal(&self, request: &TerminalCommit) -> RuntimeResult<JobProjection> {
        if !request.state.is_terminal() {
            return Err(RuntimeError::invalid(
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
                transaction.commit().map_err(|error| {
                    RuntimeError::from_sql(error, "cannot close terminal replay")
                })?;
                return Ok(load_job_snapshot(&connection, &job.job_id)?.projection);
            }
            return Err(RuntimeError::new(
                RuntimeErrorCode::ResultIdentityConflict,
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
        if !attempt.state.can_transition_to(request.state) {
            return Err(state_conflict(format!(
                "invalid Attempt transition {:?} -> {:?}",
                attempt.state, request.state
            )));
        }
        if job.resolution.is_some() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::JobAlreadyResolved,
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
            .map_err(|error| RuntimeError::from_sql(error, "cannot commit terminal Attempt"))?;
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
                return Err(RuntimeError::new(
                    RuntimeErrorCode::ArtifactIdentityConflict,
                    format!("cannot register Artifact {}: {error}", artifact.artifact_id),
                    Some("artifacts"),
                    false,
                ));
            }
        }
        upsert_condition(
            &transaction,
            &attempt.attempt_id,
            &ConditionUpdate {
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
            .map_err(|error| RuntimeError::from_sql(error, "cannot resolve Job"))?;
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
            .map_err(|error| RuntimeError::from_sql(error, "cannot commit terminal transaction"))?;
        Ok(load_job_snapshot(&connection, &attempt.job_id)?.projection)
    }

    pub fn list_nonterminal_attempts(&self) -> RuntimeResult<Vec<AttemptRecord>> {
        self.list_nonterminal_attempts_with_limit(None)
    }

    pub fn list_nonterminal_attempts_bounded(
        &self,
        limit: u32,
    ) -> RuntimeResult<Vec<AttemptRecord>> {
        if limit == 0 {
            return Err(RuntimeError::invalid("limit must be positive", "limit"));
        }
        self.list_nonterminal_attempts_with_limit(Some(limit))
    }

    fn list_nonterminal_attempts_with_limit(
        &self,
        limit: Option<u32>,
    ) -> RuntimeResult<Vec<AttemptRecord>> {
        let connection = self.open_connection()?;
        let sql = if limit.is_some() {
            "SELECT attempt_id,job_id,attempt_number,state,termination_intent,launch_token_digest,bundle_path,bundle_digest,boot_id,unit_name,invocation_id,control_group,main_pid,process_start_identity,runner_start_digest,result_digest,exit_code,infrastructure_error_digest,created_at_ms,started_at_ms,finished_at_ms,row_version FROM attempts WHERE state NOT IN ('succeeded','failed','timed_out','cancelled','lost','orphaned') ORDER BY created_at_ms DESC,attempt_id DESC LIMIT ?1"
        } else {
            "SELECT attempt_id,job_id,attempt_number,state,termination_intent,launch_token_digest,bundle_path,bundle_digest,boot_id,unit_name,invocation_id,control_group,main_pid,process_start_identity,runner_start_digest,result_digest,exit_code,infrastructure_error_digest,created_at_ms,started_at_ms,finished_at_ms,row_version FROM attempts WHERE state NOT IN ('succeeded','failed','timed_out','cancelled','lost','orphaned') ORDER BY created_at_ms,attempt_id"
        };
        let mut statement = connection
            .prepare(sql)
            .map_err(|error| RuntimeError::from_sql(error, "cannot prepare reconciliation scan"))?;
        if let Some(limit) = limit {
            let rows = statement
                .query_map([limit], raw_attempt_from_row)
                .map_err(|error| {
                    RuntimeError::from_sql(error, "cannot scan nonterminal Attempts")
                })?;
            rows.map(|row| {
                row.map_err(|error| RuntimeError::from_sql(error, "cannot decode Attempt row"))?
                    .into_record()
            })
            .collect()
        } else {
            let rows = statement
                .query_map([], raw_attempt_from_row)
                .map_err(|error| {
                    RuntimeError::from_sql(error, "cannot scan nonterminal Attempts")
                })?;
            rows.map(|row| {
                row.map_err(|error| RuntimeError::from_sql(error, "cannot decode Attempt row"))?
                    .into_record()
            })
            .collect()
        }
    }

    pub fn list_maintenance_attempts_bounded(
        &self,
        limit: u32,
    ) -> RuntimeResult<Vec<AttemptRecord>> {
        if limit == 0 {
            return Err(RuntimeError::invalid("limit must be positive", "limit"));
        }
        let connection = self.open_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT a.attempt_id,a.job_id,a.attempt_number,a.state,a.termination_intent,a.launch_token_digest,a.bundle_path,a.bundle_digest,a.boot_id,a.unit_name,a.invocation_id,a.control_group,a.main_pid,a.process_start_identity,a.runner_start_digest,a.result_digest,a.exit_code,a.infrastructure_error_digest,a.created_at_ms,a.started_at_ms,a.finished_at_ms,a.row_version FROM attempts a LEFT JOIN concurrency_reservations r ON r.attempt_id=a.attempt_id WHERE a.state NOT IN ('succeeded','failed','timed_out','cancelled','lost','orphaned') OR (a.state='orphaned' AND r.state='held_orphaned') OR EXISTS(SELECT 1 FROM attempt_conditions c WHERE c.attempt_id=a.attempt_id AND c.condition_type='recovery_required' AND c.status='true') ORDER BY CASE WHEN EXISTS(SELECT 1 FROM attempt_conditions c WHERE c.attempt_id=a.attempt_id AND c.condition_type='recovery_required' AND c.status='true') THEN 0 WHEN a.state='orphaned' AND r.state='held_orphaned' THEN 1 ELSE 2 END,a.created_at_ms,a.attempt_id LIMIT ?1",
            )
            .map_err(|error| RuntimeError::from_sql(error, "cannot prepare maintenance reconciliation scan"))?;
        let rows = statement
            .query_map([limit], raw_attempt_from_row)
            .map_err(|error| RuntimeError::from_sql(error, "cannot scan maintenance Attempts"))?;
        rows.map(|row| {
            row.map_err(|error| {
                RuntimeError::from_sql(error, "cannot decode maintenance Attempt row")
            })?
            .into_record()
        })
        .collect()
    }

    pub fn list_held_orphaned_attempts(&self) -> RuntimeResult<Vec<AttemptRecord>> {
        let connection = self.open_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT a.attempt_id,a.job_id,a.attempt_number,a.state,a.termination_intent,a.launch_token_digest,a.bundle_path,a.bundle_digest,a.boot_id,a.unit_name,a.invocation_id,a.control_group,a.main_pid,a.process_start_identity,a.runner_start_digest,a.result_digest,a.exit_code,a.infrastructure_error_digest,a.created_at_ms,a.started_at_ms,a.finished_at_ms,a.row_version FROM attempts a JOIN concurrency_reservations r ON r.attempt_id=a.attempt_id WHERE a.state='orphaned' AND r.state='held_orphaned' ORDER BY a.created_at_ms,a.attempt_id",
            )
            .map_err(|error| RuntimeError::from_sql(error, "cannot prepare orphan reconciliation scan"))?;
        let rows = statement
            .query_map([], raw_attempt_from_row)
            .map_err(|error| RuntimeError::from_sql(error, "cannot scan held orphaned Attempts"))?;
        rows.map(|row| {
            row.map_err(|error| {
                RuntimeError::from_sql(error, "cannot decode orphaned Attempt row")
            })?
            .into_record()
        })
        .collect()
    }

    pub fn recover_orphaned_terminal(
        &self,
        request: &TerminalCommit,
    ) -> RuntimeResult<JobProjection> {
        if !matches!(
            request.state,
            AttemptState::Succeeded
                | AttemptState::Failed
                | AttemptState::TimedOut
                | AttemptState::Cancelled
                | AttemptState::Lost
        ) {
            return Err(RuntimeError::invalid(
                "orphan recovery requires a conclusive terminal state",
                "state",
            ));
        }
        validate_digest(&request.result_digest, "resultDigest")?;
        for artifact in &request.artifacts {
            validate_artifact_registration(artifact)?;
        }
        let mut connection = self.open_connection()?;
        let transaction = immediate(&mut connection, "orphan recovery transaction")?;
        let attempt = load_attempt(&transaction, &request.attempt_id)?;
        let job = load_job(&transaction, &attempt.job_id)?;
        let reservation = load_reservation(&transaction, &attempt.attempt_id)?;
        if attempt.state != AttemptState::Orphaned
            || job.resolution != Some(JobResolution::Orphaned)
            || reservation.state != ReservationState::HeldOrphaned
        {
            return Err(RuntimeError::new(
                RuntimeErrorCode::OrphanRemediationDenied,
                "Attempt, Job, or reservation changed before Runner-result recovery",
                Some("attemptId"),
                false,
            ));
        }
        if attempt.row_version != request.expected_row_version {
            return Err(state_conflict(
                "Attempt row version changed before orphan recovery",
            ));
        }
        let changed = transaction
            .execute(
                "UPDATE attempts SET state=?1,result_digest=?2,exit_code=?3,infrastructure_error_digest=?4,finished_at_ms=?5,row_version=row_version+1 WHERE attempt_id=?6 AND row_version=?7 AND state='orphaned'",
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
            .map_err(|error| RuntimeError::from_sql(error, "cannot recover orphaned Attempt"))?;
        if changed != 1 {
            return Err(state_conflict("Attempt changed during orphan recovery"));
        }
        for artifact in &request.artifacts {
            transaction
                .execute(
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
                )
                .map_err(|error| RuntimeError::new(
                    RuntimeErrorCode::ArtifactIdentityConflict,
                    format!("cannot register recovered Artifact {}: {error}", artifact.artifact_id),
                    Some("artifacts"),
                    false,
                ))?;
        }
        upsert_condition(
            &transaction,
            &attempt.attempt_id,
            &ConditionUpdate {
                condition_type: "result_available".to_string(),
                status: "true".to_string(),
                reason_code: request.reason_code.clone(),
                evidence_digest: request.result_digest.clone(),
                observed_at_ms: request.finished_at_ms,
            },
        )?;
        release_reservation(
            &transaction,
            &attempt.attempt_id,
            request.finished_at_ms,
            &request.reason_code,
        )?;
        let resolution = resolution_for_state(request.state)?;
        transaction
            .execute(
                "UPDATE jobs SET resolution=?1,row_version=row_version+1 WHERE job_id=?2 AND resolution='orphaned'",
                params![resolution.as_db(), attempt.job_id],
            )
            .map_err(|error| RuntimeError::from_sql(error, "cannot correct orphaned Job resolution"))?;
        append_event(
            &transaction,
            &attempt.job_id,
            Some(&attempt.attempt_id),
            "RUNNER_RESULT_RECOVERED",
            "SYSTEM_OBSERVED",
            Some(AttemptState::Orphaned),
            Some(request.state),
            &request.reason_code,
            serde_json::json!({"resultDigest": request.result_digest, "exitCode": request.exit_code}),
            request.finished_at_ms,
        )?;
        append_event(
            &transaction,
            &attempt.job_id,
            Some(&attempt.attempt_id),
            "JOB_RESOLUTION_CORRECTED",
            "SYSTEM_DERIVED",
            Some(AttemptState::Orphaned),
            Some(request.state),
            &request.reason_code,
            serde_json::json!({"resolution": resolution.as_db()}),
            request.finished_at_ms,
        )?;
        transaction
            .commit()
            .map_err(|error| RuntimeError::from_sql(error, "cannot commit orphan recovery"))?;
        Ok(load_job_snapshot(&connection, &attempt.job_id)?.projection)
    }

    pub(crate) fn repair_admin_batch(
        &self,
        operations: &[AdminRepairOperation],
    ) -> RuntimeResult<()> {
        let mut connection = self.open_connection()?;
        let transaction = immediate(&mut connection, "administrative Runtime repair batch")?;
        for operation in operations {
            match operation {
                AdminRepairOperation::Terminal { terminal, audit } => {
                    repair_terminal_admin_transaction(&transaction, terminal, audit)?;
                }
                AdminRepairOperation::Reservation {
                    attempt_id,
                    expected_attempt_row_version,
                    audit,
                } => {
                    repair_terminal_reservation_admin_transaction(
                        &transaction,
                        attempt_id,
                        *expected_attempt_row_version,
                        audit,
                    )?;
                }
            }
        }
        let remaining = inspect_runtime_invariants_connection(&transaction)?;
        if !remaining.is_empty() {
            return Err(RuntimeError::new(
                RuntimeErrorCode::ReconciliationRequired,
                format!(
                    "administrative Runtime repair would leave {} invariant violations",
                    remaining.len()
                ),
                Some("violations"),
                false,
            ));
        }
        transaction.commit().map_err(|error| {
            RuntimeError::from_sql(error, "cannot commit administrative Runtime repair batch")
        })
    }

    pub fn converge_terminal_reservation(
        &self,
        attempt_id: &str,
        observed_at_ms: u64,
    ) -> RuntimeResult<bool> {
        let mut connection = self.open_connection()?;
        let attempt = load_attempt(&connection, attempt_id)?;
        let job = load_job(&connection, &attempt.job_id)?;
        let reservation = load_reservation(&connection, attempt_id)?;
        let target = terminal_reservation_target(&attempt, &job)?;
        if reservation.state == target {
            return Ok(false);
        }

        let transaction = immediate(&mut connection, "terminal reservation convergence")?;
        let attempt = load_attempt(&transaction, attempt_id)?;
        let job = load_job(&transaction, &attempt.job_id)?;
        let reservation = load_reservation(&transaction, attempt_id)?;
        let target = terminal_reservation_target(&attempt, &job)?;
        if reservation.state == target {
            transaction.commit().map_err(|error| {
                RuntimeError::from_sql(error, "cannot close converged terminal reservation")
            })?;
            return Ok(false);
        }
        if target == ReservationState::HeldOrphaned {
            hold_orphaned_reservation(
                &transaction,
                attempt_id,
                observed_at_ms,
                "TERMINAL_ORPHAN_RESERVATION_CONVERGED",
            )?;
        } else {
            release_reservation(
                &transaction,
                attempt_id,
                observed_at_ms,
                "TERMINAL_RESERVATION_CONVERGED",
            )?;
        }
        append_event(
            &transaction,
            &attempt.job_id,
            Some(attempt_id),
            "TERMINAL_RESERVATION_CONVERGED",
            "SYSTEM_DERIVED",
            Some(attempt.state),
            Some(attempt.state),
            "TERMINAL_RESERVATION_CONVERGED",
            serde_json::json!({
                "previousReservationState": reservation.state.as_db(),
                "newReservationState": target.as_db(),
            }),
            observed_at_ms,
        )?;
        transaction.commit().map_err(|error| {
            RuntimeError::from_sql(error, "cannot commit terminal reservation convergence")
        })?;
        Ok(true)
    }

    pub fn inspect_runtime_invariants(&self) -> RuntimeResult<Vec<RuntimeInvariantViolation>> {
        let connection = self.open_connection()?;
        inspect_runtime_invariants_connection(&connection)
    }

    pub fn record_reconciliation_failure(
        &self,
        attempt: &AttemptRecord,
        error: &RuntimeError,
        observed_at_ms: u64,
    ) -> RuntimeResult<()> {
        let reason_code = error.code.as_str();
        let evidence_digest = sha256_bytes(
            format!(
                "runtime-reconciliation-failure\0{}\0{}\0{}",
                attempt.attempt_id, reason_code, error.message
            )
            .as_bytes(),
        );
        let mut connection = self.open_connection()?;
        let existing: Option<(String, String, String)> = connection
            .query_row(
                "SELECT status,reason_code,evidence_digest FROM attempt_conditions WHERE attempt_id=?1 AND condition_type='recovery_required'",
                [&attempt.attempt_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|error| RuntimeError::from_sql(error, "cannot inspect recovery condition"))?;
        if existing.as_ref().is_some_and(|(status, reason, evidence)| {
            status == "true" && reason == reason_code && evidence == &evidence_digest
        }) {
            return Ok(());
        }

        let transaction = immediate(&mut connection, "reconciliation failure transaction")?;
        let current = load_attempt(&transaction, &attempt.attempt_id)?;
        upsert_condition(
            &transaction,
            &current.attempt_id,
            &ConditionUpdate {
                condition_type: "recovery_required".to_string(),
                status: "true".to_string(),
                reason_code: reason_code.to_string(),
                evidence_digest,
                observed_at_ms,
            },
        )?;
        append_event(
            &transaction,
            &current.job_id,
            Some(&current.attempt_id),
            "RECONCILIATION_FAILED",
            "SYSTEM_OBSERVED",
            Some(current.state),
            None,
            reason_code,
            serde_json::json!({
                "message": error.message,
                "field": error.field,
                "retryable": error.retryable,
            }),
            observed_at_ms,
        )?;
        transaction
            .commit()
            .map_err(|error| RuntimeError::from_sql(error, "cannot commit reconciliation failure"))
    }

    pub fn clear_reconciliation_failure(
        &self,
        attempt_id: &str,
        observed_at_ms: u64,
    ) -> RuntimeResult<()> {
        let mut connection = self.open_connection()?;
        let status: Option<String> = connection
            .query_row(
                "SELECT status FROM attempt_conditions WHERE attempt_id=?1 AND condition_type='recovery_required'",
                [attempt_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| RuntimeError::from_sql(error, "cannot inspect recovery condition"))?;
        if status.as_deref() != Some("true") {
            return Ok(());
        }

        let transaction = immediate(&mut connection, "reconciliation success transaction")?;
        let current = load_attempt(&transaction, attempt_id)?;
        let evidence_digest = sha256_bytes(
            format!("runtime-reconciliation-converged\0{attempt_id}\0{observed_at_ms}").as_bytes(),
        );
        let changed = transaction
            .execute(
                "UPDATE attempt_conditions SET status='false',reason_code='RECONCILIATION_CONVERGED',evidence_digest=?1,observed_at_ms=?2 WHERE attempt_id=?3 AND condition_type='recovery_required' AND status='true'",
                params![evidence_digest, observed_at_ms, attempt_id],
            )
            .map_err(|error| RuntimeError::from_sql(error, "cannot clear recovery condition"))?;
        if changed > 0 {
            append_event(
                &transaction,
                &current.job_id,
                Some(&current.attempt_id),
                "RECONCILIATION_CONVERGED",
                "SYSTEM_OBSERVED",
                Some(current.state),
                Some(current.state),
                "RECONCILIATION_CONVERGED",
                serde_json::json!({}),
                observed_at_ms,
            )?;
        }
        transaction
            .commit()
            .map_err(|error| RuntimeError::from_sql(error, "cannot commit reconciliation success"))
    }

    pub fn active_reservation_count(&self) -> RuntimeResult<u32> {
        let connection = self.open_connection()?;
        connection
            .query_row(
                "SELECT COUNT(*) FROM concurrency_reservations WHERE state IN ('active','held_orphaned')",
                [],
                |row| row.get(0),
            )
            .map_err(|error| RuntimeError::from_sql(error, "cannot count active reservations"))
    }
}

fn validate_migration_checksum(
    connection: &Connection,
    version: i64,
    expected: &str,
    label: &str,
) -> RuntimeResult<()> {
    let checksum: String = connection
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version=?1",
            [version],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| RuntimeError::from_sql(error, "cannot read migration checksum"))?
        .ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                format!("required {label} is missing"),
                None,
                false,
            )
        })?;
    if checksum != expected {
        return Err(RuntimeError::new(
            RuntimeErrorCode::MigrationChecksumMismatch,
            format!("{label} checksum does not match the compiled migration"),
            None,
            false,
        ));
    }
    Ok(())
}

fn immediate<'a>(connection: &'a mut Connection, context: &str) -> RuntimeResult<Transaction<'a>> {
    connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| RuntimeError::from_sql(error, &format!("cannot begin {context}")))
}

struct RawJob {
    job_id: String,
    principal: String,
    client_request_id: String,
    request_digest: String,
    operation_digest: String,
    workspace_id: String,
    workspace_snapshot_json: String,
    execution_plan_json: String,
    execution_plan_digest: String,
    created_at_ms: u64,
    desired_state: String,
    resolution: Option<String>,
    current_attempt_id: Option<String>,
    row_version: u64,
}

impl RawJob {
    fn into_record(self) -> RuntimeResult<RuntimeJobRecord> {
        Ok(RuntimeJobRecord {
            job_id: self.job_id,
            principal: self.principal,
            client_request_id: self.client_request_id,
            request_digest: self.request_digest,
            operation_digest: self.operation_digest,
            workspace_id: self.workspace_id,
            workspace_snapshot_json: self.workspace_snapshot_json,
            execution_plan_json: self.execution_plan_json,
            execution_plan_digest: self.execution_plan_digest,
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
        execution_plan_json: row.get(7)?,
        execution_plan_digest: row.get(8)?,
        created_at_ms: row.get(9)?,
        desired_state: row.get(10)?,
        resolution: row.get(11)?,
        current_attempt_id: row.get(12)?,
        row_version: row.get(13)?,
    })
}

pub(crate) fn load_job(connection: &Connection, job_id: &str) -> RuntimeResult<RuntimeJobRecord> {
    connection
        .query_row(
            "SELECT job_id,principal,client_request_id,request_digest,operation_digest,workspace_id,workspace_snapshot_json,execution_plan_json,execution_plan_digest,created_at_ms,desired_state,resolution,current_attempt_id,row_version FROM jobs WHERE job_id=?1",
            [job_id],
            raw_job_from_row,
        )
        .optional()
        .map_err(|error| RuntimeError::from_sql(error, "cannot load Job"))?
        .ok_or_else(|| {
            RuntimeError::new(RuntimeErrorCode::JobNotFound, "Job not found", Some("jobId"), false)
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
    fn into_record(self) -> RuntimeResult<AttemptRecord> {
        Ok(AttemptRecord {
            attempt_id: self.attempt_id,
            job_id: self.job_id,
            attempt_number: self.attempt_number,
            state: AttemptState::parse(&self.state)?,
            termination_intent: AttemptTerminationIntent::parse(&self.termination_intent)?,
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

pub(crate) fn load_attempt(
    connection: &Connection,
    attempt_id: &str,
) -> RuntimeResult<AttemptRecord> {
    connection
        .query_row(
            "SELECT attempt_id,job_id,attempt_number,state,termination_intent,launch_token_digest,bundle_path,bundle_digest,boot_id,unit_name,invocation_id,control_group,main_pid,process_start_identity,runner_start_digest,result_digest,exit_code,infrastructure_error_digest,created_at_ms,started_at_ms,finished_at_ms,row_version FROM attempts WHERE attempt_id=?1",
            [attempt_id],
            raw_attempt_from_row,
        )
        .optional()
        .map_err(|error| RuntimeError::from_sql(error, "cannot load Attempt"))?
        .ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::AttemptNotFound,
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
    global_limit: u32,
    state: String,
    acquired_at_ms: u64,
    released_at_ms: Option<u64>,
    release_reason: Option<String>,
}

pub(crate) fn load_reservation(
    connection: &Connection,
    attempt_id: &str,
) -> RuntimeResult<ReservationRecord> {
    let raw = connection
        .query_row(
            "SELECT reservation_id,attempt_id,global_limit,state,acquired_at_ms,released_at_ms,release_reason FROM concurrency_reservations WHERE attempt_id=?1",
            [attempt_id],
            |row| {
                Ok(RawReservation {
                    reservation_id: row.get(0)?,
                    attempt_id: row.get(1)?,
                    global_limit: row.get(2)?,
                    state: row.get(3)?,
                    acquired_at_ms: row.get(4)?,
                    released_at_ms: row.get(5)?,
                    release_reason: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(|error| RuntimeError::from_sql(error, "cannot load reservation"))?
        .ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorCode::ReservationStateConflict,
                "Attempt has no reservation",
                Some("attemptId"),
                false,
            )
        })?;
    Ok(ReservationRecord {
        reservation_id: raw.reservation_id,
        attempt_id: raw.attempt_id,
        global_limit: raw.global_limit,
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
) -> RuntimeResult<()> {
    let sequence: u64 = transaction
        .query_row(
            "SELECT COALESCE(MAX(event_sequence),0)+1 FROM job_events WHERE job_id=?1",
            [job_id],
            |row| row.get(0),
        )
        .map_err(|error| RuntimeError::from_sql(error, "cannot allocate event sequence"))?;
    let detail_json = serde_json::to_string(&detail).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::RegistryUnavailable,
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
        .map_err(|error| RuntimeError::from_sql(error, "cannot append Job event"))?;
    Ok(())
}

fn upsert_condition(
    transaction: &Transaction<'_>,
    attempt_id: &str,
    condition: &ConditionUpdate,
) -> RuntimeResult<()> {
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
        .map_err(|error| RuntimeError::from_sql(error, "cannot update Attempt condition"))?;
    Ok(())
}

fn release_reservation(
    transaction: &Transaction<'_>,
    attempt_id: &str,
    released_at_ms: u64,
    reason: &str,
) -> RuntimeResult<()> {
    let changed = transaction
        .execute(
            "UPDATE concurrency_reservations SET state='released',released_at_ms=?1,release_reason=?2 WHERE attempt_id=?3 AND state IN ('active','held_orphaned')",
            params![released_at_ms, reason, attempt_id],
        )
        .map_err(|error| RuntimeError::from_sql(error, "cannot release reservation"))?;
    if changed == 0 {
        let current: String = transaction
            .query_row(
                "SELECT state FROM concurrency_reservations WHERE attempt_id=?1",
                [attempt_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| RuntimeError::from_sql(error, "cannot inspect reservation state"))?
            .ok_or_else(|| {
                RuntimeError::new(
                    RuntimeErrorCode::ReservationStateConflict,
                    "reservation is missing",
                    Some("attemptId"),
                    false,
                )
            })?;
        if current != "released" {
            return Err(RuntimeError::new(
                RuntimeErrorCode::ReservationStateConflict,
                format!("reservation cannot be released from {current}"),
                Some("attemptId"),
                false,
            ));
        }
    }
    upsert_condition(
        transaction,
        attempt_id,
        &ConditionUpdate {
            condition_type: "reservation_held".to_string(),
            status: "false".to_string(),
            reason_code: reason.to_string(),
            evidence_digest: sha256_bytes(
                format!("runtime-reservation-release\0{attempt_id}\0{released_at_ms}").as_bytes(),
            ),
            observed_at_ms: released_at_ms,
        },
    )?;
    Ok(())
}
fn hold_orphaned_reservation(
    transaction: &Transaction<'_>,
    attempt_id: &str,
    observed_at_ms: u64,
    reason: &str,
) -> RuntimeResult<()> {
    let changed = transaction
        .execute(
            "UPDATE concurrency_reservations SET state='held_orphaned',released_at_ms=NULL,release_reason=?1 WHERE attempt_id=?2 AND state IN ('active','held_orphaned')",
            params![reason, attempt_id],
        )
        .map_err(|error| RuntimeError::from_sql(error, "cannot hold orphaned reservation"))?;
    if changed != 1 {
        return Err(RuntimeError::new(
            RuntimeErrorCode::ReservationStateConflict,
            "orphaned Attempt has no active reservation",
            Some("attemptId"),
            false,
        ));
    }
    upsert_condition(
        transaction,
        attempt_id,
        &ConditionUpdate {
            condition_type: "reservation_held".to_string(),
            status: "held_orphaned".to_string(),
            reason_code: reason.to_string(),
            evidence_digest: sha256_bytes(
                format!("runtime-orphaned-reservation\0{attempt_id}\0{observed_at_ms}").as_bytes(),
            ),
            observed_at_ms,
        },
    )
}

fn repair_terminal_admin_transaction(
    transaction: &Transaction<'_>,
    request: &TerminalCommit,
    audit: &AdminRepairAudit,
) -> RuntimeResult<()> {
    validate_digest(&request.result_digest, "resultDigest")?;
    for artifact in &request.artifacts {
        validate_artifact_registration(artifact)?;
    }
    let attempt = load_attempt(transaction, &request.attempt_id)?;
    let job = load_job(transaction, &attempt.job_id)?;
    let reservation = load_reservation(transaction, &attempt.attempt_id)?;
    let runner_terminal = matches!(
        request.state,
        AttemptState::Succeeded
            | AttemptState::Failed
            | AttemptState::TimedOut
            | AttemptState::Cancelled
    );
    let allowed = (matches!(attempt.state, AttemptState::Lost | AttemptState::Orphaned)
        && runner_terminal)
        || (attempt.state == AttemptState::Lost && request.state == AttemptState::Lost);
    if !allowed {
        return Err(RuntimeError::new(
            RuntimeErrorCode::OrphanRemediationDenied,
            "administrative repair cannot rewrite this terminal state",
            Some("attemptId"),
            false,
        ));
    }
    let source_resolution = resolution_for_state(attempt.state)?;
    if attempt.row_version != request.expected_row_version
        || job.row_version != audit.expected_job_row_version
        || job.resolution != Some(source_resolution)
        || job.current_attempt_id != audit.expected_current_attempt_id
        || reservation.state != audit.expected_reservation_state
    {
        return Err(RuntimeError::new(
            RuntimeErrorCode::ReconciliationRequired,
            "Runtime state changed after the Doctor plan was created",
            Some("caseFingerprint"),
            false,
        ));
    }
    let changed = transaction
        .execute(
            "UPDATE attempts SET state=?1,result_digest=?2,exit_code=?3,infrastructure_error_digest=?4,finished_at_ms=?5,row_version=row_version+1 WHERE attempt_id=?6 AND row_version=?7 AND state=?8",
            params![
                request.state.as_db(),
                request.result_digest,
                request.exit_code,
                request.infrastructure_error_digest,
                request.finished_at_ms,
                request.attempt_id,
                request.expected_row_version,
                attempt.state.as_db(),
            ],
        )
        .map_err(|error| RuntimeError::from_sql(error, "cannot repair terminal Attempt"))?;
    if changed != 1 {
        return Err(state_conflict(
            "Attempt changed during administrative repair",
        ));
    }
    for artifact in &request.artifacts {
        transaction
            .execute(
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
            )
            .map_err(|error| {
                RuntimeError::new(
                    RuntimeErrorCode::ArtifactIdentityConflict,
                    format!(
                        "cannot register administrative repair Artifact {}: {error}",
                        artifact.artifact_id
                    ),
                    Some("artifacts"),
                    false,
                )
            })?;
    }
    upsert_condition(
        transaction,
        &attempt.attempt_id,
        &ConditionUpdate {
            condition_type: "result_available".to_string(),
            status: "true".to_string(),
            reason_code: request.reason_code.clone(),
            evidence_digest: request.result_digest.clone(),
            observed_at_ms: request.finished_at_ms,
        },
    )?;
    upsert_condition(
        transaction,
        &attempt.attempt_id,
        &ConditionUpdate {
            condition_type: "recovery_required".to_string(),
            status: "false".to_string(),
            reason_code: "ADMIN_REPAIR_COMPLETED".to_string(),
            evidence_digest: audit.case_fingerprint.clone(),
            observed_at_ms: audit.observed_at_ms,
        },
    )?;
    release_reservation(
        transaction,
        &attempt.attempt_id,
        audit.observed_at_ms,
        "ADMIN_RUNTIME_REPAIR",
    )?;
    let resolution = resolution_for_state(request.state)?;
    let job_changed = transaction
        .execute(
            "UPDATE jobs SET resolution=?1,current_attempt_id=NULL,row_version=row_version+1 WHERE job_id=?2 AND row_version=?3",
            params![resolution.as_db(), attempt.job_id, audit.expected_job_row_version],
        )
        .map_err(|error| RuntimeError::from_sql(error, "cannot repair Job terminal state"))?;
    if job_changed != 1 {
        return Err(state_conflict("Job changed during administrative repair"));
    }
    let detail = serde_json::json!({
        "action": audit.action,
        "principal": audit.principal,
        "reportFingerprint": audit.report_fingerprint,
        "caseFingerprint": audit.case_fingerprint,
        "snapshotPath": audit.snapshot_path,
        "snapshotDigest": audit.snapshot_digest,
        "resultDigest": request.result_digest,
        "exitCode": request.exit_code,
    });
    append_event(
        transaction,
        &attempt.job_id,
        Some(&attempt.attempt_id),
        "ADMIN_TERMINAL_REPAIR",
        "SYSTEM_OBSERVED",
        Some(attempt.state),
        Some(request.state),
        &request.reason_code,
        detail.clone(),
        audit.observed_at_ms,
    )?;
    append_event(
        transaction,
        &attempt.job_id,
        Some(&attempt.attempt_id),
        "JOB_RESOLUTION_ADMIN_CORRECTED",
        "SYSTEM_DERIVED",
        Some(attempt.state),
        Some(request.state),
        &request.reason_code,
        detail,
        audit.observed_at_ms,
    )?;
    Ok(())
}

fn repair_terminal_reservation_admin_transaction(
    transaction: &Transaction<'_>,
    attempt_id: &str,
    expected_attempt_row_version: u64,
    audit: &AdminRepairAudit,
) -> RuntimeResult<()> {
    let attempt = load_attempt(transaction, attempt_id)?;
    let job = load_job(transaction, &attempt.job_id)?;
    let reservation = load_reservation(transaction, attempt_id)?;
    if attempt.row_version != expected_attempt_row_version
        || job.row_version != audit.expected_job_row_version
        || job.current_attempt_id != audit.expected_current_attempt_id
        || reservation.state != audit.expected_reservation_state
    {
        return Err(RuntimeError::new(
            RuntimeErrorCode::ReconciliationRequired,
            "Runtime state changed after the Doctor plan was created",
            Some("caseFingerprint"),
            false,
        ));
    }
    let target = terminal_reservation_target(&attempt, &job)?;
    if target == reservation.state {
        return Ok(());
    }
    if target == ReservationState::HeldOrphaned {
        hold_orphaned_reservation(
            transaction,
            attempt_id,
            audit.observed_at_ms,
            "ADMIN_RUNTIME_REPAIR",
        )?;
    } else {
        release_reservation(
            transaction,
            attempt_id,
            audit.observed_at_ms,
            "ADMIN_RUNTIME_REPAIR",
        )?;
    }
    append_event(
        transaction,
        &attempt.job_id,
        Some(attempt_id),
        "ADMIN_RESERVATION_REPAIR",
        "SYSTEM_OBSERVED",
        Some(attempt.state),
        Some(attempt.state),
        "ADMIN_RUNTIME_REPAIR",
        serde_json::json!({
            "action": audit.action,
            "principal": audit.principal,
            "reportFingerprint": audit.report_fingerprint,
            "caseFingerprint": audit.case_fingerprint,
            "snapshotPath": audit.snapshot_path,
            "snapshotDigest": audit.snapshot_digest,
            "previousReservationState": reservation.state.as_db(),
            "newReservationState": target.as_db(),
        }),
        audit.observed_at_ms,
    )?;
    Ok(())
}

fn terminal_reservation_target(
    attempt: &AttemptRecord,
    job: &RuntimeJobRecord,
) -> RuntimeResult<ReservationState> {
    if !attempt.state.is_terminal() {
        return Err(RuntimeError::new(
            RuntimeErrorCode::ReconciliationRequired,
            "Attempt is not terminal while repairing terminal reservation",
            Some("attemptId"),
            false,
        ));
    }
    let expected_resolution = resolution_for_state(attempt.state)?;
    let evidence_complete = attempt.result_digest.is_some()
        && attempt.finished_at_ms.is_some()
        && job.resolution == Some(expected_resolution)
        && job.current_attempt_id.is_none();
    if !evidence_complete {
        return Err(RuntimeError::new(
            RuntimeErrorCode::ReconciliationRequired,
            "terminal Attempt lacks complete result, Job resolution, or current-Attempt evidence",
            Some("attemptId"),
            false,
        ));
    }
    if attempt.state == AttemptState::Orphaned {
        Ok(ReservationState::HeldOrphaned)
    } else {
        Ok(ReservationState::Released)
    }
}

fn resolution_for_state(state: AttemptState) -> RuntimeResult<JobResolution> {
    match state {
        AttemptState::Succeeded => Ok(JobResolution::Succeeded),
        AttemptState::Failed => Ok(JobResolution::Failed),
        AttemptState::TimedOut => Ok(JobResolution::TimedOut),
        AttemptState::Cancelled => Ok(JobResolution::Cancelled),
        AttemptState::Lost => Ok(JobResolution::Lost),
        AttemptState::Orphaned => Ok(JobResolution::Orphaned),
        _ => Err(RuntimeError::invalid(
            "Attempt state is not terminal",
            "state",
        )),
    }
}

pub(crate) fn inspect_runtime_invariants_connection(
    connection: &Connection,
) -> RuntimeResult<Vec<RuntimeInvariantViolation>> {
    let mut violations = Vec::new();
    let mut collect = |sql: &str, code: &str, detail: &str| -> RuntimeResult<()> {
        let mut statement = connection
            .prepare(sql)
            .map_err(|error| RuntimeError::from_sql(error, "cannot prepare invariant query"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            })
            .map_err(|error| RuntimeError::from_sql(error, "cannot query Runtime invariants"))?;
        for row in rows {
            let (job_id, attempt_id) =
                row.map_err(|error| RuntimeError::from_sql(error, "cannot decode invariant row"))?;
            violations.push(RuntimeInvariantViolation {
                code: code.to_string(),
                job_id,
                attempt_id,
                detail: detail.to_string(),
            });
        }
        Ok(())
    };
    collect(
        "SELECT job_id,NULL FROM jobs WHERE resolution IS NULL AND current_attempt_id IS NULL",
        "UNRESOLVED_JOB_WITHOUT_CURRENT_ATTEMPT",
        "unresolved Job must reference its current Attempt",
    )?;
    collect(
        "SELECT job_id,current_attempt_id FROM jobs WHERE resolution IS NOT NULL AND current_attempt_id IS NOT NULL",
        "RESOLVED_JOB_WITH_CURRENT_ATTEMPT",
        "resolved Job must not retain current_attempt_id",
    )?;
    collect(
        "SELECT j.job_id,a.attempt_id FROM jobs j JOIN attempts a ON a.attempt_id=j.current_attempt_id WHERE j.resolution IS NULL AND a.state IN ('succeeded','failed','timed_out','cancelled','lost','orphaned')",
        "UNRESOLVED_JOB_POINTS_TO_TERMINAL_ATTEMPT",
        "unresolved Job must not reference a terminal Attempt",
    )?;
    collect(
        "SELECT j.job_id,a.attempt_id FROM jobs j JOIN attempts a ON a.job_id=j.job_id WHERE j.resolution IS NOT NULL AND NOT EXISTS (SELECT 1 FROM attempts newer WHERE newer.job_id=a.job_id AND newer.attempt_number>a.attempt_number) AND ((j.resolution='succeeded' AND a.state!='succeeded') OR (j.resolution='failed' AND a.state!='failed') OR (j.resolution='timed_out' AND a.state!='timed_out') OR (j.resolution='cancelled' AND a.state!='cancelled') OR (j.resolution='lost' AND a.state!='lost') OR (j.resolution='orphaned' AND a.state!='orphaned'))",
        "JOB_RESOLUTION_ATTEMPT_STATE_MISMATCH",
        "resolved Job must agree with its latest terminal Attempt",
    )?;
    collect(
        "SELECT a.job_id,a.attempt_id FROM attempts a JOIN concurrency_reservations r ON r.attempt_id=a.attempt_id WHERE a.state IN ('succeeded','failed','timed_out','cancelled','lost') AND r.state!='released'",
        "TERMINAL_ATTEMPT_HOLDS_RESERVATION",
        "non-orphan terminal Attempt must release its reservation",
    )?;
    collect(
        "SELECT a.job_id,a.attempt_id FROM attempts a JOIN concurrency_reservations r ON r.attempt_id=a.attempt_id WHERE a.state='orphaned' AND r.state!='held_orphaned'",
        "ORPHAN_RESERVATION_NOT_HELD",
        "orphaned Attempt must hold its reservation",
    )?;
    collect(
        "SELECT job_id,attempt_id FROM attempts WHERE state IN ('succeeded','failed','timed_out','cancelled','lost','orphaned') AND (result_digest IS NULL OR finished_at_ms IS NULL)",
        "TERMINAL_ATTEMPT_MISSING_EVIDENCE",
        "terminal Attempt must retain result digest and finish time",
    )?;
    Ok(violations)
}

fn job_execution_reason_code(
    connection: &Connection,
    job_id: &str,
    resolved: bool,
) -> RuntimeResult<Option<String>> {
    if !resolved {
        return Ok(None);
    }
    connection
        .query_row(
            "SELECT reason_code FROM job_events WHERE job_id=?1 AND (event_type IN ('JOB_TERMINAL','JOB_RESOLUTION_CORRECTED','JOB_RESOLUTION_ADMIN_CORRECTED') OR (event_type='STOP_REQUESTED' AND new_state='cancelled')) ORDER BY event_sequence DESC LIMIT 1",
            [job_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| RuntimeError::from_sql(error, "cannot read Job execution reason"))
}

fn attempt_recovery_condition_active(
    connection: &Connection,
    attempt_id: &str,
) -> RuntimeResult<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM attempt_conditions WHERE attempt_id=?1 AND condition_type='recovery_required' AND status='true')",
            [attempt_id],
            |row| row.get(0),
        )
        .map_err(|error| RuntimeError::from_sql(error, "cannot inspect Attempt recovery condition"))
}

fn load_job_snapshot(connection: &Connection, job_id: &str) -> RuntimeResult<JobSnapshot> {
    let transaction = connection.unchecked_transaction().map_err(|error| {
        RuntimeError::from_sql(error, "cannot begin Job projection read snapshot")
    })?;
    let snapshot = load_job_snapshot_in(&transaction, job_id)?;
    transaction.commit().map_err(|error| {
        RuntimeError::from_sql(error, "cannot close Job projection read snapshot")
    })?;
    Ok(snapshot)
}

fn load_job_snapshot_in(connection: &Connection, job_id: &str) -> RuntimeResult<JobSnapshot> {
    let job = load_job(connection, job_id)?;
    let attempt = match job.current_attempt_id.as_deref() {
        Some(attempt_id) => Some(load_attempt(connection, attempt_id)?),
        None => {
            let attempt_id: Option<String> = connection
                .query_row(
                    "SELECT attempt_id FROM attempts WHERE job_id=?1 ORDER BY attempt_number DESC LIMIT 1",
                    [job_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| RuntimeError::from_sql(error, "cannot find latest Attempt"))?;
            attempt_id
                .map(|attempt_id| load_attempt(connection, &attempt_id))
                .transpose()?
        }
    };
    let recovery_condition_active = attempt
        .as_ref()
        .map(|attempt| attempt_recovery_condition_active(connection, &attempt.attempt_id))
        .transpose()?
        .unwrap_or(false);
    let artifacts_available: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM artifacts WHERE job_id=?1)",
            [&job.job_id],
            |row| row.get(0),
        )
        .map_err(|error| RuntimeError::from_sql(error, "cannot inspect Job Artifacts"))?;
    let execution_reason_code =
        job_execution_reason_code(connection, &job.job_id, job.resolution.is_some())?;
    let projection = project_job(
        &job,
        attempt.as_ref(),
        recovery_condition_active,
        artifacts_available,
        execution_reason_code,
    );
    Ok(JobSnapshot {
        job,
        attempt,
        projection,
    })
}

fn project_job(
    job: &RuntimeJobRecord,
    attempt: Option<&AttemptRecord>,
    recovery_condition_active: bool,
    artifacts_available: bool,
    execution_reason_code: Option<String>,
) -> JobProjection {
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
    let execution_terminal = job.resolution.is_some();
    let recovery_required = recovery_condition_active
        || attempt.is_some_and(|attempt| {
            matches!(
                attempt.state,
                AttemptState::Recovering | AttemptState::Orphaned
            )
        });
    let delivery_disposition = if recovery_required {
        RuntimeDeliveryDisposition::ReconciliationRequired
    } else {
        match job.resolution {
            None => RuntimeDeliveryDisposition::InProgress,
            Some(JobResolution::Orphaned) => RuntimeDeliveryDisposition::ReconciliationRequired,
            Some(JobResolution::Lost) => RuntimeDeliveryDisposition::Unknown,
            Some(_) => RuntimeDeliveryDisposition::Committed,
        }
    };
    let poll_after_ms = matches!(
        delivery_disposition,
        RuntimeDeliveryDisposition::InProgress | RuntimeDeliveryDisposition::ReconciliationRequired
    )
    .then_some(250);
    JobProjection {
        job_id: job.job_id.clone(),
        status,
        desired_state: job.desired_state,
        attempt_id: attempt.map(|attempt| attempt.attempt_id.clone()),
        attempt_state: attempt.map(|attempt| attempt.state),
        termination_intent: attempt.map(|attempt| attempt.termination_intent),
        exit_code: attempt.and_then(|attempt| attempt.exit_code),
        execution_terminal,
        execution_disposition: job.resolution,
        execution_reason_code,
        delivery_disposition,
        recovery_required,
        semantic_completion_evaluated: false,
        result_available: job.resolution.is_some(),
        artifacts_available,
        artifacts: Vec::new(),
        poll_after_ms,
    }
}

fn job_request_identity_digest(job: &RuntimeJobRecord) -> RuntimeResult<String> {
    if job
        .request_digest
        .starts_with(super::REQUEST_IDENTITY_PREFIX)
        || job
            .request_digest
            .starts_with(super::PROPOSAL_IDENTITY_PREFIX)
        || job
            .request_digest
            .starts_with(super::INPUT_BOUND_IDENTITY_PREFIX)
    {
        validate_request_identity_digest(&job.request_digest)?;
        return Ok(job.request_digest.clone());
    }
    let plan: RuntimeExecutionPlan =
        serde_json::from_str(&job.execution_plan_json).map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryCorrupt,
                format!("stored execution plan is invalid: {error}"),
                Some("executionPlan"),
                false,
            )
        })?;
    operation_request_identity_digest_from_plan(&plan)
}

fn validate_request_identity_digest(value: &str) -> RuntimeResult<()> {
    let digest = value
        .strip_prefix(super::REQUEST_IDENTITY_PREFIX)
        .or_else(|| value.strip_prefix(super::PROPOSAL_IDENTITY_PREFIX))
        .or_else(|| value.strip_prefix(super::INPUT_BOUND_IDENTITY_PREFIX))
        .ok_or_else(|| {
            RuntimeError::invalid(
                "unsupported request identity digest",
                "requestIdentityDigest",
            )
        })?;
    validate_digest(digest, "requestIdentityDigest")
}

fn idempotency_conflict() -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::IdempotencyConflict,
        "clientRequestId is already bound to a different operation request",
        Some("clientRequestId"),
        false,
    )
}

fn validate_submit(request: &SubmitRequest) -> RuntimeResult<()> {
    if request.schema_version != RUNTIME_SCHEMA_VERSION
        || request.plan.schema_version != RUNTIME_SCHEMA_VERSION
    {
        return Err(RuntimeError::invalid(
            "unsupported runtime schema version",
            "schemaVersion",
        ));
    }
    validate_identifier(&request.client_request_id, "clientRequestId")?;
    if let Some(digest) = request.request_identity_digest.as_deref() {
        validate_request_identity_digest(digest)?;
    }
    validate_identifier(&request.plan.principal, "plan.principal")?;
    validate_identifier(&request.plan.workspace_id, "plan.workspaceId")?;
    if request.global_limit == 0 {
        return Err(RuntimeError::invalid(
            "globalLimit must be positive",
            "globalLimit",
        ));
    }

    for (path, field) in [
        (&request.plan.workspace_path, "plan.workspacePath"),
        (&request.plan.executable, "plan.executable"),
        (&request.plan.cwd, "plan.cwd"),
    ] {
        if !Path::new(path).is_absolute() || path.as_bytes().contains(&0) {
            return Err(RuntimeError::invalid(
                format!("{field} must be an absolute NUL-free path"),
                field,
            ));
        }
    }
    if !Path::new(&request.plan.cwd).starts_with(&request.plan.workspace_path) {
        return Err(RuntimeError::invalid(
            "plan.cwd must remain inside workspacePath",
            "plan.cwd",
        ));
    }
    validate_digest(&request.plan.executable_digest, "plan.executableDigest")?;
    if let Some(digest) = request.plan.workspace_source_digest.as_deref() {
        validate_digest(digest, "plan.workspaceSourceDigest")?;
    }
    match (
        request.plan.input_set_id.as_deref(),
        request.plan.effective_inputs.is_empty(),
    ) {
        (None, true) => {}
        (Some(input_set_id), false) => {
            if input_set_id.len() != 64
                || !input_set_id.bytes().all(|byte| byte.is_ascii_hexdigit())
                || input_set_id.bytes().any(|byte| byte.is_ascii_uppercase())
            {
                return Err(RuntimeError::invalid(
                    "plan.inputSetId must be lowercase 32-byte SHA-256 hex",
                    "plan.inputSetId",
                ));
            }
            if request.plan.execution_profile != super::ExecutionProfile::ContainedLocal {
                return Err(RuntimeError::invalid(
                    "effective inputs require contained_local execution",
                    "plan.executionProfile",
                ));
            }
        }
        (Some(_), true) => {
            return Err(RuntimeError::invalid(
                "plan.inputSetId requires effectiveInputs",
                "plan.inputSetId",
            ));
        }
        (None, false) => {
            return Err(RuntimeError::invalid(
                "effectiveInputs require plan.inputSetId",
                "plan.inputSetId",
            ));
        }
    }
    let mut input_paths = std::collections::BTreeSet::<PathBuf>::new();
    for (index, input) in request.plan.effective_inputs.iter().enumerate() {
        validate_identifier(
            &input.authority,
            &format!("plan.effectiveInputs[{index}].authority"),
        )?;
        validate_digest(
            &input.digest,
            &format!("plan.effectiveInputs[{index}].digest"),
        )?;
        let object = Path::new(&input.relative_object);
        let presentation = Path::new(&input.presentation_relative_path);
        for (path, field) in [
            (
                object,
                format!("plan.effectiveInputs[{index}].relativeObject"),
            ),
            (
                presentation,
                format!("plan.effectiveInputs[{index}].presentationRelativePath"),
            ),
        ] {
            if path.is_absolute()
                || path.as_os_str().is_empty()
                || path
                    .components()
                    .any(|component| !matches!(component, std::path::Component::Normal(_)))
            {
                return Err(RuntimeError::invalid(
                    "effective input paths must contain only normal relative components",
                    &field,
                ));
            }
        }
        for existing in &input_paths {
            if presentation == existing
                || presentation.starts_with(existing)
                || existing.starts_with(presentation)
            {
                return Err(RuntimeError::invalid(
                    "effective input presentation paths must not overlap",
                    &format!("plan.effectiveInputs[{index}].presentationRelativePath"),
                ));
            }
        }
        input_paths.insert(presentation.to_path_buf());
    }

    if request.plan.source_revision.is_empty() || request.plan.source_revision.len() > 256 {
        return Err(RuntimeError::invalid(
            "sourceRevision must be non-empty and bounded",
            "plan.sourceRevision",
        ));
    }
    if request.plan.timeout_ms == 0
        || request.plan.stdout_limit_bytes == 0
        || request.plan.stderr_limit_bytes == 0
    {
        return Err(RuntimeError::invalid(
            "runtime and output limits must be positive",
            "plan",
        ));
    }
    validate_plan_budget(&request.plan.budget)?;
    validate_exec_payload_for_plan(&request.plan.args, &request.plan.env, "plan")?;
    for (index, step) in request.plan.steps.iter().enumerate() {
        validate_exec_payload_for_plan(&step.args, &step.env, &format!("plan.steps[{index}]"))?;
    }
    Ok(())
}

fn validate_exec_payload_for_plan(
    args: &[String],
    env: &std::collections::BTreeMap<String, String>,
    field: &str,
) -> RuntimeResult<()> {
    crate::universal::validate_exec_payload(args, env, field).map_err(|error| {
        let error_field = error.field.unwrap_or_else(|| field.to_string());
        RuntimeError::invalid(error.message, &error_field)
    })
}

fn validate_plan_budget(budget: &super::ExecutionBudget) -> RuntimeResult<()> {
    if budget.memory_max_bytes == Some(0) {
        return Err(RuntimeError::invalid(
            "plan memoryMaxBytes must be positive",
            "plan.budget.memoryMaxBytes",
        ));
    }
    if budget.tasks_max == Some(0) {
        return Err(RuntimeError::invalid(
            "plan tasksMax must be positive",
            "plan.budget.tasksMax",
        ));
    }
    if budget.cpu_quota_percent == Some(0) {
        return Err(RuntimeError::invalid(
            "plan cpuQuotaPercent must be positive",
            "plan.budget.cpuQuotaPercent",
        ));
    }
    Ok(())
}

fn validate_runner_identity(identity: &RunnerIdentity) -> RuntimeResult<()> {
    for (value, field) in [
        (&identity.boot_id, "bootId"),
        (&identity.unit_name, "unitName"),
        (&identity.invocation_id, "invocationId"),
        (&identity.process_start_identity, "processStartIdentity"),
    ] {
        validate_identifier(value, field)?;
    }
    if !identity.unit_name.ends_with(".service") || identity.main_pid == 0 {
        return Err(RuntimeError::invalid(
            "invalid Runner unit or PID",
            "runnerIdentity",
        ));
    }
    if !Path::new(&identity.control_group).is_absolute() {
        return Err(RuntimeError::invalid(
            "controlGroup must be absolute",
            "controlGroup",
        ));
    }
    validate_digest(&identity.runner_start_digest, "runnerStartDigest")
}

fn validate_artifact_registration(artifact: &ArtifactRegistration) -> RuntimeResult<()> {
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
        return Err(RuntimeError::invalid(
            "Artifact path must be a bounded relative path",
            "artifact.relativePath",
        ));
    }
    if artifact.media_type.is_empty() || artifact.media_type.len() > 256 {
        return Err(RuntimeError::invalid(
            "Artifact mediaType must be non-empty and bounded",
            "artifact.mediaType",
        ));
    }
    Ok(())
}

fn validate_identifier(value: &str, field: &str) -> RuntimeResult<()> {
    if value.trim().is_empty()
        || value.len() > 256
        || value.as_bytes().contains(&0)
        || value.chars().any(char::is_control)
    {
        return Err(RuntimeError::invalid(
            format!("{field} must be non-empty, bounded, and control-free"),
            field,
        ));
    }
    Ok(())
}

fn validate_digest(value: &str, field: &str) -> RuntimeResult<()> {
    let valid = value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()));
    if !valid {
        return Err(RuntimeError::invalid(
            format!("{field} must be a SHA-256 digest"),
            field,
        ));
    }
    Ok(())
}

fn state_conflict(message: impl Into<String>) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorCode::AttemptStateConflict,
        message,
        Some("attemptId"),
        false,
    )
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn now_ms() -> RuntimeResult<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryUnavailable,
                format!("system clock precedes Unix epoch: {error}"),
                None,
                false,
            )
        })?
        .as_millis()
        .try_into()
        .map_err(|_| {
            RuntimeError::new(
                RuntimeErrorCode::RegistryUnavailable,
                "current time does not fit u64 milliseconds",
                None,
                false,
            )
        })
}

fn create_private_directory(path: &Path) -> RuntimeResult<()> {
    fs::create_dir_all(path).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::IoError,
            format!("cannot create {}: {error}", path.display()),
            Some("storeRoot"),
            false,
        )
    })?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::IoError,
            format!("cannot protect {}: {error}", path.display()),
            Some("storeRoot"),
            false,
        )
    })
}

fn set_private_file(path: &Path) -> RuntimeResult<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorCode::IoError,
            format!("cannot protect {}: {error}", path.display()),
            Some("dbPath"),
            false,
        )
    })
}
