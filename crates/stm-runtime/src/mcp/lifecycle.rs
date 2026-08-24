use std::{
    collections::BTreeSet,
    fs,
    fs::OpenOptions,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    MoveFileExW, ReplaceFileW, MOVEFILE_WRITE_THROUGH, REPLACEFILE_WRITE_THROUGH,
};

use fs2::FileExt;

use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use toml::Value as TomlValue;

use crate::storage::SqliteSnapshotStore;
use stm_core::{
    domain::mcp::{
        AuthReferenceKind, McpClientName, McpHealthState, McpServerRecord, McpTransport,
    },
    mcp::lifecycle::{
        McpBackupReceipt, McpBackupState, McpConfigTarget, McpLifecycleReceipt, McpMutationAction,
        McpMutationOutcome, McpRecoveryPhase, McpRecoveryRecord, McpTargetOutcome, McpTargetStatus,
        PreparedMcpMutation,
    },
    CoreError,
};

const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

use super::backup_crypto::{decrypt_backup_bytes, encrypt_backup_bytes, load_backup_key};
const MAX_BACKUP_BYTES: u64 = MAX_CONFIG_BYTES + 128;
static MCP_CONFIG_WRITES: Mutex<()> = Mutex::new(());

pub struct McpConfigMaterializer {
    store: SqliteSnapshotStore,
    home: PathBuf,
    backup_key_path: PathBuf,
    backup_key: Mutex<Option<[u8; 32]>>,
}
impl McpConfigMaterializer {
    pub fn new(
        workspace_db_path: impl AsRef<Path>,
        home: impl AsRef<Path>,
    ) -> Result<Self, CoreError> {
        let home = fs::canonicalize(home)?;
        if !home.is_dir() {
            return Err(CoreError::InvalidPath(
                "MCP home root is not a directory".into(),
            ));
        }
        let db_path = workspace_db_path.as_ref().to_path_buf();
        let (store, _) = SqliteSnapshotStore::open(db_path.clone())?;
        Ok(Self {
            store,
            home,
            backup_key_path: db_path,
            backup_key: Mutex::new(None),
        })
    }

    fn backup_key(&self) -> Result<[u8; 32], CoreError> {
        let mut key = self
            .backup_key
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        if let Some(key) = *key {
            return Ok(key);
        }
        let loaded = load_backup_key(&self.backup_key_path)?;
        *key = Some(loaded);
        Ok(loaded)
    }

