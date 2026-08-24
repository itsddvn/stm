pub mod lifecycle;
pub mod policy;

use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use toml::Value as TomlValue;
use url::Url;

use crate::{
    application::adapters::FixtureWorkspace,
    domain::{
        inventory::InventoryState,
        mcp::{
            AuthReference, AuthReferenceKind, AuthReferenceState, MalformedMcpEntry,
            McpBindingScope, McpBindingState, McpClientBindingRecord, McpClientName,
            McpDiscoveryReport, McpHealthState, McpServerRecord, McpTransport, McpTrustState,
        },
    },
    error::CoreError,
};

const MAX_MCP_CONFIG_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpHealthEvidence {
    pub id: String,
    pub health: McpHealthState,
    pub last_checked: String,
}

#[derive(Debug, Clone)]
pub struct McpInventorySnapshot {
    pub servers: Vec<McpServerRecord>,
    pub report: McpDiscoveryReport,
}

#[derive(Debug, Clone)]
struct ParsedServer {
    server: McpServerRecord,
    binding_state: McpBindingState,
}

pub fn discover_mcp(workspace: &FixtureWorkspace) -> Result<McpInventorySnapshot, CoreError> {
    if workspace.has_skill_home_override() {
        return discover_runtime_mcp(workspace);
    }
    let health: Vec<McpHealthEvidence> = workspace.read_json("tests/fixtures/mcp/health.json")?;
    let health_by_id = health
        .into_iter()
        .map(|item| (item.id.clone(), item))
        .collect::<BTreeMap<_, _>>();

    let reports = [
        parse_codex_config(&workspace.resolve("tests/fixtures/mcp/codex/config.toml"))?,
        parse_json_client_config(
            McpClientName::ClaudeCode,
            &workspace.resolve("tests/fixtures/mcp/claude-code/config.json"),
        )?,
        parse_json_client_config(
            McpClientName::Cursor,
            &workspace.resolve("tests/fixtures/mcp/cursor/mcp.json"),
        )?,
    ];
    let unsupported = parse_unsupported_schema(
        McpClientName::Cursor,
        &workspace.resolve("tests/fixtures/mcp/cursor/unsupported-config.json"),
    )?;

    let mut report = merge_reports(reports);
    if let Some(warning) = unsupported {
        report.warnings.push(warning);
    }

    for server in &mut report.servers {
        if let Some(health) = health_by_id.get(&server.id) {
            server.health = health.health.clone();
            server.last_checked = health.last_checked.clone();
        }
        if server.auth_state == AuthReferenceState::ReferenceMissing {
            server.state = InventoryState::Blocked;
            server.trust = McpTrustState::Blocked;
        } else if server.health == McpHealthState::Degraded {
            server.state = InventoryState::SourceUnavailable;
        }
    }

    Ok(McpInventorySnapshot {
        servers: report.servers.clone(),
        report,
    })
}

fn discover_runtime_mcp(workspace: &FixtureWorkspace) -> Result<McpInventorySnapshot, CoreError> {
    let home = fs::canonicalize(workspace.skill_home()?)?;
    let sources = [
        (
            McpClientName::Codex,
            home.join(".codex").join("config.toml"),
            true,
        ),
        (McpClientName::ClaudeCode, home.join(".claude.json"), false),
        (
            McpClientName::Cursor,
            home.join(".cursor").join("mcp.json"),
            false,
        ),
    ];
    let mut reports = Vec::new();
    let mut warnings = Vec::new();
    for (client, path, is_toml) in sources {
        if !path.exists() {
            continue;
        }
        let resolved = match validated_runtime_config_path(&home, &path) {
            Ok(path) => path,
            Err(error) => {
                warnings.push(format!(
                    "{} MCP configuration was rejected: {error}",
                    client_label(&client)
                ));
                continue;
            }
        };
        let parsed = if is_toml {
            parse_codex_config(&resolved)
        } else {
            parse_json_client_config(client.clone(), &resolved)
        };
        match parsed {
            Ok(report) => reports.push(report),
            Err(error) => warnings.push(format!(
                "{} MCP configuration could not be parsed: {error}",
                client_label(&client)
            )),
        }
    }
    let mut report = merge_reports(reports);
    report.warnings.extend(warnings);
    for server in &mut report.servers {
        server.health = McpHealthState::Unknown;
        server.last_checked = "not_checked".to_string();
        if server.auth_state == AuthReferenceState::ReferenceMissing {
            server.state = InventoryState::Blocked;
            server.trust = McpTrustState::Blocked;
        }
    }
    Ok(McpInventorySnapshot {
        servers: report.servers.clone(),
        report,
    })
}

