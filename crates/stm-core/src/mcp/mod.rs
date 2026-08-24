use std::{collections::BTreeMap, fs, path::Path};

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

pub fn parse_codex_config(path: &Path) -> Result<McpDiscoveryReport, CoreError> {
    let raw = fs::read_to_string(path)?;
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
    let raw = fs::read_to_string(path)?;
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
    let raw = fs::read_to_string(path)?;
    let value: JsonValue = serde_json::from_str(&raw)?;
    if value.get("mcpServers").is_none() {
        return Ok(Some(format!(
            "{} config uses unsupported schema root",
            client_label(&client)
        )));
    }
    Ok(None)
}

fn merge_reports(reports: impl IntoIterator<Item = McpDiscoveryReport>) -> McpDiscoveryReport {
    let mut logical = BTreeMap::<String, McpServerRecord>::new();
    let mut malformed_entries = Vec::new();
    let mut warnings = Vec::new();

    for report in reports {
        malformed_entries.extend(report.malformed_entries);
        warnings.extend(report.warnings);
        for server in report.servers {
            let key = logical_key(&server);
            logical
                .entry(key)
                .and_modify(|existing| {
                    for binding in &server.clients {
                        if !existing
                            .clients
                            .iter()
                            .any(|item| item.client == binding.client)
                        {
                            existing.clients.push(binding.clone());
                        }
                    }
                    if existing.auth_state == AuthReferenceState::None
                        && server.auth_state != AuthReferenceState::None
                    {
                        existing.auth_state = server.auth_state.clone();
                    }
                    for capability in &server.capabilities {
                        if !existing.capabilities.contains(capability) {
                            existing.capabilities.push(capability.clone());
                        }
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
                    existing.clients.push(McpClientBindingRecord {
                        client: client.clone(),
                        state: parsed.binding_state.clone(),
                        scope: McpBindingScope::Global,
                    });
                }
            })
            .or_insert_with(|| {
                let mut server = parsed.server;
                server.clients = vec![McpClientBindingRecord {
                    client: client.clone(),
                    state: parsed.binding_state.clone(),
                    scope: McpBindingScope::Global,
                }];
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
    let command_or_url = command
        .map(ToOwned::to_owned)
        .or_else(|| url.map(redacted_url))
        .ok_or_else(|| "entry missing command/url".to_string())?;
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
    let auth_references = collect_auth_references(table);
    let requires_auth = table
        .get("authRequired")
        .and_then(TomlValue::as_bool)
        .unwrap_or(false);
    let auth_state = if auth_references.is_empty() && requires_auth {
        AuthReferenceState::ReferenceMissing
    } else if auth_references.is_empty() {
        AuthReferenceState::None
    } else {
        AuthReferenceState::ReferenceConfigured
    };

    Ok(ParsedServer {
        binding_state: if entry_disabled(table) {
            McpBindingState::Disabled
        } else {
            McpBindingState::Enabled
        },
        server: McpServerRecord {
            id: canonical_server_id(name),
            name: name.to_string(),
            description: format!("{name} configuration"),
            source: "fixture".to_string(),
            transport,
            command_or_url,
            args,
            auth_references: auth_references.clone(),
            clients: Vec::new(),
            capabilities: table
                .get("capabilities")
                .and_then(TomlValue::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(TomlValue::as_str)
                        .map(ToOwned::to_owned)
                        .collect()
                })
                .unwrap_or_else(|| vec!["tools".to_string()]),
            trust: if name.eq_ignore_ascii_case("github") || name.eq_ignore_ascii_case("filesystem")
            {
                McpTrustState::Verified
            } else {
                McpTrustState::ReviewRequired
            },
            auth_state,
            health: McpHealthState::Unknown,
            last_checked: "2026-08-20T00:00:00Z".to_string(),
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
        references.extend(env.iter().filter_map(|(key, value)| {
            value.as_str().map(|_| AuthReference {
                kind: AuthReferenceKind::EnvVar,
                reference: key.clone(),
            })
        }));
    }

    if let Some(headers) = table.get("headers").and_then(TomlValue::as_table) {
        references.extend(headers.iter().filter_map(|(key, value)| {
            value.as_str().map(|_| AuthReference {
                kind: AuthReferenceKind::HeaderName,
                reference: key.clone(),
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

    references
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
    use std::path::PathBuf;

    use super::*;

    fn workspace() -> FixtureWorkspace {
        FixtureWorkspace::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
    }

    #[test]
    fn discovers_normalized_servers_with_redacted_auth() {
        let snapshot = discover_mcp(&workspace()).expect("mcp");
        assert!(snapshot.servers.iter().any(|server| server.id == "github"));
        let postgres = snapshot
            .servers
            .iter()
            .find(|server| server.id == "postgres")
            .expect("postgres");
        assert!(postgres
            .command_or_url
            .starts_with("https://mcp.example.com/postgres"));
        assert!(postgres.auth_references.iter().any(|reference| matches!(
            reference.kind,
            AuthReferenceKind::TokenAlias | AuthReferenceKind::FileReference
        )));
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
}
