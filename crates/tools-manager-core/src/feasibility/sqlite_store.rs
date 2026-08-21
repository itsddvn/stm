use std::{fs, path::Path};

use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::error::CoreError;

const INITIAL_MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS operation_receipts (
  id TEXT PRIMARY KEY,
  operation_id TEXT NOT NULL,
  status TEXT NOT NULL,
  started_at TEXT NOT NULL,
  completed_at TEXT,
  summary TEXT NOT NULL,
  details_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS recorded_consents (
  operation_id TEXT PRIMARY KEY,
  granted INTEGER NOT NULL,
  actor TEXT NOT NULL,
  recorded_at TEXT NOT NULL
);

PRAGMA user_version = 1;
"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SqliteOpenReport {
    pub path: String,
    pub user_version: i64,
}

pub fn open_and_migrate(path: &Path) -> Result<SqliteOpenReport, CoreError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.execute_batch(INITIAL_MIGRATION)?;
    let user_version =
        connection.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?;

    Ok(SqliteOpenReport {
        path: path.display().to_string(),
        user_version,
    })
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn sqlite_open_runs_initial_migration() {
        let temp = TempDir::new().expect("tempdir");
        let report = open_and_migrate(&temp.path().join("state/stm.sqlite")).expect("open sqlite");
        assert_eq!(report.user_version, 1);
    }
}
