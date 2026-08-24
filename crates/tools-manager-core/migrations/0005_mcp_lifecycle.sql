CREATE TABLE IF NOT EXISTS mcp_lifecycle_receipts (
    receipt_id TEXT PRIMARY KEY,
    operation_id TEXT NOT NULL,
    server_id TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    recorded_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mcp_lifecycle_receipts_server
    ON mcp_lifecycle_receipts(server_id, recorded_at DESC);

CREATE TABLE IF NOT EXISTS mcp_backup_receipts (
    backup_id TEXT PRIMARY KEY,
    operation_id TEXT NOT NULL,
    server_id TEXT NOT NULL,
    client TEXT NOT NULL,
    state TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    recorded_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mcp_backup_receipts_server
    ON mcp_backup_receipts(server_id, recorded_at DESC);

CREATE TABLE IF NOT EXISTS mcp_recovery_journal (
    operation_id TEXT NOT NULL,
    client TEXT NOT NULL,
    phase TEXT NOT NULL,
    journal_json TEXT NOT NULL,
    recorded_at TEXT NOT NULL,
    PRIMARY KEY (operation_id, client)
);
