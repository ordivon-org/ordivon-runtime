use rusqlite::backup::Backup;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

use crate::universal::{sha256_bytes, sha256_file};
use crate::{
    M6Error, M6ErrorCode, M6Registry, M6Result, M7RuntimeHardeningConfig, M7_SCHEMA_VERSION,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct M7LifecyclePolicy {
    pub schema_version: u32,
    pub retention_ms: u64,
    pub max_retained_artifact_bytes: u64,
    pub max_single_job_artifact_bytes: u64,
    pub max_gc_items: u32,
}

impl M7LifecyclePolicy {
    pub fn validate(&self) -> M6Result<()> {
        if self.schema_version != M7_SCHEMA_VERSION {
            return Err(M6Error::invalid(
                "unsupported M7 lifecycle schema",
                "schemaVersion",
            ));
        }
        if self.retention_ms == 0
            || self.max_retained_artifact_bytes == 0
            || self.max_single_job_artifact_bytes == 0
            || self.max_single_job_artifact_bytes > self.max_retained_artifact_bytes
            || self.max_gc_items == 0
            || self.max_gc_items > 10_000
        {
            return Err(M6Error::invalid(
                "invalid M7 lifecycle policy bounds",
                "policy",
            ));
        }
        Ok(())
    }

    pub fn digest(&self) -> M6Result<String> {
        let bytes = serde_json::to_vec(self).map_err(serialization_error)?;
        Ok(sha256_bytes(&bytes))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct M7AdmissionQuota {
    pub policy_digest: String,
    pub estimated_artifact_bytes: u64,
    pub max_retained_artifact_bytes: u64,
    pub max_single_job_artifact_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct M7LifecycleUsage {
    pub retained_artifact_bytes: u64,
    pub artifact_count: u64,
    pub active_holds: u64,
    pub active_reservations: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct M7GcPlan {
    pub plan_id: String,
    pub policy_digest: String,
    pub plan_digest: String,
    pub item_count: u32,
    pub byte_length: u64,
    pub attempt_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct M7BackupResult {
    pub backup_id: String,
    pub backup_path: String,
    pub registry_digest: String,
    pub manifest_digest: String,
    pub file_count: u64,
    pub byte_length: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct M7RestoreResult {
    pub backup_id: String,
    pub restored_control_root: String,
    pub registry_digest: String,
    pub file_count: u64,
}

#[derive(Clone)]
pub struct M7LifecycleManager {
    registry: M6Registry,
    hardening: M7RuntimeHardeningConfig,
    policy: M7LifecyclePolicy,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BackupManifest {
    schema_version: u32,
    backup_id: String,
    source_control_root: String,
    registry_digest: String,
    files: Vec<BackupFile>,
    created_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BackupFile {
    relative_path: String,
    digest: String,
    byte_length: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GcCandidate {
    attempt_id: String,
    job_id: String,
    bundle_path: String,
    bundle_digest: String,
    byte_length: u64,
}

impl M7LifecycleManager {
    pub fn new(
        registry: M6Registry,
        hardening: M7RuntimeHardeningConfig,
        policy: M7LifecyclePolicy,
    ) -> M6Result<Self> {
        hardening.validate_layout()?;
        policy.validate()?;
        Ok(Self {
            registry,
            hardening,
            policy,
        })
    }

    pub fn policy(&self) -> &M7LifecyclePolicy {
        &self.policy
    }

    pub fn usage(&self) -> M6Result<M7LifecycleUsage> {
        let connection = self.registry.open_connection()?;
        let (bytes, count): (u64, u64) = connection
            .query_row(
                "SELECT COALESCE(SUM(byte_length),0),COUNT(*) FROM artifacts",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|error| M6Error::from_sql(error, "cannot calculate Artifact usage"))?;
        let holds = connection
            .query_row(
                "SELECT COUNT(*) FROM m7_investigation_holds WHERE released_at_ms IS NULL",
                [],
                |row| row.get(0),
            )
            .map_err(|error| M6Error::from_sql(error, "cannot count investigation holds"))?;
        let reservations = connection
            .query_row(
                "SELECT COUNT(*) FROM concurrency_reservations WHERE state IN ('active','held_orphaned')",
                [],
                |row| row.get(0),
            )
            .map_err(|error| M6Error::from_sql(error, "cannot count lifecycle reservations"))?;
        Ok(M7LifecycleUsage {
            retained_artifact_bytes: bytes,
            artifact_count: count,
            active_holds: holds,
            active_reservations: reservations,
        })
    }

    pub fn enforce_admission_quota(
        &self,
        estimated_artifact_bytes: u64,
        observed_at_ms: u64,
    ) -> M6Result<()> {
        let mut connection = self.registry.open_connection()?;
        let transaction = immediate(&mut connection, "M7 quota evaluation")?;
        let retained: u64 = transaction
            .query_row(
                "SELECT COALESCE(SUM(byte_length),0) FROM artifacts",
                [],
                |row| row.get(0),
            )
            .map_err(|error| M6Error::from_sql(error, "cannot read retained Artifact bytes"))?;
        let denied = estimated_artifact_bytes > self.policy.max_single_job_artifact_bytes
            || retained.saturating_add(estimated_artifact_bytes)
                > self.policy.max_retained_artifact_bytes;
        if denied {
            append_lifecycle_event(
                &transaction,
                "ADMISSION_QUOTA_REJECTED",
                None,
                None,
                None,
                serde_json::json!({
                    "retainedBytes": retained,
                    "estimatedBytes": estimated_artifact_bytes,
                    "maxRetainedBytes": self.policy.max_retained_artifact_bytes,
                    "maxSingleJobBytes": self.policy.max_single_job_artifact_bytes,
                }),
                observed_at_ms,
            )?;
            transaction
                .commit()
                .map_err(|error| M6Error::from_sql(error, "cannot commit quota rejection"))?;
            return Err(M6Error::new(
                M6ErrorCode::LifecycleQuotaExceeded,
                "M7 lifecycle quota rejected the Job before admission",
                Some("execution"),
                false,
            ));
        }
        transaction
            .commit()
            .map_err(|error| M6Error::from_sql(error, "cannot commit quota evaluation"))?;
        Ok(())
    }

    pub fn place_hold(
        &self,
        job_id: &str,
        operator_id: &str,
        reason: &str,
        observed_at_ms: u64,
    ) -> M6Result<String> {
        validate_operator(operator_id)?;
        if reason.trim().is_empty() || reason.len() > 4096 {
            return Err(M6Error::invalid("hold reason must be bounded", "reason"));
        }
        self.registry.get_job(job_id)?;
        let hold_id = format!("hold-{}", Uuid::now_v7());
        let mut connection = self.registry.open_connection()?;
        let transaction = immediate(&mut connection, "M7 investigation hold")?;
        transaction
            .execute(
                "INSERT INTO m7_investigation_holds(hold_id,job_id,operator_id,reason_digest,created_at_ms,released_at_ms) VALUES(?1,?2,?3,?4,?5,NULL)",
                params![hold_id, job_id, operator_id, sha256_bytes(reason.as_bytes()), observed_at_ms],
            )
            .map_err(|error| M6Error::from_sql(error, "cannot place investigation hold"))?;
        append_lifecycle_event(
            &transaction,
            "INVESTIGATION_HOLD_PLACED",
            Some(operator_id),
            Some(job_id),
            None,
            serde_json::json!({"holdId": hold_id}),
            observed_at_ms,
        )?;
        transaction
            .commit()
            .map_err(|error| M6Error::from_sql(error, "cannot commit investigation hold"))?;
        Ok(hold_id)
    }

    pub fn release_hold(
        &self,
        hold_id: &str,
        operator_id: &str,
        observed_at_ms: u64,
    ) -> M6Result<()> {
        validate_operator(operator_id)?;
        let mut connection = self.registry.open_connection()?;
        let transaction = immediate(&mut connection, "release M7 investigation hold")?;
        let job_id: String = transaction
            .query_row(
                "SELECT job_id FROM m7_investigation_holds WHERE hold_id=?1 AND released_at_ms IS NULL",
                [hold_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| M6Error::from_sql(error, "cannot load investigation hold"))?
            .ok_or_else(|| M6Error::new(
                M6ErrorCode::InvestigationHoldActive,
                "active investigation hold was not found",
                Some("holdId"),
                false,
            ))?;
        transaction
            .execute(
                "UPDATE m7_investigation_holds SET released_at_ms=?1 WHERE hold_id=?2 AND released_at_ms IS NULL",
                params![observed_at_ms, hold_id],
            )
            .map_err(|error| M6Error::from_sql(error, "cannot release investigation hold"))?;
        append_lifecycle_event(
            &transaction,
            "INVESTIGATION_HOLD_RELEASED",
            Some(operator_id),
            Some(&job_id),
            None,
            serde_json::json!({"holdId": hold_id}),
            observed_at_ms,
        )?;
        transaction
            .commit()
            .map_err(|error| M6Error::from_sql(error, "cannot commit hold release"))
    }

    pub fn plan_gc(&self, observed_at_ms: u64) -> M6Result<M7GcPlan> {
        let cutoff = observed_at_ms.saturating_sub(self.policy.retention_ms);
        let connection = self.registry.open_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT a.attempt_id,a.job_id,a.bundle_path,a.bundle_digest,COALESCE(SUM(ar.byte_length),0) \
                 FROM attempts a \
                 JOIN concurrency_reservations r ON r.attempt_id=a.attempt_id \
                 LEFT JOIN artifacts ar ON ar.attempt_id=a.attempt_id \
                 WHERE a.state IN ('succeeded','failed','timed_out','cancelled','lost') \
                   AND a.finished_at_ms IS NOT NULL AND a.finished_at_ms<=?1 \
                   AND a.bundle_digest IS NOT NULL AND r.state='released' \
                   AND NOT EXISTS(SELECT 1 FROM m7_investigation_holds h WHERE h.job_id=a.job_id AND h.released_at_ms IS NULL) \
                 GROUP BY a.attempt_id,a.job_id,a.bundle_path,a.bundle_digest,a.finished_at_ms \
                 ORDER BY a.finished_at_ms,a.attempt_id LIMIT ?2",
            )
            .map_err(|error| M6Error::from_sql(error, "cannot prepare GC candidate scan"))?;
        let candidates = statement
            .query_map(params![cutoff, self.policy.max_gc_items], |row| {
                Ok(GcCandidate {
                    attempt_id: row.get(0)?,
                    job_id: row.get(1)?,
                    bundle_path: row.get(2)?,
                    bundle_digest: row.get(3)?,
                    byte_length: row.get(4)?,
                })
            })
            .map_err(|error| M6Error::from_sql(error, "cannot scan GC candidates"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| M6Error::from_sql(error, "cannot decode GC candidate"))?;
        drop(statement);
        drop(connection);

        let policy_digest = self.policy.digest()?;
        let candidate_bytes = serde_json::to_vec(&candidates).map_err(serialization_error)?;
        let plan_digest = sha256_bytes(&candidate_bytes);
        let plan_id = format!("gc-plan-{}", Uuid::now_v7());
        let byte_length = candidates.iter().map(|item| item.byte_length).sum();
        let mut connection = self.registry.open_connection()?;
        let transaction = immediate(&mut connection, "create M7 GC plan")?;
        transaction
            .execute(
                "INSERT INTO m7_gc_plans(plan_id,policy_digest,plan_digest,state,item_count,byte_length,created_at_ms,finished_at_ms) VALUES(?1,?2,?3,'planned',?4,?5,?6,NULL)",
                params![plan_id, policy_digest, plan_digest, candidates.len() as u64, byte_length, observed_at_ms],
            )
            .map_err(|error| M6Error::from_sql(error, "cannot insert GC plan"))?;
        for candidate in &candidates {
            transaction
                .execute(
                    "INSERT INTO m7_gc_items(plan_id,attempt_id,bundle_path,bundle_digest,byte_length,state) VALUES(?1,?2,?3,?4,?5,'planned')",
                    params![plan_id, candidate.attempt_id, candidate.bundle_path, candidate.bundle_digest, candidate.byte_length],
                )
                .map_err(|error| M6Error::from_sql(error, "cannot insert GC plan item"))?;
        }
        append_lifecycle_event(
            &transaction,
            "GC_PLAN_CREATED",
            None,
            None,
            None,
            serde_json::json!({
                "planId": plan_id,
                "planDigest": plan_digest,
                "itemCount": candidates.len(),
                "byteLength": byte_length,
            }),
            observed_at_ms,
        )?;
        transaction
            .commit()
            .map_err(|error| M6Error::from_sql(error, "cannot commit GC plan"))?;
        Ok(M7GcPlan {
            plan_id,
            policy_digest,
            plan_digest,
            item_count: candidates.len() as u32,
            byte_length,
            attempt_ids: candidates.into_iter().map(|item| item.attempt_id).collect(),
        })
    }

    pub fn execute_gc(
        &self,
        plan_id: &str,
        operator_id: &str,
        observed_at_ms: u64,
    ) -> M6Result<M7GcPlan> {
        validate_operator(operator_id)?;
        let items = self.load_gc_items(plan_id)?;
        let staging_root = self.hardening.control_root.join("gc-staging").join(plan_id);
        fs::create_dir_all(&staging_root)
            .map_err(|error| io_error("create GC staging root", error))?;
        fs::set_permissions(&staging_root, fs::Permissions::from_mode(0o700))
            .map_err(|error| io_error("protect GC staging root", error))?;

        for item in &items {
            self.verify_gc_candidate(item)?;
            let source = PathBuf::from(&item.bundle_path);
            let staged = staging_root.join(&item.attempt_id);
            if source.exists() {
                ensure_directory_nofollow(&source, &self.hardening.control_root)?;
                if staged.exists() {
                    fs::remove_dir_all(&staged)
                        .map_err(|error| io_error("remove stale GC staging item", error))?;
                }
                fs::rename(&source, &staged).map_err(|error| io_error("stage GC bundle", error))?;
            } else if !staged.exists() {
                return Err(M6Error::new(
                    M6ErrorCode::LifecyclePlanConflict,
                    "GC candidate is missing from both source and staging",
                    Some("planId"),
                    false,
                ));
            }
        }

        let mut connection = self.registry.open_connection()?;
        let transaction = immediate(&mut connection, "execute M7 GC plan")?;
        let state: String = transaction
            .query_row(
                "SELECT state FROM m7_gc_plans WHERE plan_id=?1",
                [plan_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| M6Error::from_sql(error, "cannot load GC plan state"))?
            .ok_or_else(|| {
                M6Error::new(
                    M6ErrorCode::LifecyclePlanConflict,
                    "GC plan was not found",
                    Some("planId"),
                    false,
                )
            })?;
        if state != "planned" && state != "executing" {
            return Err(M6Error::new(
                M6ErrorCode::LifecyclePlanConflict,
                format!("GC plan is not executable from state {state}"),
                Some("planId"),
                false,
            ));
        }
        transaction
            .execute(
                "UPDATE m7_gc_plans SET state='executing' WHERE plan_id=?1",
                [plan_id],
            )
            .map_err(|error| M6Error::from_sql(error, "cannot mark GC plan executing"))?;
        for item in &items {
            tombstone_artifacts(&transaction, plan_id, &item.attempt_id, observed_at_ms)?;
            transaction
                .execute(
                    "UPDATE m7_gc_items SET state='deleted' WHERE plan_id=?1 AND attempt_id=?2",
                    params![plan_id, item.attempt_id],
                )
                .map_err(|error| M6Error::from_sql(error, "cannot mark GC item deleted"))?;
        }
        transaction
            .execute(
                "UPDATE m7_gc_plans SET state='completed',finished_at_ms=?1 WHERE plan_id=?2",
                params![observed_at_ms, plan_id],
            )
            .map_err(|error| M6Error::from_sql(error, "cannot finish GC plan"))?;
        append_lifecycle_event(
            &transaction,
            "GC_PLAN_EXECUTED",
            Some(operator_id),
            None,
            None,
            serde_json::json!({"planId": plan_id, "itemCount": items.len()}),
            observed_at_ms,
        )?;
        transaction
            .commit()
            .map_err(|error| M6Error::from_sql(error, "cannot commit GC execution"))?;
        fs::remove_dir_all(&staging_root)
            .map_err(|error| io_error("remove GC staging root", error))?;
        self.gc_plan(plan_id)
    }

    fn load_gc_items(&self, plan_id: &str) -> M6Result<Vec<GcCandidate>> {
        let connection = self.registry.open_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT i.attempt_id,a.job_id,i.bundle_path,i.bundle_digest,i.byte_length \
                 FROM m7_gc_items i JOIN attempts a ON a.attempt_id=i.attempt_id \
                 WHERE i.plan_id=?1 ORDER BY i.attempt_id",
            )
            .map_err(|error| M6Error::from_sql(error, "cannot prepare GC item load"))?;
        let rows = statement
            .query_map([plan_id], |row| {
                Ok(GcCandidate {
                    attempt_id: row.get(0)?,
                    job_id: row.get(1)?,
                    bundle_path: row.get(2)?,
                    bundle_digest: row.get(3)?,
                    byte_length: row.get(4)?,
                })
            })
            .map_err(|error| M6Error::from_sql(error, "cannot load GC items"))?;
        let items = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| M6Error::from_sql(error, "cannot decode GC item"))?;
        Ok(items)
    }

    fn verify_gc_candidate(&self, candidate: &GcCandidate) -> M6Result<()> {
        let connection = self.registry.open_connection()?;
        let valid: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM attempts a \
                 JOIN concurrency_reservations r ON r.attempt_id=a.attempt_id \
                 WHERE a.attempt_id=?1 \
                   AND a.state IN ('succeeded','failed','timed_out','cancelled','lost') \
                   AND a.bundle_digest=?2 AND a.bundle_path=?3 AND r.state='released' \
                   AND NOT EXISTS(SELECT 1 FROM m7_investigation_holds h WHERE h.job_id=a.job_id AND h.released_at_ms IS NULL))",
                params![candidate.attempt_id, candidate.bundle_digest, candidate.bundle_path],
                |row| row.get(0),
            )
            .map_err(|error| M6Error::from_sql(error, "cannot revalidate GC candidate"))?;
        if !valid {
            return Err(M6Error::new(
                M6ErrorCode::LifecyclePlanConflict,
                "GC candidate changed after planning",
                Some("planId"),
                false,
            ));
        }
        Ok(())
    }

    fn gc_plan(&self, plan_id: &str) -> M6Result<M7GcPlan> {
        let connection = self.registry.open_connection()?;
        let (policy_digest, plan_digest, item_count, byte_length): (String, String, u32, u64) =
            connection
                .query_row(
                    "SELECT policy_digest,plan_digest,item_count,byte_length FROM m7_gc_plans WHERE plan_id=?1",
                    [plan_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()
                .map_err(|error| M6Error::from_sql(error, "cannot load GC plan"))?
                .ok_or_else(|| M6Error::new(
                    M6ErrorCode::LifecyclePlanConflict,
                    "GC plan was not found",
                    Some("planId"),
                    false,
                ))?;
        let attempt_ids = self
            .load_gc_items(plan_id)?
            .into_iter()
            .map(|item| item.attempt_id)
            .collect();
        Ok(M7GcPlan {
            plan_id: plan_id.to_string(),
            policy_digest,
            plan_digest,
            item_count,
            byte_length,
            attempt_ids,
        })
    }

    pub fn checkpoint(&self, operator_id: &str, observed_at_ms: u64) -> M6Result<()> {
        validate_operator(operator_id)?;
        if self.registry.active_reservation_count()? != 0 {
            return Err(M6Error::new(
                M6ErrorCode::BackupBusy,
                "WAL checkpoint requires zero active or orphan-held reservations",
                None,
                true,
            ));
        }
        let connection = self.registry.open_connection()?;
        let _: (u32, u32, u32) = connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|error| M6Error::from_sql(error, "cannot checkpoint M7 Registry"))?;
        append_lifecycle_event_direct(
            &connection,
            "WAL_CHECKPOINTED",
            Some(operator_id),
            serde_json::json!({}),
            observed_at_ms,
        )
    }

    pub fn create_backup(
        &self,
        backup_path: &Path,
        operator_id: &str,
        observed_at_ms: u64,
    ) -> M6Result<M7BackupResult> {
        validate_operator(operator_id)?;
        validate_new_absolute_root(backup_path, "backupPath")?;
        self.checkpoint(operator_id, observed_at_ms)?;
        fs::create_dir_all(backup_path)
            .map_err(|error| io_error("create M7 backup root", error))?;
        fs::set_permissions(backup_path, fs::Permissions::from_mode(0o700))
            .map_err(|error| io_error("protect M7 backup root", error))?;
        let database_copy = backup_path.join("registry.sqlite3");
        let source = self.registry.open_connection()?;
        let mut destination = Connection::open(&database_copy)
            .map_err(|error| M6Error::from_sql(error, "cannot create backup Registry"))?;
        {
            let backup = Backup::new(&source, &mut destination)
                .map_err(|error| M6Error::from_sql(error, "cannot initialize SQLite backup"))?;
            backup
                .run_to_completion(100, Duration::from_millis(10), None)
                .map_err(|error| M6Error::from_sql(error, "cannot execute SQLite backup"))?;
        }
        drop(destination);
        let registry_digest = sha256_file(&database_copy).map_err(map_universal_error)?;
        let content_root = backup_path.join("content");
        fs::create_dir(&content_root)
            .map_err(|error| io_error("create backup content root", error))?;
        let mut files = Vec::new();
        collect_control_files(
            &self.hardening.control_root,
            &self.hardening.control_root,
            &content_root,
            &mut files,
        )?;
        files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        let backup_id = format!("backup-{}", Uuid::now_v7());
        let manifest = BackupManifest {
            schema_version: M7_SCHEMA_VERSION,
            backup_id: backup_id.clone(),
            source_control_root: self.hardening.control_root.to_string_lossy().into_owned(),
            registry_digest: registry_digest.clone(),
            files,
            created_at_ms: observed_at_ms,
        };
        let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(serialization_error)?;
        let manifest_path = backup_path.join("manifest.json");
        fs::write(&manifest_path, &manifest_bytes)
            .map_err(|error| io_error("write backup manifest", error))?;
        fs::set_permissions(&manifest_path, fs::Permissions::from_mode(0o400))
            .map_err(|error| io_error("protect backup manifest", error))?;
        let manifest_digest = sha256_bytes(&manifest_bytes);
        let byte_length = manifest
            .files
            .iter()
            .map(|file| file.byte_length)
            .sum::<u64>()
            + fs::metadata(&database_copy)
                .map_err(|error| io_error("inspect backup Registry", error))?
                .len();
        self.record_backup(
            &backup_id,
            backup_path,
            &registry_digest,
            &manifest_digest,
            operator_id,
            observed_at_ms,
        )?;
        Ok(M7BackupResult {
            backup_id,
            backup_path: backup_path.to_string_lossy().into_owned(),
            registry_digest,
            manifest_digest,
            file_count: manifest.files.len() as u64,
            byte_length,
        })
    }

    fn record_backup(
        &self,
        backup_id: &str,
        backup_path: &Path,
        registry_digest: &str,
        manifest_digest: &str,
        operator_id: &str,
        observed_at_ms: u64,
    ) -> M6Result<()> {
        let mut connection = self.registry.open_connection()?;
        let transaction = immediate(&mut connection, "record M7 backup")?;
        transaction
            .execute(
                "INSERT INTO m7_backups(backup_id,backup_path,registry_digest,manifest_digest,state,created_at_ms,restored_at_ms) VALUES(?1,?2,?3,?4,'verified',?5,NULL)",
                params![backup_id, backup_path.to_string_lossy(), registry_digest, manifest_digest, observed_at_ms],
            )
            .map_err(|error| M6Error::from_sql(error, "cannot record M7 backup"))?;
        append_lifecycle_event(
            &transaction,
            "BACKUP_VERIFIED",
            Some(operator_id),
            None,
            None,
            serde_json::json!({
                "backupId": backup_id,
                "registryDigest": registry_digest,
                "manifestDigest": manifest_digest,
            }),
            observed_at_ms,
        )?;
        transaction
            .commit()
            .map_err(|error| M6Error::from_sql(error, "cannot commit backup record"))
    }

    pub fn restore_backup(
        &self,
        backup_path: &Path,
        target_control_root: &Path,
        operator_id: &str,
        observed_at_ms: u64,
    ) -> M6Result<M7RestoreResult> {
        validate_operator(operator_id)?;
        validate_existing_absolute_directory(backup_path, "backupPath")?;
        validate_new_absolute_root(target_control_root, "targetControlRoot")?;
        let manifest_path = backup_path.join("manifest.json");
        let manifest_bytes =
            fs::read(&manifest_path).map_err(|error| io_error("read backup manifest", error))?;
        let manifest: BackupManifest =
            serde_json::from_slice(&manifest_bytes).map_err(|error| {
                M6Error::new(
                    M6ErrorCode::RestoreInvalid,
                    format!("invalid backup manifest: {error}"),
                    Some("backupPath"),
                    false,
                )
            })?;
        if manifest.schema_version != M7_SCHEMA_VERSION {
            return Err(M6Error::new(
                M6ErrorCode::RestoreInvalid,
                "backup manifest schema is unsupported",
                Some("backupPath"),
                false,
            ));
        }
        let database_copy = backup_path.join("registry.sqlite3");
        if sha256_file(&database_copy).map_err(map_universal_error)? != manifest.registry_digest {
            return Err(M6Error::new(
                M6ErrorCode::RestoreInvalid,
                "backup Registry digest mismatch",
                Some("backupPath"),
                false,
            ));
        }
        verify_backup_files(backup_path, &manifest.files)?;

        let parent = target_control_root.parent().ok_or_else(|| {
            M6Error::invalid("target control root has no parent", "targetControlRoot")
        })?;
        fs::create_dir_all(parent).map_err(|error| io_error("create restore parent", error))?;
        let name = target_control_root
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| M6Error::invalid("invalid target root name", "targetControlRoot"))?;
        let staging = parent.join(format!(".{name}.restore-{}", Uuid::now_v7()));
        fs::create_dir(&staging).map_err(|error| io_error("create restore staging", error))?;
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o700))
            .map_err(|error| io_error("protect restore staging", error))?;
        let restore_result =
            self.restore_into_staging(backup_path, &manifest, &staging, target_control_root);
        if let Err(error) = restore_result {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        fs::rename(&staging, target_control_root)
            .map_err(|error| io_error("select restored control root", error))?;
        let registry_path = target_control_root.join("registry/registry.sqlite3");
        let registry_digest = sha256_file(&registry_path).map_err(map_universal_error)?;
        self.mark_backup_restored(
            &manifest.backup_id,
            target_control_root,
            operator_id,
            observed_at_ms,
        )?;
        Ok(M7RestoreResult {
            backup_id: manifest.backup_id,
            restored_control_root: target_control_root.to_string_lossy().into_owned(),
            registry_digest,
            file_count: manifest.files.len() as u64,
        })
    }

    fn restore_into_staging(
        &self,
        backup_path: &Path,
        manifest: &BackupManifest,
        staging: &Path,
        final_root: &Path,
    ) -> M6Result<()> {
        let registry_dir = staging.join("registry");
        fs::create_dir_all(&registry_dir)
            .map_err(|error| io_error("create restored Registry directory", error))?;
        fs::copy(
            backup_path.join("registry.sqlite3"),
            registry_dir.join("registry.sqlite3"),
        )
        .map_err(|error| io_error("copy restored Registry", error))?;
        for file in &manifest.files {
            let relative = validate_relative(&file.relative_path)?;
            let source = backup_path.join("content").join(relative);
            let destination = staging.join(relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| io_error("create restored content parent", error))?;
            }
            fs::copy(&source, &destination)
                .map_err(|error| io_error("copy restored control content", error))?;
        }
        let database = registry_dir.join("registry.sqlite3");
        let mut connection = Connection::open(&database)
            .map_err(|error| M6Error::from_sql(error, "cannot open restored Registry"))?;
        connection
            .pragma_update(None, "foreign_keys", true)
            .map_err(|error| M6Error::from_sql(error, "cannot enable restored foreign keys"))?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| M6Error::from_sql(error, "cannot rebase restored paths"))?;
        let mut statement = transaction
            .prepare("SELECT attempt_id,bundle_path FROM attempts")
            .map_err(|error| M6Error::from_sql(error, "cannot scan restored bundle paths"))?;
        let paths = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| M6Error::from_sql(error, "cannot read restored bundle paths"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| M6Error::from_sql(error, "cannot decode restored bundle path"))?;
        drop(statement);
        let source_root = PathBuf::from(&manifest.source_control_root);
        for (attempt_id, bundle_path) in paths {
            let relative = Path::new(&bundle_path)
                .strip_prefix(&source_root)
                .map_err(|_| {
                    M6Error::new(
                        M6ErrorCode::RestoreInvalid,
                        format!("Attempt {attempt_id} bundle escaped backup control root"),
                        Some("backupPath"),
                        false,
                    )
                })?;
            let final_path = final_root.join(relative);
            transaction
                .execute(
                    "UPDATE attempts SET bundle_path=?1 WHERE attempt_id=?2",
                    params![final_path.to_string_lossy(), attempt_id],
                )
                .map_err(|error| M6Error::from_sql(error, "cannot rebase restored Attempt"))?;
        }
        transaction
            .commit()
            .map_err(|error| M6Error::from_sql(error, "cannot commit restored path rebasing"))?;
        validate_sqlite_database(&connection)?;
        Ok(())
    }

    fn mark_backup_restored(
        &self,
        backup_id: &str,
        target: &Path,
        operator_id: &str,
        observed_at_ms: u64,
    ) -> M6Result<()> {
        let mut connection = self.registry.open_connection()?;
        let transaction = immediate(&mut connection, "record M7 restore")?;
        transaction
            .execute(
                "UPDATE m7_backups SET state='restored',restored_at_ms=?1 WHERE backup_id=?2 AND state='verified'",
                params![observed_at_ms, backup_id],
            )
            .map_err(|error| M6Error::from_sql(error, "cannot mark backup restored"))?;
        append_lifecycle_event(
            &transaction,
            "BACKUP_RESTORED",
            Some(operator_id),
            None,
            None,
            serde_json::json!({
                "backupId": backup_id,
                "targetControlRoot": target,
            }),
            observed_at_ms,
        )?;
        transaction
            .commit()
            .map_err(|error| M6Error::from_sql(error, "cannot commit restore record"))
    }
}

fn immediate<'a>(connection: &'a mut Connection, context: &str) -> M6Result<Transaction<'a>> {
    connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| M6Error::from_sql(error, &format!("cannot begin {context}")))
}

fn append_lifecycle_event(
    transaction: &Transaction<'_>,
    event_type: &str,
    operator_id: Option<&str>,
    job_id: Option<&str>,
    attempt_id: Option<&str>,
    detail: serde_json::Value,
    observed_at_ms: u64,
) -> M6Result<()> {
    let detail_json = serde_json::to_string(&detail).map_err(serialization_error)?;
    transaction
        .execute(
            "INSERT INTO m7_lifecycle_events(event_id,event_type,operator_id,job_id,attempt_id,detail_json,detail_digest,observed_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                format!("lifecycle-event-{}", Uuid::now_v7()),
                event_type,
                operator_id,
                job_id,
                attempt_id,
                detail_json,
                sha256_bytes(detail_json.as_bytes()),
                observed_at_ms,
            ],
        )
        .map_err(|error| M6Error::from_sql(error, "cannot append M7 lifecycle event"))?;
    Ok(())
}

fn append_lifecycle_event_direct(
    connection: &Connection,
    event_type: &str,
    operator_id: Option<&str>,
    detail: serde_json::Value,
    observed_at_ms: u64,
) -> M6Result<()> {
    let detail_json = serde_json::to_string(&detail).map_err(serialization_error)?;
    connection
        .execute(
            "INSERT INTO m7_lifecycle_events(event_id,event_type,operator_id,job_id,attempt_id,detail_json,detail_digest,observed_at_ms) VALUES(?1,?2,?3,NULL,NULL,?4,?5,?6)",
            params![
                format!("lifecycle-event-{}", Uuid::now_v7()),
                event_type,
                operator_id,
                detail_json,
                sha256_bytes(detail_json.as_bytes()),
                observed_at_ms,
            ],
        )
        .map_err(|error| M6Error::from_sql(error, "cannot append direct lifecycle event"))?;
    Ok(())
}

fn tombstone_artifacts(
    transaction: &Transaction<'_>,
    plan_id: &str,
    attempt_id: &str,
    observed_at_ms: u64,
) -> M6Result<()> {
    let mut statement = transaction
        .prepare(
            "SELECT artifact_id,job_id,kind,digest,byte_length FROM artifacts WHERE attempt_id=?1",
        )
        .map_err(|error| M6Error::from_sql(error, "cannot prepare Artifact tombstones"))?;
    let artifacts = statement
        .query_map([attempt_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, u64>(4)?,
            ))
        })
        .map_err(|error| M6Error::from_sql(error, "cannot scan Artifact tombstones"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| M6Error::from_sql(error, "cannot decode Artifact tombstone"))?;
    drop(statement);
    for (artifact_id, job_id, kind, digest, byte_length) in artifacts {
        transaction
            .execute(
                "INSERT INTO m7_artifact_tombstones(artifact_id,job_id,attempt_id,kind,digest,byte_length,gc_plan_id,deleted_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![artifact_id, job_id, attempt_id, kind, digest, byte_length, plan_id, observed_at_ms],
            )
            .map_err(|error| M6Error::from_sql(error, "cannot insert Artifact tombstone"))?;
    }
    transaction
        .execute("DELETE FROM artifacts WHERE attempt_id=?1", [attempt_id])
        .map_err(|error| M6Error::from_sql(error, "cannot remove GC Artifact rows"))?;
    Ok(())
}

