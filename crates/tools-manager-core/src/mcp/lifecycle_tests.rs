use std::{
    fs,
    path::Path,
    sync::{Arc, Barrier},
    thread,
};

use tempfile::TempDir;

use crate::{
    domain::{
        inventory::InventoryState,
        mcp::{
            AuthReferenceState, McpBindingScope, McpBindingState, McpClientBindingRecord,
            McpClientName, McpHealthState, McpServerRecord, McpTransport, McpTrustState,
        },
    },
    error::CoreError,
    mcp::lifecycle::{
        client_config_path, config_digest, McpBackupReceipt, McpBackupState, McpConfigMaterializer,
        McpConfigTarget, McpMutationAction, McpRecoveryPhase, McpRecoveryRecord, McpTargetStatus,
        PreparedMcpMutation,
    },
    storage::SqliteSnapshotStore,
};

fn server() -> McpServerRecord {
    McpServerRecord {
        id: "filesystem".into(),
        name: "Filesystem".into(),
        description: "Filesystem MCP".into(),
        source: "modelcontextprotocol".into(),
        transport: McpTransport::Stdio,
        command_or_url: "npx".into(),
        args: vec![
            "-y".into(),
            "@modelcontextprotocol/server-filesystem".into(),
            "/tmp".into(),
        ],
        auth_references: Vec::new(),
        auth_required: false,
        clients: Vec::new(),
        capabilities: vec!["resources".into(), "tools".into()],
        trust: McpTrustState::Verified,
        auth_state: AuthReferenceState::None,
        health: McpHealthState::Unknown,
        last_checked: "not_checked".into(),
        state: InventoryState::ManagedCurrent,
    }
}

fn write_config(path: &Path, content: &[u8]) {
    fs::create_dir_all(path.parent().expect("config parent")).expect("create parent");
    fs::write(path, content).expect("write config");
}

fn target(home: &Path, client: McpClientName) -> McpConfigTarget {
    let config_path = client_config_path(home, &client);
    McpConfigTarget {
        expected_sha256: config_digest(&config_path).expect("config digest"),
        client,
        config_path,
        entry_name: "Filesystem".into(),
    }
}

#[test]
fn supported_client_mutation_is_receipt_backed_and_rollback_restores_exact_bytes() {
    let temp = TempDir::new().expect("tempdir");
    let home = temp.path().join("home");
    fs::create_dir_all(&home).expect("home");
    let database = temp.path().join("state/stm.sqlite");
    let codex_path = client_config_path(&home, &McpClientName::Codex);
    let claude_path = client_config_path(&home, &McpClientName::ClaudeCode);
    let cursor_path = client_config_path(&home, &McpClientName::Cursor);
    let codex = br#"model = "gpt-5"

[mcp_servers.Filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
capabilities = ["resources", "tools"]
"#;
    let claude = br#"{
  "theme": "dark",
  "mcpServers": {
    "Filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
      "capabilities": ["resources", "tools"]
    },
    "GitHub": {
      "url": "https://api.githubcopilot.com/mcp/",
      "transport": "streamable_http",
      "capabilities": ["tools", "prompts"]
    }
  }
}"#;
    let cursor = br#"{
  "mcpServers": {
    "Filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
    }
  },
  "unrelated": true
}"#;
    write_config(&codex_path, codex);
    write_config(&claude_path, claude);
    write_config(&cursor_path, cursor);

    let materializer = McpConfigMaterializer::new(&database, &home).expect("materializer");
    let prepared = PreparedMcpMutation {
        operation_id: "mcp-disable".into(),
        server: server(),
        action: McpMutationAction::Disable,
        targets: vec![
            target(&home, McpClientName::Codex),
            target(&home, McpClientName::ClaudeCode),
            target(&home, McpClientName::Cursor),
        ],
    };
    let outcome = materializer
        .materialize(&prepared, "2026-08-21T12:00:00Z")
        .expect("disable across clients");
    assert_eq!(outcome.completed, 3);
    assert_eq!(outcome.failed, 0);
    assert!(outcome
        .targets
        .iter()
        .all(|target| target.status == McpTargetStatus::Success));

    let codex_value: toml::Value =
        toml::from_str(&fs::read_to_string(&codex_path).expect("read codex config"))
            .expect("codex toml");
    assert_eq!(
        codex_value["mcp_servers"]["Filesystem"]["enabled"].as_bool(),
        Some(false)
    );
    for path in [&claude_path, &cursor_path] {
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(path).expect("read json config")).expect("json");
        assert_eq!(
            value["mcpServers"]["Filesystem"]["enabled"].as_bool(),
            Some(false)
        );
    }

    for target_outcome in &outcome.targets {
        let backup_id = target_outcome.backup_id.as_deref().expect("backup id");
        let restored = materializer
            .restore_backup(backup_id, "2026-08-21T12:01:00Z")
            .expect("restore backup");
        assert_eq!(restored.completed, 1);
        assert_eq!(restored.targets[0].status, McpTargetStatus::Restored);
    }
    assert_eq!(fs::read(&codex_path).expect("restored codex"), codex);
    assert_eq!(fs::read(&claude_path).expect("restored claude"), claude);
    assert_eq!(fs::read(&cursor_path).expect("restored cursor"), cursor);

    let remove = PreparedMcpMutation {
        operation_id: "mcp-remove".into(),
        server: server(),
        action: McpMutationAction::Remove,
        targets: vec![target(&home, McpClientName::ClaudeCode)],
    };
    materializer
        .materialize(&remove, "2026-08-21T12:02:00Z")
        .expect("remove one server");
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(&claude_path).expect("removed config"))
            .expect("removed JSON");
    assert!(value["mcpServers"].get("Filesystem").is_none());
    assert_eq!(
        value["mcpServers"]["GitHub"]["url"].as_str(),
        Some("https://api.githubcopilot.com/mcp/")
    );
    assert_eq!(value["theme"].as_str(), Some("dark"));
}