    #[cfg(test)]
    pub(super) fn encrypted_backup_fixture(
        &self,
        backup_id: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CoreError> {
        encrypt_backup_bytes(&self.backup_key()?, backup_id, plaintext)
    }

    pub fn materialize(
        &self,
        prepared: &PreparedMcpMutation,
        recorded_at: &str,
    ) -> Result<McpMutationOutcome, CoreError> {
        let _write = MCP_CONFIG_WRITES
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        if matches!(
            prepared.action,
            McpMutationAction::Add | McpMutationAction::Update | McpMutationAction::Enable
        ) {
            for target in &prepared.targets {
                let server = server_for_target(&prepared.server, target);
                stm_core::mcp::policy::validate_lifecycle_server(&server)
                    .map_err(CoreError::CommandDenied)?;
            }
        } else {
            stm_core::mcp::policy::validate_inventory_fields(
                &prepared.server.transport,
                &prepared.server.command_or_url,
                &prepared.server.args,
                &prepared.server.capabilities,
            )
            .map_err(CoreError::CommandDenied)?;
        }
        if prepared.targets.is_empty() {
            return Err(CoreError::MalformedInput(
                "MCP mutation has no targets".into(),
            ));
        }
        for target in &prepared.targets {
            self.validate_target(target)?;
        }
        let _scope_locks = lock_config_scopes(
            prepared
                .targets
                .iter()
                .map(|target| target.config_path.as_path()),
        )?;
        for target in &prepared.targets {
            if config_digest(&target.config_path)? != target.expected_sha256 {
                return Err(CoreError::LifecycleEvidenceChanged(
                    "MCP client configuration changed after review".into(),
                ));
            }
        }

        let mut outcomes = Vec::with_capacity(prepared.targets.len());
        for target in &prepared.targets {
            outcomes.push(match self.apply_target(prepared, target, recorded_at) {
                Ok(outcome) => outcome,
                Err(error) => McpTargetOutcome {
                    client: target.client.clone(),
                    status: McpTargetStatus::Failed,
                    receipt_id: None,
                    backup_id: None,
                    health: McpHealthState::Unknown,
                    redacted_detail: format!("Client configuration mutation failed: {error}"),
                },
            });
        }
        let completed = outcomes
            .iter()
            .filter(|outcome| {
                matches!(
                    outcome.status,
                    McpTargetStatus::Success | McpTargetStatus::NoOp | McpTargetStatus::Restored
                )
            })
            .count();
        let failed = outcomes.len().saturating_sub(completed);
        Ok(McpMutationOutcome {
            operation_id: prepared.operation_id.clone(),
            completed,
            failed,
            targets: outcomes,
        })
    }

    pub fn record_health(
        &self,
        outcome: &mut McpMutationOutcome,
        client: &McpClientName,
        health: McpHealthState,
        recorded_at: &str,
    ) -> Result<(), CoreError> {
        let Some(target) = outcome
            .targets
            .iter_mut()
            .find(|target| &target.client == client)
        else {
            return Ok(());
        };
        let Some(receipt_id) = target.receipt_id.as_deref() else {
            return Ok(());
        };
        self.store
            .update_mcp_receipt_health(receipt_id, health.clone(), recorded_at)?;
        target.health = health.clone();
        target.redacted_detail = format!(
            "Client configuration changed atomically; protocol initialization health is {}.",
            health_label(&health)
        );
        Ok(())
    }

    pub fn restore_backup(
        &self,
        backup_id: &str,
        recorded_at: &str,
    ) -> Result<McpMutationOutcome, CoreError> {
        let _write = MCP_CONFIG_WRITES
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        let mut backup = self.store.load_mcp_backup(backup_id)?.ok_or_else(|| {
            CoreError::LifecycleEvidenceChanged("MCP backup is unavailable".into())
        })?;
        if backup.state != McpBackupState::Available {
            return Err(CoreError::LifecycleEvidenceChanged(
                "MCP backup is no longer available".into(),
            ));
        }
        let target_path = client_config_path(&self.home, &backup.client);
        let backup_path = target_path
            .parent()
            .ok_or_else(|| CoreError::InvalidPath("MCP config has no parent".into()))?
            .join(&backup.backup_file_name);
        self.validate_path(&target_path)?;
        let _scope_locks = lock_config_scopes([target_path.as_path()])?;
        let current_existed = target_path.exists();
        let current_sha256 = config_digest(&target_path)?.unwrap_or_else(|| sha256(&[]));
        if backup.replacement_sha256.is_empty()
            || current_existed != backup.replacement_existed
            || current_sha256 != backup.replacement_sha256
        {
            return Err(CoreError::LifecycleEvidenceChanged(
                "MCP client configuration changed after the receipt; rollback requires fresh evidence"
                    .into(),
            ));
        }
        if backup.target_existed {
            let original = decrypt_backup_bytes(
                &self.backup_key()?,
                &backup.backup_id,
                &read_bounded_with_limit(&backup_path, MAX_BACKUP_BYTES)?,
            )?;
            if backup.original_sha256.as_deref() != Some(sha256(&original).as_str()) {
                return Err(CoreError::LifecycleEvidenceChanged(
                    "MCP backup digest changed".into(),
                ));
            }
            replace_from_bytes(&target_path, &original)?;
        } else if target_path.exists() {
            fs::remove_file(&target_path)?;
        }
        remove_file_if_exists(&backup_path)?;
        backup.state = McpBackupState::Restored;
        backup.recorded_at = recorded_at.to_string();
        self.store.persist_mcp_backup(&backup)?;
        Ok(McpMutationOutcome {
            operation_id: backup.operation_id.clone(),
            completed: 1,
            failed: 0,
            targets: vec![McpTargetOutcome {
                client: backup.client,
                status: McpTargetStatus::Restored,
                receipt_id: None,
                backup_id: Some(backup.backup_id),
                health: McpHealthState::Unknown,
                redacted_detail: "Receipt-backed client configuration restored.".into(),
            }],
        })
    }

    pub fn recover_interrupted(&self, recorded_at: &str) -> Result<(), CoreError> {
        let _write = MCP_CONFIG_WRITES
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        for mut recovery in self.store.load_mcp_recoveries()? {
            self.validate_path(&recovery.target_path)?;
            let _scope_locks = lock_config_scopes([recovery.target_path.as_path()])?;
            let backup_path = recovery
                .target_path
                .parent()
                .ok_or_else(|| CoreError::InvalidPath("MCP config has no parent".into()))?
                .join(&recovery.backup.backup_file_name);
            match recovery.phase {
                McpRecoveryPhase::Prepared => {}
                McpRecoveryPhase::BackupCreated | McpRecoveryPhase::ReplacementActivated => {
                    let current_existed = recovery.target_path.exists();
                    let current_sha256 =
                        config_digest(&recovery.target_path)?.unwrap_or_else(|| sha256(&[]));
                    let replacement_matches = recovery.backup.replacement_existed
                        == current_existed
                        && recovery.backup.replacement_sha256 == current_sha256;
                    let original_matches = if recovery.backup.target_existed {
                        current_existed
                            && recovery.backup.original_sha256.as_deref()
                                == Some(current_sha256.as_str())
                    } else {
                        !current_existed
                    };
                    let should_restore = match recovery.phase {
                        McpRecoveryPhase::BackupCreated => {
                            if !replacement_matches && !original_matches {
                                return Err(CoreError::LifecycleEvidenceChanged(
                                    "MCP client configuration changed during interrupted recovery"
                                        .into(),
                                ));
                            }
                            replacement_matches
                        }
                        McpRecoveryPhase::ReplacementActivated => {
                            if !replacement_matches {
                                return Err(CoreError::LifecycleEvidenceChanged(
                                    "MCP client configuration changed after interrupted activation"
                                        .into(),
                                ));
                            }
                            true
                        }
                        McpRecoveryPhase::Prepared => unreachable!(),
                    };
                    if should_restore {
                        if recovery.backup.target_existed {
                            let original = decrypt_backup_bytes(
                                &self.backup_key()?,
                                &recovery.backup.backup_id,
                                &read_bounded_with_limit(&backup_path, MAX_BACKUP_BYTES)?,
                            )?;
                            if recovery.backup.original_sha256.as_deref()
                                != Some(sha256(&original).as_str())
                            {
                                return Err(CoreError::LifecycleEvidenceChanged(
                                    "MCP backup digest changed".into(),
                                ));
                            }
                            replace_from_bytes(&recovery.target_path, &original)?;
                        } else {
                            remove_file_if_exists(&recovery.target_path)?;
                        }
                    }
                    remove_file_if_exists(&backup_path)?;
                    recovery.backup.state = McpBackupState::Restored;
                    recovery.backup.recorded_at = recorded_at.to_string();
                    self.store.persist_mcp_backup(&recovery.backup)?;
                }
            }
            self.store
                .delete_mcp_recovery(&recovery.operation_id, &recovery.client)?;
        }
        Ok(())
    }

    fn apply_target(
        &self,
        prepared: &PreparedMcpMutation,
        target: &McpConfigTarget,
        recorded_at: &str,
    ) -> Result<McpTargetOutcome, CoreError> {
        self.validate_target(target)?;
        if config_digest(&target.config_path)? != target.expected_sha256 {
            return Err(CoreError::LifecycleEvidenceChanged(
                "MCP client configuration changed immediately before mutation".into(),
            ));
        }
        let existing = target.config_path.exists();
        let original = if existing {
            read_bounded(&target.config_path)?
        } else {
            Vec::new()
        };
        let server = server_for_target(&prepared.server, target);
        let replacement = mutate_config(&original, target, &server, &prepared.action)?;
        if existing && replacement == original {
            return Ok(McpTargetOutcome {
                client: target.client.clone(),
                status: McpTargetStatus::NoOp,
                receipt_id: None,
                backup_id: None,
                health: McpHealthState::Unknown,
                redacted_detail: "Client configuration already matches the reviewed state.".into(),
            });
        }
        if !existing && replacement.is_empty() {
            return Ok(McpTargetOutcome {
                client: target.client.clone(),
                status: McpTargetStatus::NoOp,
                receipt_id: None,
                backup_id: None,
                health: McpHealthState::Unknown,
                redacted_detail: "Client configuration entry was already absent.".into(),
            });
        }

        let backup_id = format!(
            "mcp-backup-{}",
            short_id(&format!("{}:{:?}", prepared.operation_id, target.client))
        );
        let extension = target
            .config_path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("config");
        let backup_file_name = format!(".stm-{backup_id}.{extension}");
        let backup_path = target
            .config_path
            .parent()
            .ok_or_else(|| CoreError::InvalidPath("MCP config has no parent".into()))?
            .join(&backup_file_name);
        let mut backup = McpBackupReceipt {
            backup_id: backup_id.clone(),
            operation_id: prepared.operation_id.clone(),
            server_id: prepared.server.id.clone(),
            client: target.client.clone(),
            backup_file_name,
            target_existed: existing,
            original_sha256: existing.then(|| sha256(&original)),
            replacement_sha256: sha256(&replacement),
            replacement_existed: !replacement.is_empty(),
            state: McpBackupState::Available,
            recorded_at: recorded_at.to_string(),
        };
        let mut recovery = McpRecoveryRecord {
            operation_id: prepared.operation_id.clone(),
            server_id: prepared.server.id.clone(),
            client: target.client.clone(),
            target_path: target.config_path.clone(),
            backup: backup.clone(),
            replacement_sha256: sha256(&replacement),
            phase: McpRecoveryPhase::Prepared,
            recorded_at: recorded_at.to_string(),
        };
        self.store.persist_mcp_recovery(&recovery)?;
        if existing {
            let encrypted = encrypt_backup_bytes(&self.backup_key()?, &backup_id, &original)?;
            write_private_file(&backup_path, &encrypted)?;
        }
        self.store.persist_mcp_backup(&backup)?;
        recovery.phase = McpRecoveryPhase::BackupCreated;
        self.store.persist_mcp_recovery(&recovery)?;

        let activation = if replacement.is_empty() {
            remove_file_if_exists(&target.config_path)
        } else {
            replace_from_bytes(&target.config_path, &replacement)
        };
        if let Err(error) = activation {
            if existing {
                let _ = replace_from_bytes(&target.config_path, &original);
            } else {
                let _ = remove_file_if_exists(&target.config_path);
            }
            return Err(error);
        }
        let receipt_id = format!(
            "mcp-receipt-{}",
            short_id(&format!("{}:{:?}", prepared.operation_id, target.client))
        );
        let receipt = McpLifecycleReceipt {
            receipt_id: receipt_id.clone(),
            operation_id: prepared.operation_id.clone(),
            server_id: prepared.server.id.clone(),
            action: prepared.action.clone(),
            client: target.client.clone(),
            config_sha256: config_digest(&target.config_path)?,
            health: McpHealthState::Unknown,
            recorded_at: recorded_at.to_string(),
        };
        backup.state = McpBackupState::Available;
        if let Err(error) = self.store.finalize_mcp_activation(&receipt, &backup) {
            if existing {
                let _ = replace_from_bytes(&target.config_path, &original);
            } else {
                let _ = remove_file_if_exists(&target.config_path);
            }
            return Err(error);
        }
        Ok(McpTargetOutcome {
            client: target.client.clone(),
            status: McpTargetStatus::Success,
            receipt_id: Some(receipt_id),
            backup_id: Some(backup_id),
            health: McpHealthState::Unknown,
            redacted_detail:
                "Client configuration changed atomically; protocol health remains unverified."
                    .into(),
        })
    }

    fn validate_target(&self, target: &McpConfigTarget) -> Result<(), CoreError> {
        self.validate_path(&target.config_path)?;
        let actual = resolve_candidate(&target.config_path)?;
        let expected = resolve_candidate(&client_config_path(&self.home, &target.client))?;
        if actual != expected {
            return Err(CoreError::PathEscape(target.config_path.clone()));
        }
        Ok(())
    }

    fn validate_path(&self, path: &Path) -> Result<(), CoreError> {
        if path.exists() && fs::symlink_metadata(path)?.file_type().is_symlink() {
            return Err(CoreError::PathEscape(path.to_path_buf()));
        }
        let resolved = resolve_candidate(path)?;
        if !resolved.starts_with(&self.home) {
            return Err(CoreError::PathEscape(path.to_path_buf()));
        }
        let parent = path
            .parent()
            .ok_or_else(|| CoreError::InvalidPath("MCP config has no parent".into()))?;
        fs::create_dir_all(parent)?;
        Ok(())
    }
}

fn resolve_candidate(path: &Path) -> Result<PathBuf, CoreError> {
    let mut cursor = path;
    let mut missing = Vec::new();
    while !cursor.exists() {
        let name = cursor
            .file_name()
            .ok_or_else(|| CoreError::InvalidPath("MCP path has no existing ancestor".into()))?;
        missing.push(name.to_os_string());
        cursor = cursor
            .parent()
            .ok_or_else(|| CoreError::InvalidPath("MCP path has no parent".into()))?;
    }
    let mut resolved = fs::canonicalize(cursor)?;
    for component in missing.into_iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

pub fn client_config_path(home: &Path, client: &McpClientName) -> PathBuf {
    match client {
        McpClientName::Codex => home.join(".codex").join("config.toml"),
        McpClientName::ClaudeCode => home.join(".claude.json"),
        McpClientName::Cursor => home.join(".cursor").join("mcp.json"),
    }
}

pub fn config_digest(path: &Path) -> Result<Option<String>, CoreError> {
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(sha256(&read_bounded(path)?)))
}

fn server_for_target(server: &McpServerRecord, target: &McpConfigTarget) -> McpServerRecord {
    let Some(binding) = server
        .clients
        .iter()
        .find(|binding| binding.client == target.client && !binding.command_or_url.is_empty())
    else {
        return server.clone();
    };
    binding.project_server(server)
}

fn mutate_config(
    original: &[u8],
    target: &McpConfigTarget,
    server: &McpServerRecord,
    action: &McpMutationAction,
) -> Result<Vec<u8>, CoreError> {
    match target.client {
        McpClientName::Codex => mutate_toml_config(original, target, server, action),
        McpClientName::ClaudeCode | McpClientName::Cursor => {
            mutate_json_config(original, target, server, action)
        }
    }
}

fn mutate_toml_config(
    original: &[u8],
    target: &McpConfigTarget,
    server: &McpServerRecord,
    action: &McpMutationAction,
) -> Result<Vec<u8>, CoreError> {
    let text = std::str::from_utf8(original)
        .map_err(|_| CoreError::UnsupportedSchema("Codex config is not UTF-8".into()))?;
    let mut root = if text.trim().is_empty() {
        TomlValue::Table(Default::default())
    } else {
        toml::from_str(text)?
    };
    let table = root
        .as_table_mut()
        .ok_or_else(|| CoreError::UnsupportedSchema("Codex config root is not a table".into()))?;
    let servers = table
        .entry("mcp_servers")
        .or_insert_with(|| TomlValue::Table(Default::default()))
        .as_table_mut()
        .ok_or_else(|| CoreError::UnsupportedSchema("Codex mcp_servers is not a table".into()))?;
    mutate_toml_entries(servers, target, server, action)?;
    if servers.is_empty() && table.len() == 1 {
        return Ok(Vec::new());
    }
    toml::to_string_pretty(&root)
        .map(String::into_bytes)
        .map_err(|error| {
            CoreError::MalformedInput(format!("MCP TOML serialization failed: {error}"))
        })
}

fn mutate_toml_entries(
    servers: &mut toml::map::Map<String, TomlValue>,
    target: &McpConfigTarget,
    server: &McpServerRecord,
    action: &McpMutationAction,
) -> Result<(), CoreError> {
    let key = matching_key(servers.keys(), &target.entry_name)
        .unwrap_or_else(|| target.entry_name.clone());
    if *action == McpMutationAction::Remove {
        servers.remove(&key);
        return Ok(());
    }
    let entry = servers
        .entry(key)
        .or_insert_with(|| TomlValue::Table(Default::default()))
        .as_table_mut()
        .ok_or_else(|| CoreError::UnsupportedSchema("MCP entry is not a table".into()))?;
    mutate_toml_entry(entry, server, action)
}

fn mutate_toml_entry(
    entry: &mut toml::map::Map<String, TomlValue>,
    server: &McpServerRecord,
    action: &McpMutationAction,
) -> Result<(), CoreError> {
    match action {
        McpMutationAction::Add | McpMutationAction::Update => write_toml_endpoint(entry, server)?,
        McpMutationAction::Enable => {
            entry.insert("enabled".into(), TomlValue::Boolean(true));
            entry.remove("disabled");
        }
        McpMutationAction::Disable => {
            entry.insert("enabled".into(), TomlValue::Boolean(false));
            entry.remove("disabled");
        }
        McpMutationAction::Remove => unreachable!(),
    }
    Ok(())
}

fn write_toml_endpoint(
    entry: &mut toml::map::Map<String, TomlValue>,
    server: &McpServerRecord,
) -> Result<(), CoreError> {
    match server.transport {
        McpTransport::StreamableHttp | McpTransport::Sse => {
            validate_remote_endpoint(&server.command_or_url)?;
            entry.remove("command");
            entry.remove("args");
            entry.insert(
                "url".into(),
                TomlValue::String(server.command_or_url.clone()),
            );
            entry.insert(
                "transport".into(),
                TomlValue::String(transport_label(&server.transport).into()),
            );
        }
        McpTransport::Stdio => {
            entry.remove("url");
            entry.insert(
                "command".into(),
                TomlValue::String(server.command_or_url.clone()),
            );
            entry.insert(
                "args".into(),
                TomlValue::Array(server.args.iter().cloned().map(TomlValue::String).collect()),
            );
            entry.insert("transport".into(), TomlValue::String("stdio".into()));
        }
    }
    entry.insert(
        "capabilities".into(),
        TomlValue::Array(
            server
                .capabilities
                .iter()
                .cloned()
                .map(TomlValue::String)
                .collect(),
        ),
    );
    write_toml_auth_references(entry, server)?;
    Ok(())
}

fn write_toml_auth_references(
    entry: &mut toml::map::Map<String, TomlValue>,
    server: &McpServerRecord,
) -> Result<(), CoreError> {
    for key in [
        "env",
        "headers",
        "tokenAlias",
        "token_alias",
        "authFile",
        "auth_file",
        "authRequired",
    ] {
        entry.remove(key);
    }
    let mut environment = toml::map::Map::new();
    for reference in &server.auth_references {
        match reference.kind {
            AuthReferenceKind::EnvVar => {
                environment.insert(
                    reference.reference.clone(),
                    TomlValue::String(format!("${{{}}}", reference.reference)),
                );
            }
            AuthReferenceKind::TokenAlias => {
                entry.insert(
                    "tokenAlias".into(),
                    TomlValue::String(reference.reference.clone()),
                );
            }
            AuthReferenceKind::FileReference => {
                entry.insert(
                    "authFile".into(),
                    TomlValue::String(reference.reference.clone()),
                );
            }
            AuthReferenceKind::HeaderName => {
                return Err(CoreError::CommandDenied(
                    "MCP header-name references require a credential source binding".into(),
                ));
            }
        }
    }
    if !environment.is_empty() {
        entry.insert("env".into(), TomlValue::Table(environment));
    }
    entry.insert(
        "authRequired".into(),
        TomlValue::Boolean(server.auth_required),
    );
    Ok(())
}

fn mutate_json_config(
    original: &[u8],
    target: &McpConfigTarget,
    server: &McpServerRecord,
    action: &McpMutationAction,
) -> Result<Vec<u8>, CoreError> {
    let mut root = if original.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_slice::<JsonValue>(original)?
    };
    let root_object = root.as_object_mut().ok_or_else(|| {
        CoreError::UnsupportedSchema("MCP JSON config root is not an object".into())
    })?;
    let servers = root_object
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| CoreError::UnsupportedSchema("mcpServers is not an object".into()))?;
    let key = matching_key(servers.keys(), &target.entry_name)
        .unwrap_or_else(|| target.entry_name.clone());
    if *action == McpMutationAction::Remove {
        servers.remove(&key);
    } else {
        let entry = servers
            .entry(key)
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| CoreError::UnsupportedSchema("MCP entry is not an object".into()))?;
        match action {
            McpMutationAction::Add | McpMutationAction::Update => {
                write_json_endpoint(entry, server)?
            }
            McpMutationAction::Enable => {
                entry.insert("enabled".into(), JsonValue::Bool(true));
                entry.remove("disabled");
            }
            McpMutationAction::Disable => {
                entry.insert("enabled".into(), JsonValue::Bool(false));
                entry.remove("disabled");
            }
            McpMutationAction::Remove => unreachable!(),
        }
    }
    if servers.is_empty() && root_object.len() == 1 {
        return Ok(Vec::new());
    }
    serde_json::to_vec_pretty(&root).map_err(CoreError::Json)
}

