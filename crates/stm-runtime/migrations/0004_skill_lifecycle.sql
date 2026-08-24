CREATE TABLE IF NOT EXISTS authenticated_skill_catalog_state (
    channel TEXT PRIMARY KEY NOT NULL,
    catalog_version TEXT NOT NULL,
    key_id TEXT NOT NULL,
    manifest_sha256 TEXT NOT NULL,
    payload_sha256 TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    activated_at TEXT NOT NULL,
    state_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS managed_skill_receipts (
    target_key TEXT PRIMARY KEY NOT NULL,
    skill_id TEXT NOT NULL,
    client TEXT NOT NULL,
    target_path TEXT NOT NULL,
    tree_sha256 TEXT NOT NULL,
    source_commit TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    recorded_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS managed_skill_receipts_skill_id
    ON managed_skill_receipts(skill_id);

CREATE TABLE IF NOT EXISTS skill_backup_receipts (
    backup_id TEXT PRIMARY KEY NOT NULL,
    operation_id TEXT NOT NULL,
    target_key TEXT NOT NULL,
    state TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    recorded_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS skill_backup_receipts_target_key
    ON skill_backup_receipts(target_key, recorded_at DESC);

CREATE TABLE IF NOT EXISTS skill_recovery_journal (
    operation_id TEXT NOT NULL,
    target_key TEXT NOT NULL,
    phase TEXT NOT NULL,
    journal_json TEXT NOT NULL,
    recorded_at TEXT NOT NULL,
    PRIMARY KEY (operation_id, target_key)
);

PRAGMA user_version = 4;