#[test]
fn digest_revalidation_rejects_every_target_before_any_write() {
    let temp = TempDir::new().expect("tempdir");
    let home = temp.path().join("home");
    fs::create_dir_all(&home).expect("home");
    let database = temp.path().join("state/stm.sqlite");
    let codex_path = client_config_path(&home, &McpClientName::Codex);
    let claude_path = client_config_path(&home, &McpClientName::ClaudeCode);
    let codex = b"[mcp_servers.Filesystem]\ncommand = \"npx\"\n";
    let claude = br#"{"mcpServers":{"Filesystem":{"command":"npx"}}}"#;
    write_config(&codex_path, codex);
    write_config(&claude_path, claude);
    let materializer = McpConfigMaterializer::new(&database, &home).expect("materializer");
    let mut stale_target = target(&home, McpClientName::ClaudeCode);
    stale_target.expected_sha256 = Some("sha256:stale".into());
    let prepared = PreparedMcpMutation {
        operation_id: "mcp-stale".into(),
        server: server(),
        action: McpMutationAction::Remove,
        targets: vec![target(&home, McpClientName::Codex), stale_target],
    };

    let error = materializer
        .materialize(&prepared, "2026-08-21T12:00:00Z")
        .expect_err("stale configuration must fail");
    assert!(matches!(error, CoreError::LifecycleEvidenceChanged(_)));
    assert_eq!(fs::read(&codex_path).expect("codex unchanged"), codex);
    assert_eq!(fs::read(&claude_path).expect("claude unchanged"), claude);
}

#[test]
fn rollback_rejects_configuration_changes_after_the_receipt() {
    let temp = TempDir::new().expect("tempdir");
    let home = temp.path().join("home");
    fs::create_dir_all(&home).expect("home");
    let database = temp.path().join("state/stm.sqlite");
    let config_path = client_config_path(&home, &McpClientName::Codex);
    write_config(
        &config_path,
        b"[mcp_servers.Filesystem]\ncommand = \"npx\"\n",
    );
    let materializer = McpConfigMaterializer::new(&database, &home).expect("materializer");
    let prepared = PreparedMcpMutation {
        operation_id: "mcp-rollback-stale".into(),
        server: server(),
        action: McpMutationAction::Disable,
        targets: vec![target(&home, McpClientName::Codex)],
    };
    let outcome = materializer
        .materialize(&prepared, "2026-08-21T12:00:00Z")
        .expect("disable");
    let backup_id = outcome.targets[0].backup_id.as_deref().expect("backup");
    let changed = b"model = \"gpt-5\"\n";
    write_config(&config_path, changed);

    let error = materializer
        .restore_backup(backup_id, "2026-08-21T12:01:00Z")
        .expect_err("stale rollback must fail");

    assert!(matches!(error, CoreError::LifecycleEvidenceChanged(_)));
    assert_eq!(fs::read(&config_path).expect("preserved config"), changed);
}