fn collect_control_files(
    root: &Path,
    current: &Path,
    destination_root: &Path,
    files: &mut Vec<BackupFile>,
) -> M6Result<()> {
    for entry in
        fs::read_dir(current).map_err(|error| io_error("scan control root for backup", error))?
    {
        let entry = entry.map_err(|error| io_error("read control backup entry", error))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| io_error("inspect control backup entry", error))?;
        if metadata.file_type().is_symlink() {
            return Err(M6Error::new(
                M6ErrorCode::RestoreInvalid,
                "control backup refuses symbolic links",
                Some("controlRoot"),
                false,
            ));
        }
        let relative = path.strip_prefix(root).map_err(|_| {
            M6Error::new(
                M6ErrorCode::RestoreInvalid,
                "control backup path escaped root",
                Some("controlRoot"),
                false,
            )
        })?;
        if relative.starts_with("gc-staging")
            || relative == Path::new("registry/registry.sqlite3")
            || relative == Path::new("registry/registry.sqlite3-wal")
            || relative == Path::new("registry/registry.sqlite3-shm")
        {
            continue;
        }
        if metadata.is_dir() {
            collect_control_files(root, &path, destination_root, files)?;
        } else if metadata.is_file() {
            let destination = destination_root.join(relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| io_error("create backup content parent", error))?;
            }
            fs::copy(&path, &destination)
                .map_err(|error| io_error("copy control backup file", error))?;
            let digest = sha256_file(&path).map_err(map_universal_error)?;
            if sha256_file(&destination).map_err(map_universal_error)? != digest {
                return Err(M6Error::new(
                    M6ErrorCode::RestoreInvalid,
                    "backup copy digest mismatch",
                    Some("backupPath"),
                    false,
                ));
            }
            files.push(BackupFile {
                relative_path: relative.to_string_lossy().into_owned(),
                digest,
                byte_length: metadata.len(),
            });
        }
    }
    Ok(())
}

