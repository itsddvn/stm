use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::{
    domain::{
        application_update::ApplicationUpdateRecord,
        inventory::Freshness,
        lifecycle::{
            LifecycleConsentAuthorization, LifecycleExecutionResult, LifecyclePlanRequest,
        },
        mcp::McpServerRecord,
        operation::OperationReceipt,
        skill::SkillRecord,
        tool::ToolRecord,
    },
    error::CoreError,
};

const MIGRATIONS: &[(&str, &str)] = &[
    (
        "0001_initial.sql",
        include_str!("../../migrations/0001_initial.sql"),
    ),
    (
        "0002_read_only_snapshot.sql",
        include_str!("../../migrations/0002_read_only_snapshot.sql"),
    ),
    (
        "0003_lifecycle_receipts.sql",
        include_str!("../../migrations/0003_lifecycle_receipts.sql"),
    ),
];
static STORAGE_WRITES: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationLogEntry {
    pub receipt: OperationReceipt,
    pub resource: String,
    pub action: String,
    pub owner: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_request: Option<LifecyclePlanRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_result: Option<LifecycleExecutionResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_process_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_process_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScanErrorEntry {
    pub scope: String,
    pub code: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotBundle {
    pub generated_at: String,
    pub catalog_version: String,
    pub freshness: Freshness,
    pub tools: Vec<ToolRecord>,
    pub skills: Vec<SkillRecord>,
    pub mcp_servers: Vec<McpServerRecord>,
    pub updates: Vec<ApplicationUpdateRecord>,
    pub operations: Vec<OperationLogEntry>,
    pub errors: Vec<ScanErrorEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StorageHealth {
    pub path: String,
    pub user_version: i64,
    pub recovered_from_corruption: bool,
    pub last_good_available: bool,
}

pub struct SqliteSnapshotStore {
    db_path: PathBuf,
    last_good_path: PathBuf,
    recovered_from_corruption: bool,
}

impl SqliteSnapshotStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<(Self, StorageHealth), CoreError> {
        let db_path = path.into();
        let last_good_path = db_path.with_extension("last-good.sqlite");
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut recovered_from_corruption = false;
        if db_path.exists() && !integrity_check(&db_path)? {
            recovered_from_corruption = true;
            for suffix in ["-wal", "-shm"] {
                let _ = fs::remove_file(sqlite_sidecar_path(&db_path, suffix));
            }
            let corrupt_path = db_path.with_extension("corrupt.sqlite");
            let _ = fs::rename(&db_path, &corrupt_path);
            if last_good_path.exists() {
                fs::copy(&last_good_path, &db_path)?;
            }
        }

        let connection = open_connection(&db_path)?;
        apply_migrations(&connection)?;
        let user_version =
            connection.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?;
        drop(connection);

        let store = Self {
            db_path: db_path.clone(),
            last_good_path: last_good_path.clone(),
            recovered_from_corruption,
        };
        let health = StorageHealth {
            path: db_path.display().to_string(),
            user_version,
            recovered_from_corruption,
            last_good_available: last_good_path.exists(),
        };
        Ok((store, health))
    }

    pub fn persist_snapshot(&self, snapshot: &SnapshotBundle) -> Result<(), CoreError> {
        let _write = STORAGE_WRITES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut connection = open_connection(&self.db_path)?;
        apply_migrations(&connection)?;
        let tx = connection.transaction()?;
        tx.execute("DELETE FROM snapshot_meta", [])?;
        tx.execute("DELETE FROM snapshot_payloads", [])?;
        tx.execute("DELETE FROM scan_errors", [])?;
        tx.execute(
            "INSERT INTO snapshot_meta (id, generated_at, catalog_version, freshness) VALUES (1, ?1, ?2, ?3)",
            params![
                snapshot.generated_at,
                snapshot.catalog_version,
                freshness_label(&snapshot.freshness),
            ],
        )?;
        tx.execute(
            "INSERT INTO snapshot_payloads (id, tools_json, skills_json, mcp_json, updates_json, operations_json) VALUES (1, ?1, ?2, ?3, ?4, ?5)",
            params![
                serde_json::to_string(&snapshot.tools)?,
                serde_json::to_string(&snapshot.skills)?,
                serde_json::to_string(&snapshot.mcp_servers)?,
                serde_json::to_string(&snapshot.updates)?,
                serde_json::to_string(&snapshot.operations)?,
            ],
        )?;
        for error in &snapshot.errors {
            tx.execute(
                "INSERT INTO scan_errors (scope, code, detail) VALUES (?1, ?2, ?3)",
                params![error.scope, error.code, error.detail],
            )?;
        }
        tx.commit()?;
        connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        fs::copy(&self.db_path, &self.last_good_path)?;
        Ok(())
    }

    pub fn load_snapshot(&self) -> Result<Option<SnapshotBundle>, CoreError> {
        let connection = open_connection(&self.db_path)?;
        let mut meta = connection.prepare(
            "SELECT generated_at, catalog_version, freshness FROM snapshot_meta WHERE id = 1",
        )?;
        let meta_row = meta.query_row([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        });
        let Ok((generated_at, catalog_version, freshness)) = meta_row else {
            return Ok(None);
        };

        let mut payload = connection.prepare(
            "SELECT tools_json, skills_json, mcp_json, updates_json, operations_json FROM snapshot_payloads WHERE id = 1",
        )?;
        let (tools_json, skills_json, mcp_json, updates_json, operations_json) = payload
            .query_row([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?;

        let mut errors = connection.prepare("SELECT scope, code, detail FROM scan_errors")?;
        let errors = errors
            .query_map([], |row| {
                Ok(ScanErrorEntry {
                    scope: row.get(0)?,
                    code: row.get(1)?,
                    detail: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(SnapshotBundle {
            generated_at,
            catalog_version,
            freshness: freshness_from_label(&freshness),
            tools: serde_json::from_str(&tools_json)?,
            skills: serde_json::from_str(&skills_json)?,
            mcp_servers: serde_json::from_str(&mcp_json)?,
            updates: serde_json::from_str(&updates_json)?,
            operations: serde_json::from_str(&operations_json)?,
            errors,
        }))
    }
    pub fn persist_lifecycle_receipt(
        &self,
        operation: &OperationLogEntry,
        result: &LifecycleExecutionResult,
        authorization: &LifecycleConsentAuthorization,
        recorded_at: &str,
    ) -> Result<(), CoreError> {
        let _write = STORAGE_WRITES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let connection = open_connection(&self.db_path)?;
        apply_migrations(&connection)?;
        connection.execute(
            "INSERT OR REPLACE INTO lifecycle_receipts (
                operation_id, plan_digest, consent_digest, consent_expires_at,
                consent_granted_at, operation_json, result_json, recorded_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                result.operation_id,
                result.plan_digest,
                authorization.plan_digest,
                authorization.plan_expires_at,
                authorization.granted_at,
                serde_json::to_string(operation)?,
                serde_json::to_string(result)?,
                recorded_at,
            ],
        )?;
        connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        fs::copy(&self.db_path, &self.last_good_path)?;
        Ok(())
    }

    pub fn reconcile_lifecycle_receipt(
        &self,
        operation: &OperationLogEntry,
        result: &LifecycleExecutionResult,
        recorded_at: &str,
    ) -> Result<(), CoreError> {
        let _write = STORAGE_WRITES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let connection = open_connection(&self.db_path)?;
        apply_migrations(&connection)?;
        connection.execute(
            "UPDATE lifecycle_receipts
             SET operation_json = ?1, result_json = ?2, recorded_at = ?3
             WHERE operation_id = ?4",
            params![
                serde_json::to_string(operation)?,
                serde_json::to_string(result)?,
                recorded_at,
                result.operation_id,
            ],
        )?;
        connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        fs::copy(&self.db_path, &self.last_good_path)?;
        Ok(())
    }

    pub fn persist_lifecycle_child_process(
        &self,
        operation_id: &str,
        child_process_id: u32,
    ) -> Result<(), CoreError> {
        let _write = STORAGE_WRITES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let connection = open_connection(&self.db_path)?;
        apply_migrations(&connection)?;
        let operation_json: String = connection.query_row(
            "SELECT operation_json FROM lifecycle_receipts WHERE operation_id = ?1",
            params![operation_id],
            |row| row.get(0),
        )?;
        let mut operation: OperationLogEntry = serde_json::from_str(&operation_json)?;
        operation.child_process_id = Some(child_process_id);
        connection.execute(
            "UPDATE lifecycle_receipts SET operation_json = ?1 WHERE operation_id = ?2",
            params![serde_json::to_string(&operation)?, operation_id],
        )?;
        connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        fs::copy(&self.db_path, &self.last_good_path)?;
        Ok(())
    }

    pub fn load_lifecycle_receipts(&self) -> Result<Vec<OperationLogEntry>, CoreError> {
        let connection = open_connection(&self.db_path)?;
        apply_migrations(&connection)?;
        let mut statement = connection.prepare(
            "SELECT operation_json, result_json FROM lifecycle_receipts ORDER BY recorded_at DESC",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(operation_json, result_json)| {
                let mut operation: OperationLogEntry = serde_json::from_str(&operation_json)?;
                operation.lifecycle_result = Some(serde_json::from_str(&result_json)?);
                Ok(operation)
            })
            .collect()
    }

    pub fn recovered_from_corruption(&self) -> bool {
        self.recovered_from_corruption
    }
}

fn open_connection(path: &Path) -> Result<Connection, CoreError> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(Duration::from_secs(10))?;
    Ok(connection)
}

fn apply_migrations(connection: &Connection) -> Result<(), CoreError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );",
    )?;

    for (version, sql) in MIGRATIONS {
        let exists: i64 = connection.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = ?1",
            params![version],
            |row| row.get(0),
        )?;

        if exists == 0 {
            connection.execute_batch(sql)?;
            connection.execute(
                "INSERT INTO schema_migrations (version) VALUES (?1)",
                params![version],
            )?;
        }
    }
    Ok(())
}
fn sqlite_sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut sidecar = path.as_os_str().to_os_string();
    sidecar.push(suffix);
    PathBuf::from(sidecar)
}