#[test]
fn target_paths_cannot_escape_the_approved_home() {
    let temp = TempDir::new().expect("tempdir");
    let home = temp.path().join("home");
    fs::create_dir_all(&home).expect("home");
    let outside = temp.path().join("outside.json");
    write_config(&outside, br#"{"mcpServers":{}}"#);
    let materializer = McpConfigMaterializer::new(temp.path().join("state/stm.sqlite"), &home)
        .expect("materializer");
    let prepared = PreparedMcpMutation {
        operation_id: "mcp-escape".into(),
        server: server(),
        action: McpMutationAction::Add,
        targets: vec![McpConfigTarget {
            client: McpClientName::ClaudeCode,
            config_path: outside,
            entry_name: "Filesystem".into(),
            expected_sha256: None,
        }],
    };

    let error = materializer
        .materialize(&prepared, "2026-08-21T12:00:00Z")
        .expect_err("path escape must fail");
    assert!(matches!(error, CoreError::PathEscape(_)));
}

#[test]
fn interrupted_replacement_is_rolled_back_before_recovery_journal_cleanup() {
    let temp = TempDir::new().expect("tempdir");
    let home = temp.path().join("home");
    fs::create_dir_all(&home).expect("home");
    let database = temp.path().join("state/stm.sqlite");
    let config_path = client_config_path(&home, &McpClientName::ClaudeCode);
    let original = br#"{"mcpServers":{"Filesystem":{"command":"npx"}}}"#;
    let replacement = br#"{"mcpServers":{"Filesystem":{"command":"npx","enabled":false}}}"#;
    write_config(&config_path, replacement);
    let backup_file_name = ".stm-mcp-backup-interrupted.json".to_string();
    let backup_path = config_path
        .parent()
        .expect("config parent")
        .join(&backup_file_name);
    let materializer = McpConfigMaterializer::new(&database, &home).expect("materializer");
    let encrypted = materializer
        .encrypted_backup_fixture("mcp-backup-interrupted", original)
        .expect("encrypted backup");
    write_config(&backup_path, &encrypted);
    let backup = McpBackupReceipt {
        backup_id: "mcp-backup-interrupted".into(),
        operation_id: "mcp-interrupted".into(),
        server_id: "filesystem".into(),
        client: McpClientName::ClaudeCode,
        backup_file_name,
        target_existed: true,
        original_sha256: Some(
            crate::adapters::compute_sha256([original.to_vec()])
                .trim_start_matches("sha256:")
                .to_string(),
        ),
        replacement_sha256: crate::adapters::compute_sha256([replacement.to_vec()])
            .trim_start_matches("sha256:")
            .to_string(),
        replacement_existed: true,
        state: McpBackupState::Available,
        recorded_at: "2026-08-21T12:00:00Z".into(),
    };
    let recovery = McpRecoveryRecord {
        operation_id: "mcp-interrupted".into(),
        server_id: "filesystem".into(),
        client: McpClientName::ClaudeCode,
        target_path: config_path.clone(),
        backup: backup.clone(),
        replacement_sha256: crate::adapters::compute_sha256([replacement.to_vec()])
            .trim_start_matches("sha256:")
            .to_string(),
        phase: McpRecoveryPhase::BackupCreated,
        recorded_at: "2026-08-21T12:00:00Z".into(),
    };
    let (store, _) = SqliteSnapshotStore::open(&database).expect("store");
    store.persist_mcp_backup(&backup).expect("backup receipt");
    store
        .persist_mcp_recovery(&recovery)
        .expect("recovery journal");

    materializer
        .recover_interrupted("2026-08-21T12:01:00Z")
        .expect("recover interrupted replacement");

    assert_eq!(fs::read(&config_path).expect("recovered config"), original);
    assert!(!backup_path.exists());
    assert!(store
        .load_mcp_recoveries()
        .expect("remaining recoveries")
        .is_empty());
    assert_eq!(
        store
            .load_mcp_backup("mcp-backup-interrupted")
            .expect("backup lookup")
            .expect("backup")
            .state,
        McpBackupState::Restored
    );
}
#[test]
fn plaintext_credentials_are_only_written_to_authenticated_encrypted_backups() {
    let temp = TempDir::new().expect("tempdir");
    let home = temp.path().join("home");
    fs::create_dir_all(&home).expect("home");
    let database = temp.path().join("state/stm.sqlite");
    let config_path = client_config_path(&home, &McpClientName::ClaudeCode);
    let original = br#"{"mcpServers":{"Filesystem":{"command":"npx","env":{"SERVICE_TOKEN":"raw-fixture-credential"}}}}"#;
    write_config(&config_path, original);
    let materializer = McpConfigMaterializer::new(&database, &home).expect("materializer");
    let prepared = PreparedMcpMutation {
        operation_id: "mcp-secret-backup".into(),
        server: server(),
        action: McpMutationAction::Disable,
        targets: vec![target(&home, McpClientName::ClaudeCode)],
    };

    let outcome = materializer
        .materialize(&prepared, "2026-08-21T12:00:00Z")
        .expect("encrypted backup outcome");

    assert_eq!(outcome.completed, 1);
    assert_eq!(outcome.failed, 0);
    assert!(!outcome.targets[0]
        .redacted_detail
        .contains("raw-fixture-credential"));
    let backup_id = outcome.targets[0].backup_id.as_deref().expect("backup id");
    let backup_path = fs::read_dir(config_path.parent().expect("config parent"))
        .expect("config directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(".stm-mcp-backup-"))
        })
        .expect("encrypted backup file");
    let backup_bytes = fs::read(&backup_path).expect("encrypted backup");
    assert!(backup_bytes.starts_with(b"STMMCP01"));
    assert!(!String::from_utf8_lossy(&backup_bytes).contains("raw-fixture-credential"));
    assert!(
        !String::from_utf8_lossy(&fs::read(&database).expect("database"))
            .contains("raw-fixture-credential")
    );

    materializer
        .restore_backup(backup_id, "2026-08-21T12:01:00Z")
        .expect("decrypt backup");
    assert_eq!(fs::read(&config_path).expect("restored config"), original);
}