fn verify_backup_files(backup_path: &Path, files: &[BackupFile]) -> M6Result<()> {
    for file in files {
        let relative = validate_relative(&file.relative_path)?;
        let path = backup_path.join("content").join(relative);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| io_error("inspect backup content", error))?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() != file.byte_length
            || sha256_file(&path).map_err(map_universal_error)? != file.digest
        {
            return Err(M6Error::new(
                M6ErrorCode::RestoreInvalid,
                format!("backup file identity mismatch: {}", file.relative_path),
                Some("backupPath"),
                false,
            ));
        }
    }
    Ok(())
}

fn validate_sqlite_database(connection: &Connection) -> M6Result<()> {
    let quick: String = connection
        .query_row("PRAGMA quick_check(20)", [], |row| row.get(0))
        .map_err(|error| M6Error::from_sql(error, "restored Registry quick_check failed"))?;
    if quick != "ok" {
        return Err(M6Error::new(
            M6ErrorCode::RestoreInvalid,
            format!("restored Registry quick_check returned {quick}"),
            Some("backupPath"),
            false,
        ));
    }
    let problem: Option<String> = connection
        .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
        .optional()
        .map_err(|error| M6Error::from_sql(error, "restored foreign key check failed"))?;
    if problem.is_some() {
        return Err(M6Error::new(
            M6ErrorCode::RestoreInvalid,
            "restored Registry has a foreign-key violation",
            Some("backupPath"),
            false,
        ));
    }
    Ok(())
}