pub fn parse_codex_config(path: &Path) -> Result<McpDiscoveryReport, CoreError> {
    let raw = read_bounded_config(path)?;
    let value: TomlValue = toml::from_str(&raw)?;
    let table = value
        .get("mcp_servers")
        .and_then(TomlValue::as_table)
        .ok_or_else(|| CoreError::UnsupportedSchema("Codex mcp_servers missing".to_string()))?;
    Ok(parse_entries(
        McpClientName::Codex,
        table
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
    ))
}

pub fn parse_json_client_config(
    client: McpClientName,
    path: &Path,
) -> Result<McpDiscoveryReport, CoreError> {
    let raw = read_bounded_config(path)?;
    let value: JsonValue = serde_json::from_str(&raw)?;
    let table = value
        .get("mcpServers")
        .and_then(JsonValue::as_object)
        .ok_or_else(|| CoreError::UnsupportedSchema("mcpServers missing".to_string()))?;
    Ok(parse_entries(
        client,
        table
            .iter()
            .map(|(name, value)| (name.clone(), toml_like(value)))
            .collect(),
    ))
}

fn parse_unsupported_schema(
    client: McpClientName,
    path: &Path,
) -> Result<Option<String>, CoreError> {
    let raw = read_bounded_config(path)?;
    let value: JsonValue = serde_json::from_str(&raw)?;
    if value.get("mcpServers").is_none() {
        return Ok(Some(format!(
            "{} config uses unsupported schema root",
            client_label(&client)
        )));
    }
    Ok(None)
}

fn validated_runtime_config_path(home: &Path, path: &Path) -> Result<PathBuf, CoreError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_MCP_CONFIG_BYTES
    {
        return Err(CoreError::PathEscape(path.to_path_buf()));
    }
    let resolved = fs::canonicalize(path)?;
    if !resolved.starts_with(home) {
        return Err(CoreError::PathEscape(path.to_path_buf()));
    }
    Ok(resolved)
}