#[test]
fn interrupted_recovery_rejects_user_changes_and_preserves_the_journal() {
    let temp = TempDir::new().expect("tempdir");
    let home = temp.path().join("home");
    fs::create_dir_all(&home).expect("home");
    let database = temp.path().join("state/stm.sqlite");
    let config_path = client_config_path(&home, &McpClientName::ClaudeCode);
    let original = br#"{"mcpServers":{"Filesystem":{"command":"npx"}}}"#;
    let replacement = br#"{"mcpServers":{"Filesystem":{"command":"npx","enabled":false}}}"#;
    let user_change = br#"{"mcpServers":{"Filesystem":{"command":"npx","enabled":true}}}"#;
    write_config(&config_path, user_change);
    let backup_file_name = ".stm-mcp-backup-stale-recovery.json".to_string();
    let backup_path = config_path
        .parent()
        .expect("config parent")
        .join(&backup_file_name);
    let materializer = McpConfigMaterializer::new(&database, &home).expect("materializer");
    let encrypted = materializer
        .encrypted_backup_fixture("mcp-backup-stale-recovery", original)
        .expect("encrypted backup");
    write_config(&backup_path, &encrypted);
    let digest = |bytes: &[u8]| {
        crate::adapters::compute_sha256([bytes.to_vec()])
            .trim_start_matches("sha256:")
            .to_string()
    };
    let backup = McpBackupReceipt {
        backup_id: "mcp-backup-stale-recovery".into(),
        operation_id: "mcp-stale-recovery".into(),
        server_id: "filesystem".into(),
        client: McpClientName::ClaudeCode,
        backup_file_name,
        target_existed: true,
        original_sha256: Some(digest(original)),
        replacement_sha256: digest(replacement),
        replacement_existed: true,
        state: McpBackupState::Available,
        recorded_at: "2026-08-21T12:00:00Z".into(),
    };
    let recovery = McpRecoveryRecord {
        operation_id: "mcp-stale-recovery".into(),
        server_id: "filesystem".into(),
        client: McpClientName::ClaudeCode,
        target_path: config_path.clone(),
        backup: backup.clone(),
        replacement_sha256: digest(replacement),
        phase: McpRecoveryPhase::ReplacementActivated,
        recorded_at: "2026-08-21T12:00:00Z".into(),
    };
    let (store, _) = SqliteSnapshotStore::open(&database).expect("store");
    store.persist_mcp_backup(&backup).expect("backup");
    store.persist_mcp_recovery(&recovery).expect("recovery");

    let error = materializer
        .recover_interrupted("2026-08-21T12:01:00Z")
        .expect_err("stale recovery must fail");

    assert!(matches!(error, CoreError::LifecycleEvidenceChanged(_)));
    assert_eq!(fs::read(&config_path).expect("user change"), user_change);
    assert!(backup_path.exists());
    assert_eq!(store.load_mcp_recoveries().expect("recoveries").len(), 1);
}