fn ensure_directory_nofollow(path: &Path, root: &Path) -> M6Result<()> {
    if !path.starts_with(root) {
        return Err(M6Error::new(
            M6ErrorCode::LifecyclePlanConflict,
            "lifecycle path escaped control root",
            Some("path"),
            false,
        ));
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| io_error("inspect lifecycle directory", error))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(M6Error::new(
            M6ErrorCode::LifecyclePlanConflict,
            "lifecycle path is not a real directory",
            Some("path"),
            false,
        ));
    }
    Ok(())
}

fn validate_relative(value: &str) -> M6Result<&Path> {
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(M6Error::new(
            M6ErrorCode::RestoreInvalid,
            "backup manifest contains an unsafe relative path",
            Some("backupPath"),
            false,
        ));
    }
    Ok(path)
}

fn validate_operator(value: &str) -> M6Result<()> {
    if value.len() < 3
        || value.len() > 256
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_:./@".contains(&byte))
    {
        return Err(M6Error::invalid("invalid operator identity", "operatorId"));
    }
    Ok(())
}

fn validate_new_absolute_root(path: &Path, field: &str) -> M6Result<()> {
    if !path.is_absolute() || path.exists() {
        return Err(M6Error::invalid(
            format!("{field} must be a new absolute path"),
            field,
        ));
    }
    Ok(())
}

fn validate_existing_absolute_directory(path: &Path, field: &str) -> M6Result<()> {
    if !path.is_absolute() || !path.is_dir() {
        return Err(M6Error::invalid(
            format!("{field} must be an existing absolute directory"),
            field,
        ));
    }
    Ok(())
}

fn serialization_error(error: serde_json::Error) -> M6Error {
    M6Error::new(
        M6ErrorCode::RegistryCorrupt,
        format!("cannot serialize M7 lifecycle evidence: {error}"),
        None,
        false,
    )
}

fn io_error(context: &str, error: std::io::Error) -> M6Error {
    M6Error::new(
        M6ErrorCode::IoError,
        format!("{context}: {error}"),
        None,
        false,
    )
}

fn map_universal_error(error: crate::UniversalExecError) -> M6Error {
    M6Error::new(
        M6ErrorCode::IoError,
        error.message,
        error.field.as_deref(),
        error.retryable,
    )
}