fn read_bounded_config(path: &Path) -> Result<String, CoreError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_MCP_CONFIG_BYTES
    {
        return Err(CoreError::PathEscape(path.to_path_buf()));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(path)?
        .take(MAX_MCP_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_MCP_CONFIG_BYTES {
        return Err(CoreError::PathEscape(path.to_path_buf()));
    }
    String::from_utf8(bytes)
        .map_err(|_| CoreError::UnsupportedSchema("MCP config is not UTF-8".into()))
}

fn merge_reports(reports: impl IntoIterator<Item = McpDiscoveryReport>) -> McpDiscoveryReport {
    let mut logical = BTreeMap::<String, McpServerRecord>::new();
    let mut malformed_entries = Vec::new();
    let mut warnings = Vec::new();

    for report in reports {
        malformed_entries.extend(report.malformed_entries);
        warnings.extend(report.warnings);
        for server in report.servers {
            let key = server.id.clone();
            logical
                .entry(key)
                .and_modify(|existing| {
                    for binding in &server.clients {
                        if let Some(current) = existing
                            .clients
                            .iter()
                            .find(|item| item.client == binding.client)
                        {
                            if current != binding {
                                existing.state = InventoryState::Conflict;
                                existing.trust = McpTrustState::Blocked;
                                warnings.push(format!(
                                    "{} has conflicting {} client entries",
                                    existing.id,
                                    client_label(&binding.client)
                                ));
                            }
                        } else {
                            existing.clients.push(binding.clone());
                        }
                    }
                    existing.auth_required |= server.auth_required;
                    existing.auth_state =
                        merge_auth_reference_state(&existing.auth_state, &server.auth_state);
                    existing.trust = merge_trust_state(&existing.trust, &server.trust);
                    existing.health = merge_health_state(&existing.health, &server.health);
                    if server.last_checked > existing.last_checked {
                        existing.last_checked = server.last_checked.clone();
                    }
                    for capability in &server.capabilities {
                        if !existing.capabilities.contains(capability) {
                            existing.capabilities.push(capability.clone());
                        }
                    }
                    if existing.state != InventoryState::Conflict
                        && (existing.auth_state == AuthReferenceState::ReferenceMissing
                            || existing.trust == McpTrustState::Blocked)
                    {
                        existing.state = InventoryState::Blocked;
                    }
                })
                .or_insert(server);
        }
    }

    McpDiscoveryReport {
        servers: logical.into_values().collect(),
        malformed_entries,
        warnings,
    }
}

fn merge_auth_reference_state(
    existing: &AuthReferenceState,
    incoming: &AuthReferenceState,
) -> AuthReferenceState {
    match (existing, incoming) {
        (AuthReferenceState::ReferenceMissing, _) | (_, AuthReferenceState::ReferenceMissing) => {
            AuthReferenceState::ReferenceMissing
        }
        (AuthReferenceState::ReferenceConfigured, _)
        | (_, AuthReferenceState::ReferenceConfigured) => AuthReferenceState::ReferenceConfigured,
        _ => AuthReferenceState::None,
    }
}

fn merge_trust_state(existing: &McpTrustState, incoming: &McpTrustState) -> McpTrustState {
    match (existing, incoming) {
        (McpTrustState::Blocked, _) | (_, McpTrustState::Blocked) => McpTrustState::Blocked,
        (McpTrustState::ReviewRequired, _) | (_, McpTrustState::ReviewRequired) => {
            McpTrustState::ReviewRequired
        }
        _ => McpTrustState::Verified,
    }
}

fn merge_health_state(existing: &McpHealthState, incoming: &McpHealthState) -> McpHealthState {
    match (existing, incoming) {
        (McpHealthState::Unreachable, _) | (_, McpHealthState::Unreachable) => {
            McpHealthState::Unreachable
        }
        (McpHealthState::Degraded, _) | (_, McpHealthState::Degraded) => McpHealthState::Degraded,
        (McpHealthState::Unknown, _) | (_, McpHealthState::Unknown) => McpHealthState::Unknown,
        _ => McpHealthState::Healthy,
    }
}

fn parse_entries(client: McpClientName, entries: Vec<(String, TomlValue)>) -> McpDiscoveryReport {
    let mut logical = BTreeMap::<String, McpServerRecord>::new();
    let mut malformed_entries = Vec::new();
    let mut duplicates = 0_usize;

    for (name, value) in entries {
        let parsed = match parse_server(&name, &value) {
            Ok(parsed) => parsed,
            Err(reason) => {
                malformed_entries.push(MalformedMcpEntry {
                    client: client.clone(),
                    entry_name: name,
                    reason,
                });
                continue;
            }
        };
        let binding = McpClientBindingRecord::from_server(
            client.clone(),
            parsed.binding_state.clone(),
            McpBindingScope::Global,
            parsed.server.name.clone(),
            &parsed.server,
        );

        let key = logical_key(&parsed.server);
        logical
            .entry(key)
            .and_modify(|existing| {
                duplicates += 1;
                if let Some(binding) = existing
                    .clients
                    .iter_mut()
                    .find(|binding| binding.client == client)
                {
                    binding.state = merge_binding_state(&binding.state, &parsed.binding_state);
                } else {
                    existing.clients.push(binding.clone());
                }
            })
            .or_insert_with(|| {
                let mut server = parsed.server;
                server.clients = vec![binding];
                server
            });
    }

    McpDiscoveryReport {
        servers: logical.into_values().collect(),
        malformed_entries,
        warnings: if duplicates == 0 {
            Vec::new()
        } else {
            vec![format!(
                "deduplicated {duplicates} logical {} binding(s)",
                client_label(&client)
            )]
        },
    }
}

fn parse_server(name: &str, value: &TomlValue) -> Result<ParsedServer, String> {
    let table = value
        .as_table()
        .ok_or_else(|| "entry is not an object".to_string())?;
    let command = table.get("command").and_then(TomlValue::as_str);
    let url = table.get("url").and_then(TomlValue::as_str);
    let transport = normalize_transport(
        table.get("transport").and_then(TomlValue::as_str),
        url.is_some(),
    )
    .ok_or_else(|| "unsupported transport or missing command/url".to_string())?;
    let raw_command_or_url = command
        .or(url)
        .ok_or_else(|| "entry missing command/url".to_string())?;
    let command_or_url = if url.is_some() {
        redacted_url(raw_command_or_url)
    } else {
        raw_command_or_url.to_string()
    };
    let args = table
        .get("args")
        .and_then(TomlValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(TomlValue::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let capabilities = table
        .get("capabilities")
        .and_then(TomlValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(TomlValue::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec!["tools".to_string()]);
    validate_credential_references(table)?;
    policy::validate_inventory_fields(&transport, raw_command_or_url, &args, &capabilities)?;
    let trust = policy::trust_state(name, &transport, &command_or_url, &args, &capabilities);
    let auth_references = collect_auth_references(table);
    let requires_auth = table
        .get("authRequired")
        .and_then(TomlValue::as_bool)
        .unwrap_or(false);
    let auth_state = if auth_references.is_empty() && requires_auth {
        AuthReferenceState::ReferenceMissing
    } else if auth_references.is_empty() {
        AuthReferenceState::None
    } else if policy::auth_references_available(&auth_references) {
        AuthReferenceState::ReferenceConfigured
    } else {
        AuthReferenceState::ReferenceMissing
    };

    Ok(ParsedServer {
        binding_state: if entry_disabled(table) {
            McpBindingState::Disabled
        } else {
            McpBindingState::Enabled
        },
        server: McpServerRecord {
            id: policy::approved_mapping_id(
                name,
                &transport,
                &command_or_url,
                &args,
                &capabilities,
            )
            .unwrap_or_else(|| canonical_server_id(name)),
            name: name.to_string(),
            description: format!("{name} configuration"),
            source: "fixture".to_string(),
            transport,
            command_or_url,
            args,
            auth_references: auth_references.clone(),
            clients: Vec::new(),
            capabilities,
            trust,
            auth_state,
            health: McpHealthState::Unknown,
            last_checked: "2026-08-20T00:00:00Z".to_string(),
            auth_required: requires_auth,
            state: InventoryState::ManagedCurrent,
        },
    })
}

fn normalize_transport(raw: Option<&str>, has_url: bool) -> Option<McpTransport> {
    match raw.unwrap_or(if has_url { "streamable_http" } else { "stdio" }) {
        "stdio" => Some(McpTransport::Stdio),
        "sse" => Some(McpTransport::Sse),
        "http" | "streamable_http" | "streamable-http" => Some(McpTransport::StreamableHttp),
        _ => None,
    }
}

fn logical_key(server: &McpServerRecord) -> String {
    format!(
        "{:?}|{}|{}",
        server.transport,
        server.command_or_url,
        server.args.join("\u{1f}")
    )
}

fn collect_auth_references(table: &toml::map::Map<String, TomlValue>) -> Vec<AuthReference> {
    let mut references = Vec::new();

    if let Some(env) = table.get("env").and_then(TomlValue::as_table) {
        references.extend(
            env.values()
                .filter_map(TomlValue::as_str)
                .filter_map(|value| {
                    reference_name(value).map(|reference| AuthReference {
                        kind: AuthReferenceKind::EnvVar,
                        reference,
                    })
                }),
        );
    }

    if let Some(headers) = table.get("headers").and_then(TomlValue::as_table) {
        references.extend(headers.iter().filter_map(|(key, value)| {
            if !sensitive_credential_key(key) {
                return None;
            }
            value
                .as_str()
                .and_then(reference_name)
                .map(|reference| AuthReference {
                    kind: AuthReferenceKind::EnvVar,
                    reference,
                })
        }));
    }

    for key in ["tokenAlias", "token_alias"] {
        if let Some(value) = table.get(key).and_then(TomlValue::as_str) {
            references.push(AuthReference {
                kind: AuthReferenceKind::TokenAlias,
                reference: value.to_string(),
            });
        }
    }
    for key in ["authFile", "auth_file"] {
        if let Some(value) = table.get(key).and_then(TomlValue::as_str) {
            references.push(AuthReference {
                kind: AuthReferenceKind::FileReference,
                reference: value.to_string(),
            });
        }
    }

    references.sort_by(|left, right| {
        format!("{:?}:{}", left.kind, left.reference)
            .cmp(&format!("{:?}:{}", right.kind, right.reference))
    });
    references.dedup();
    references
}

fn validate_credential_references(table: &toml::map::Map<String, TomlValue>) -> Result<(), String> {
    if let Some(env) = table.get("env").and_then(TomlValue::as_table) {
        if env
            .values()
            .any(|value| value.as_str().and_then(reference_name).is_none())
        {
            return Err("MCP environment values must use credential references".into());
        }
    }
    if let Some(headers) = table.get("headers").and_then(TomlValue::as_table) {
        if headers.iter().any(|(key, value)| {
            sensitive_credential_key(key) && value.as_str().and_then(reference_name).is_none()
        }) {
            return Err("MCP credential headers must use environment references".into());
        }
    }
    Ok(())
}

fn reference_name(value: &str) -> Option<String> {
    value
        .strip_prefix("${")
        .and_then(|value| value.strip_suffix('}'))
        .or_else(|| value.strip_prefix("$env:"))
        .filter(|value| {
            !value.is_empty()
                && value
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
        })
        .map(ToOwned::to_owned)
}

fn sensitive_credential_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    [
        "authorization",
        "cookie",
        "credential",
        "password",
        "secret",
        "token",
        "api-key",
        "api_key",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn merge_binding_state(existing: &McpBindingState, incoming: &McpBindingState) -> McpBindingState {
    match (existing, incoming) {
        (McpBindingState::Enabled, _) | (_, McpBindingState::Enabled) => McpBindingState::Enabled,
        (McpBindingState::Disabled, _) | (_, McpBindingState::Disabled) => {
            McpBindingState::Disabled
        }
        _ => McpBindingState::Unsupported,
    }
}

fn entry_disabled(table: &toml::map::Map<String, TomlValue>) -> bool {
    table.get("disabled").and_then(TomlValue::as_bool) == Some(true)
        || table.get("enabled").and_then(TomlValue::as_bool) == Some(false)
}

fn canonical_server_id(name: &str) -> String {
    name.to_ascii_lowercase().replace(' ', "-")
}

fn client_label(client: &McpClientName) -> &'static str {
    match client {
        McpClientName::Codex => "Codex",
        McpClientName::ClaudeCode => "Claude Code",
        McpClientName::Cursor => "Cursor",
    }
}

fn redacted_url(raw: &str) -> String {
    match Url::parse(raw) {
        Ok(mut url) => {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            url.set_query(None);
            url.set_fragment(None);
            url.to_string()
        }
        Err(_) => raw.to_string(),
    }
}

fn toml_like(value: &JsonValue) -> TomlValue {
    match value {
        JsonValue::Null => TomlValue::String(String::new()),
        JsonValue::Bool(value) => TomlValue::Boolean(*value),
        JsonValue::Number(value) => TomlValue::String(value.to_string()),
        JsonValue::String(value) => TomlValue::String(value.clone()),
        JsonValue::Array(values) => TomlValue::Array(values.iter().map(toml_like).collect()),
        JsonValue::Object(values) => TomlValue::Table(
            values
                .iter()
                .map(|(key, value)| (key.clone(), toml_like(value)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;

    fn workspace() -> FixtureWorkspace {
        FixtureWorkspace::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
    }

    #[test]
    fn discovers_normalized_servers_and_rejects_secret_bearing_endpoints() {
        let snapshot = discover_mcp(&workspace()).expect("mcp");
        assert!(snapshot.servers.iter().any(|server| server.id == "github"));
        let github = snapshot
            .servers
            .iter()
            .find(|server| server.id == "github")
            .expect("github");
        assert!(github.clients.iter().all(|binding| {
            binding.transport == McpTransport::StreamableHttp
                && binding.command_or_url == "https://api.githubcopilot.com/mcp/"
                && binding.capabilities == vec!["tools", "prompts"]
                && !binding.auth_references.is_empty()
        }));
        assert!(!snapshot
            .servers
            .iter()
            .any(|server| server.id == "postgres"));
        assert!(snapshot
            .report
            .malformed_entries
            .iter()
            .any(|entry| entry.entry_name.eq_ignore_ascii_case("postgres")));
        assert!(snapshot
            .report
            .malformed_entries
            .iter()
            .any(|entry| entry.entry_name == "Broken Entry"));
        assert!(snapshot
            .report
            .warnings
            .iter()
            .any(|warning| warning.contains("unsupported schema")));
    }

    #[test]
    fn canonical_mapping_identity_merges_aliases_and_retains_entry_names() {
        let temp = tempfile::TempDir::new().expect("tempdir");
        let codex_path = temp.path().join("config.toml");
        let claude_path = temp.path().join("claude.json");
        fs::write(
            &codex_path,
            "[mcp_servers.filesystem]\ncommand = \"npx\"\nargs = [\"-y\", \"@modelcontextprotocol/server-filesystem\", \"/tmp\"]\ncapabilities = [\"resources\", \"tools\"]\n",
        )
        .expect("codex config");
        fs::write(
            &claude_path,
            r#"{"mcpServers":{"server-filesystem":{"command":"npx","args":["-y","@modelcontextprotocol/server-filesystem","/tmp"],"capabilities":["resources","tools"]}}}"#,
        )
        .expect("claude config");

        let report = merge_reports([
            parse_codex_config(&codex_path).expect("codex"),
            parse_json_client_config(McpClientName::ClaudeCode, &claude_path).expect("claude"),
        ]);

        assert_eq!(report.servers.len(), 1);
        assert_eq!(report.servers[0].id, "filesystem");
        assert_eq!(report.servers[0].clients.len(), 2);
        assert!(report.servers[0]
            .clients
            .iter()
            .any(|binding| binding.entry_name == "filesystem"));
        assert!(report.servers[0]
            .clients
            .iter()
            .any(|binding| binding.entry_name == "server-filesystem"));
    }

    #[test]
    fn logical_identity_retains_mixed_transports_and_missing_auth_per_binding() {
        let temp = tempfile::TempDir::new().expect("tempdir");
        let codex_path = temp.path().join("config.toml");
        let claude_path = temp.path().join("claude.json");
        fs::write(
            &codex_path,
            "[mcp_servers.Mixed]\ncommand = \"mixed-mcp\"\ncapabilities = [\"tools\"]\n",
        )
        .expect("codex config");
        fs::write(
            &claude_path,
            r#"{"mcpServers":{"Mixed":{"url":"https://mcp.invalid.test/service","transport":"streamable_http","capabilities":["tools"],"authRequired":true}}}"#,
        )
        .expect("claude config");

        let report = merge_reports([
            parse_codex_config(&codex_path).expect("codex"),
            parse_json_client_config(McpClientName::ClaudeCode, &claude_path).expect("claude"),
        ]);

        assert_eq!(report.servers.len(), 1);
        let server = &report.servers[0];
        assert_eq!(server.clients.len(), 2);
        assert_eq!(server.auth_state, AuthReferenceState::ReferenceMissing);
        assert!(server.auth_required);
        assert_eq!(server.state, InventoryState::Blocked);
        assert!(server
            .clients
            .iter()
            .any(|binding| binding.transport == McpTransport::Stdio));
        let remote = server
            .clients
            .iter()
            .find(|binding| binding.transport == McpTransport::StreamableHttp)
            .expect("remote binding");
        assert!(remote.auth_required);
        assert_eq!(remote.auth_state, AuthReferenceState::ReferenceMissing);
    }
    #[test]
    fn config_reads_reject_oversized_files_before_parsing() {
        let temp = tempfile::TempDir::new().expect("tempdir");
        let path = temp.path().join("oversized.json");
        let file = fs::File::create(&path).expect("config");
        file.set_len(MAX_MCP_CONFIG_BYTES + 1).expect("oversized");

        let error = parse_json_client_config(McpClientName::Cursor, &path)
            .expect_err("oversized config must fail");

        assert!(matches!(error, CoreError::PathEscape(_)));
    }

    #[cfg(unix)]
    #[test]
    fn runtime_discovery_rejects_parent_symlinks_that_escape_home() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&home).expect("home");
        fs::create_dir_all(&outside).expect("outside");
        fs::write(
            outside.join("config.toml"),
            "[mcp_servers.Filesystem]\ncommand = \"npx\"\n",
        )
        .expect("outside config");
        symlink(&outside, home.join(".codex")).expect("parent symlink");
        let workspace = FixtureWorkspace::new(temp.path()).with_skill_home(&home);

        let snapshot = discover_mcp(&workspace).expect("bounded discovery");

        assert!(snapshot.servers.is_empty());
        assert!(snapshot
            .report
            .warnings
            .iter()
            .any(|warning| warning.contains("path escapes approved root")));
    }
}