#[test]
fn client_specific_binding_metadata_drives_each_serialized_configuration() {
    let temp = TempDir::new().expect("tempdir");
    let home = temp.path().join("home");
    fs::create_dir_all(&home).expect("home");
    let database = temp.path().join("state/stm.sqlite");
    let mut aggregate = server();
    let mut claude = aggregate.clone();
    claude.args[2] = "/tmp/claude".into();
    let mut cursor = aggregate.clone();
    cursor.args[2] = "/tmp/cursor".into();
    aggregate.clients = vec![
        McpClientBindingRecord::from_server(
            McpClientName::ClaudeCode,
            McpBindingState::Enabled,
            McpBindingScope::Global,
            "Filesystem",
            &claude,
        ),
        McpClientBindingRecord::from_server(
            McpClientName::Cursor,
            McpBindingState::Enabled,
            McpBindingScope::Global,
            "Filesystem",
            &cursor,
        ),
    ];
    let materializer = McpConfigMaterializer::new(&database, &home).expect("materializer");
    let prepared = PreparedMcpMutation {
        operation_id: "mcp-client-bindings".into(),
        server: aggregate,
        action: McpMutationAction::Add,
        targets: vec![
            target(&home, McpClientName::ClaudeCode),
            target(&home, McpClientName::Cursor),
        ],
    };

    let outcome = materializer
        .materialize(&prepared, "2026-08-21T12:00:00Z")
        .expect("client-specific add");

    assert_eq!(outcome.completed, 2);
    let claude_value: serde_json::Value = serde_json::from_slice(
        &fs::read(client_config_path(&home, &McpClientName::ClaudeCode)).expect("claude config"),
    )
    .expect("claude json");
    let cursor_value: serde_json::Value = serde_json::from_slice(
        &fs::read(client_config_path(&home, &McpClientName::Cursor)).expect("cursor config"),
    )
    .expect("cursor json");
    assert_eq!(
        claude_value["mcpServers"]["Filesystem"]["args"][2].as_str(),
        Some("/tmp/claude")
    );
    assert_eq!(
        cursor_value["mcpServers"]["Filesystem"]["args"][2].as_str(),
        Some("/tmp/cursor")
    );
}

#[test]
fn overlapping_prepared_mutations_allow_only_one_stale_digest_to_commit() {
    let temp = TempDir::new().expect("tempdir");
    let home = temp.path().join("home");
    fs::create_dir_all(&home).expect("home");
    let config_path = client_config_path(&home, &McpClientName::ClaudeCode);
    write_config(
        &config_path,
        br#"{"mcpServers":{"Filesystem":{"command":"npx"}}}"#,
    );
    let target_a = target(&home, McpClientName::ClaudeCode);
    let target_b = target_a.clone();
    let materializer_a =
        McpConfigMaterializer::new(temp.path().join("state/a.sqlite"), &home).expect("first");
    let materializer_b =
        McpConfigMaterializer::new(temp.path().join("state/b.sqlite"), &home).expect("second");
    let barrier = Arc::new(Barrier::new(2));
    let first_barrier = barrier.clone();
    let first = thread::spawn(move || {
        first_barrier.wait();
        materializer_a.materialize(
            &PreparedMcpMutation {
                operation_id: "mcp-overlap-disable".into(),
                server: server(),
                action: McpMutationAction::Disable,
                targets: vec![target_a],
            },
            "2026-08-21T12:00:00Z",
        )
    });
    let second = thread::spawn(move || {
        barrier.wait();
        materializer_b.materialize(
            &PreparedMcpMutation {
                operation_id: "mcp-overlap-remove".into(),
                server: server(),
                action: McpMutationAction::Remove,
                targets: vec![target_b],
            },
            "2026-08-21T12:00:00Z",
        )
    });

    let results = [
        first.join().expect("first thread"),
        second.join().expect("second thread"),
    ];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(CoreError::LifecycleEvidenceChanged(_))))
            .count(),
        1
    );
}