fn integrity_check(path: &Path) -> Result<bool, CoreError> {
    let connection = open_connection(path)?;
    match connection.pragma_query_value(None, "integrity_check", |row| row.get::<_, String>(0)) {
        Ok(check) => Ok(check == "ok"),
        Err(rusqlite::Error::SqliteFailure(error, _))
            if matches!(
                error.code,
                rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase
            ) =>
        {
            Ok(false)
        }
        Err(error) => Err(error.into()),
    }
}

fn freshness_label(value: &Freshness) -> &'static str {
    match value {
        Freshness::Fresh => "fresh",
        Freshness::Stale => "stale",
        Freshness::Unknown => "unknown",
    }
}

fn freshness_from_label(value: &str) -> Freshness {
    match value {
        "fresh" => Freshness::Fresh,
        "stale" => Freshness::Stale,
        _ => Freshness::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn sample_snapshot() -> SnapshotBundle {
        SnapshotBundle {
            generated_at: "2026-08-20T10:00:00Z".to_string(),
            catalog_version: "2026-08-20".to_string(),
            freshness: Freshness::Fresh,
            tools: Vec::new(),
            skills: Vec::new(),
            mcp_servers: Vec::new(),
            updates: Vec::new(),
            operations: vec![OperationLogEntry {
                receipt: OperationReceipt {
                    id: "receipt-1".to_string(),
                    operation_id: "op-1".to_string(),
                    status: crate::domain::operation::OperationStatus::Success,
                    started_at: "2026-08-20T10:00:00Z".to_string(),
                    completed_at: Some("2026-08-20T10:00:01Z".to_string()),
                    summary: "snapshot".to_string(),
                    details: vec!["ok".to_string()],
                },
                resource: "STM".to_string(),
                action: "Inventory refresh".to_string(),
                owner: "STM".to_string(),
                lifecycle_request: None,
                lifecycle_result: None,
                owner_process_id: None,
                child_process_id: None,
            }],
            errors: vec![ScanErrorEntry {
                scope: "mcp".to_string(),
                code: "malformed_entry".to_string(),
                detail: "Broken Entry".to_string(),
            }],
        }
    }

    #[test]
    fn stores_and_recovers_snapshot() {
        let temp = TempDir::new().expect("tempdir");
        let path = temp.path().join("stm.sqlite");
        let (store, _) = SqliteSnapshotStore::open(&path).expect("open");
        store.persist_snapshot(&sample_snapshot()).expect("persist");
        let loaded = store.load_snapshot().expect("load").expect("snapshot");
        assert_eq!(loaded.catalog_version, "2026-08-20");
        assert_eq!(loaded.errors.len(), 1);
    }

    #[test]
    fn restores_last_good_snapshot_after_corruption() {
        let temp = TempDir::new().expect("tempdir");
        let path = temp.path().join("stm.sqlite");
        let (store, _) = SqliteSnapshotStore::open(&path).expect("open");
        store.persist_snapshot(&sample_snapshot()).expect("persist");
        fs::write(&path, "not sqlite").expect("corrupt");
        let (_, health) = SqliteSnapshotStore::open(&path).expect("reopen");
        assert!(health.recovered_from_corruption);
        assert!(health.last_good_available);
    }

    #[test]
    fn last_good_recovery_preserves_lifecycle_receipts() {
        let temp = TempDir::new().expect("tempdir");
        let path = temp.path().join("stm.sqlite");
        let (store, _) = SqliteSnapshotStore::open(&path).expect("open");
        let snapshot = sample_snapshot();
        store.persist_snapshot(&snapshot).expect("persist snapshot");
        let result = crate::domain::lifecycle::LifecycleExecutionResult {
            operation_id: "op-1".to_string(),
            plan_digest: "sha256:plan".to_string(),
            status: crate::domain::lifecycle::LifecycleExecutionStatus::Success,
            completed_steps: 1,
            total_steps: 1,
            can_cancel: false,
            receipt: Some("receipt-1".to_string()),
            redacted_detail: "completed".to_string(),
            items: Vec::new(),
            retry_actions: Vec::new(),
            recovery_actions: Vec::new(),
        };
        let authorization = LifecycleConsentAuthorization {
            plan_digest: result.plan_digest.clone(),
            plan_expires_at: "2026-08-20T10:01:00Z".to_string(),
            granted_at: "2026-08-20T10:00:00Z".to_string(),
        };
        store
            .persist_lifecycle_receipt(
                &snapshot.operations[0],
                &result,
                &authorization,
                "2026-08-20T10:00:01Z",
            )
            .expect("persist receipt");

        fs::write(&path, "not sqlite").expect("corrupt");
        fs::write(sqlite_sidecar_path(&path, "-wal"), "stale wal").expect("stale wal");
        fs::write(sqlite_sidecar_path(&path, "-shm"), "stale shm").expect("stale shm");
        let (recovered, health) = SqliteSnapshotStore::open(&path).expect("reopen");
        assert!(!sqlite_sidecar_path(&path, "-wal").exists());
        assert!(!sqlite_sidecar_path(&path, "-shm").exists());
        assert!(health.recovered_from_corruption);
        let receipts = recovered.load_lifecycle_receipts().expect("receipts");
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].lifecycle_result.as_ref(), Some(&result));
    }
}
