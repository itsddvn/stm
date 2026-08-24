CREATE TABLE IF NOT EXISTS lifecycle_receipts (
  operation_id TEXT PRIMARY KEY,
  plan_digest TEXT NOT NULL,
  consent_digest TEXT NOT NULL,
  consent_expires_at TEXT NOT NULL,
  consent_granted_at TEXT NOT NULL,
  operation_json TEXT NOT NULL,
  result_json TEXT NOT NULL,
  recorded_at TEXT NOT NULL
);

PRAGMA user_version = 3;