fn write_json_endpoint(
    entry: &mut serde_json::Map<String, JsonValue>,
    server: &McpServerRecord,
) -> Result<(), CoreError> {
    match server.transport {
        McpTransport::StreamableHttp | McpTransport::Sse => {
            validate_remote_endpoint(&server.command_or_url)?;
            entry.remove("command");
            entry.remove("args");
            entry.insert(
                "url".into(),
                JsonValue::String(server.command_or_url.clone()),
            );
            entry.insert(
                "transport".into(),
                JsonValue::String(transport_label(&server.transport).into()),
            );
        }
        McpTransport::Stdio => {
            entry.remove("url");
            entry.insert(
                "command".into(),
                JsonValue::String(server.command_or_url.clone()),
            );
            entry.insert(
                "args".into(),
                JsonValue::Array(server.args.iter().cloned().map(JsonValue::String).collect()),
            );
            entry.insert("transport".into(), JsonValue::String("stdio".into()));
        }
    }
    entry.insert(
        "capabilities".into(),
        JsonValue::Array(
            server
                .capabilities
                .iter()
                .cloned()
                .map(JsonValue::String)
                .collect(),
        ),
    );
    write_json_auth_references(entry, server)?;
    Ok(())
}

fn write_json_auth_references(
    entry: &mut serde_json::Map<String, JsonValue>,
    server: &McpServerRecord,
) -> Result<(), CoreError> {
    for key in [
        "env",
        "headers",
        "tokenAlias",
        "token_alias",
        "authFile",
        "auth_file",
        "authRequired",
    ] {
        entry.remove(key);
    }
    let mut environment = serde_json::Map::new();
    for reference in &server.auth_references {
        match reference.kind {
            AuthReferenceKind::EnvVar => {
                environment.insert(
                    reference.reference.clone(),
                    JsonValue::String(format!("${{{}}}", reference.reference)),
                );
            }
            AuthReferenceKind::TokenAlias => {
                entry.insert(
                    "tokenAlias".into(),
                    JsonValue::String(reference.reference.clone()),
                );
            }
            AuthReferenceKind::FileReference => {
                entry.insert(
                    "authFile".into(),
                    JsonValue::String(reference.reference.clone()),
                );
            }
            AuthReferenceKind::HeaderName => {
                return Err(CoreError::CommandDenied(
                    "MCP header-name references require a credential source binding".into(),
                ));
            }
        }
    }
    if !environment.is_empty() {
        entry.insert("env".into(), JsonValue::Object(environment));
    }
    entry.insert("authRequired".into(), JsonValue::Bool(server.auth_required));
    Ok(())
}

