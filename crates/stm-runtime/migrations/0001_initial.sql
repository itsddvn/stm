CREATE TABLE IF NOT EXISTS snapshot_meta (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  generated_at TEXT NOT NULL,
  catalog_version TEXT NOT NULL,
  freshness TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS snapshot_payloads (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  tools_json TEXT NOT NULL,
  skills_json TEXT NOT NULL,
  mcp_json TEXT NOT NULL,
  updates_json TEXT NOT NULL,
  operations_json TEXT NOT NULL
);

PRAGMA user_version = 1;