fn matching_key<'a>(mut keys: impl Iterator<Item = &'a String>, name: &str) -> Option<String> {
    keys.find(|key| key.eq_ignore_ascii_case(name)).cloned()
}

fn transport_label(transport: &McpTransport) -> &'static str {
    match transport {
        McpTransport::Stdio => "stdio",
        McpTransport::StreamableHttp => "streamable_http",
        McpTransport::Sse => "sse",
    }
}

fn validate_remote_endpoint(raw: &str) -> Result<(), CoreError> {
    let url = url::Url::parse(raw)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(CoreError::CommandDenied(
            "MCP endpoint must be credential-free HTTPS without query or fragment".into(),
        ));
    }
    Ok(())
}

fn health_label(health: &McpHealthState) -> &'static str {
    match health {
        McpHealthState::Healthy => "healthy",
        McpHealthState::Degraded => "degraded",
        McpHealthState::Unreachable => "unreachable",
        McpHealthState::Unknown => "unknown",
    }
}

struct ConfigScopeLocks(Vec<fs::File>);

impl Drop for ConfigScopeLocks {
    fn drop(&mut self) {
        for file in &self.0 {
            let _ = FileExt::unlock(file);
        }
    }
}

fn lock_config_scopes<'a>(
    paths: impl IntoIterator<Item = &'a Path>,
) -> Result<ConfigScopeLocks, CoreError> {
    let lock_paths = paths
        .into_iter()
        .map(|path| {
            path.parent()
                .ok_or_else(|| CoreError::InvalidPath("MCP config has no parent".into()))
                .map(|parent| parent.join(".stm-mcp-config.lock"))
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let mut files = Vec::with_capacity(lock_paths.len());
    for lock_path in lock_paths {
        if lock_path.exists() && fs::symlink_metadata(&lock_path)?.file_type().is_symlink() {
            return Err(CoreError::PathEscape(lock_path));
        }
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&lock_path)?;
        file.lock_exclusive()?;
        files.push(file);
    }
    Ok(ConfigScopeLocks(files))
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, CoreError> {
    read_bounded_with_limit(path, MAX_CONFIG_BYTES)
}

fn read_bounded_with_limit(path: &Path, limit: u64) -> Result<Vec<u8>, CoreError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CoreError::PathEscape(path.to_path_buf()));
    }
    if metadata.len() > limit {
        return Err(CoreError::MalformedInput(
            "MCP configuration exceeds the bounded read limit".into(),
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(CoreError::MalformedInput(
            "MCP configuration exceeds the bounded read limit".into(),
        ));
    }
    Ok(bytes)
}

fn replace_from_bytes(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::InvalidPath("MCP config has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".stm-mcp-write-{}",
        short_id(&path.display().to_string())
    ));
    remove_file_if_exists(&temporary)?;
    write_private_file(&temporary, bytes)?;
    let replaced = replace_temporary_file(&temporary, path);
    if let Err(error) = replaced {
        let _ = remove_file_if_exists(&temporary);
        return Err(error.into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_temporary_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    fs::rename(temporary, path)
}

#[cfg(windows)]
fn replace_temporary_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    let temporary_wide = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let path_wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replaced = unsafe {
        if path.exists() {
            ReplaceFileW(
                path_wide.as_ptr(),
                temporary_wide.as_ptr(),
                std::ptr::null(),
                REPLACEFILE_WRITE_THROUGH,
                std::ptr::null(),
                std::ptr::null(),
            )
        } else {
            MoveFileExW(
                temporary_wide.as_ptr(),
                path_wide.as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        }
    };
    if replaced == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn remove_file_if_exists(path: &Path) -> Result<(), CoreError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn short_id(value: &str) -> String {
    sha256(value.as_bytes())[..20].to_string()
}
