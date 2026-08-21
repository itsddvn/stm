use std::{collections::BTreeMap, fs, path::Path};

use serde_json::Value as JsonValue;
use toml::Value as TomlValue;
use url::Url;

use crate::{
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

struct ParsedServer {
    server: McpServerRecord,
    binding_state: McpBindingState,
}

pub fn parse_codex_config(path: &Path) -> Result<McpDiscoveryReport, CoreError> {
    let raw = fs::read_to_string(path)?;
    let value: TomlValue = toml::from_str(&raw)?;
    let table = value
        .get("mcp_servers")
        .and_then(TomlValue::as_table)
        .ok_or_else(|| CoreError::UnsupportedSchema("codex mcp_servers missing".to_string()))?;
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

pub fn merge_reports(reports: impl IntoIterator<Item = McpDiscoveryReport>) -> McpDiscoveryReport {
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
                    for client in &server.clients {
                        if !existing
                            .clients
                            .iter()
                            .any(|binding| binding.client == client.client)
                        {
                            existing.clients.push(client.clone());
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
    let mut malformed = Vec::new();
    let mut duplicate_count = 0_usize;

    for (name, value) in entries {
        let parsed = match parse_server(&name, &value) {
            Ok(parsed) => parsed,
            Err(reason) => {
                malformed.push(MalformedMcpEntry {
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
                duplicate_count += 1;
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
                    state: parsed.binding_state,
                    scope: McpBindingScope::Global,
                }];
                server
            });
    }

    McpDiscoveryReport {
        servers: logical.into_values().collect(),
        malformed_entries: malformed,
        warnings: if duplicate_count == 0 {
            Vec::new()
        } else {
            vec![format!(
                "deduplicated {duplicate_count} duplicate logical binding(s) for {}",
                client_name_label(&client)
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
                .collect()
        })
        .unwrap_or_default();
    let auth_references = collect_auth_references(table);
    let binding_state = if entry_disabled(table) {
        McpBindingState::Disabled
    } else {
        McpBindingState::Enabled
    };

    Ok(ParsedServer {
        server: McpServerRecord {
            id: name.to_lowercase().replace(' ', "-"),
            name: name.to_string(),
            description: format!("{name} configuration"),
            source: "fixture".to_string(),
            transport,
            command_or_url,
            args,
            auth_references: auth_references.clone(),
            clients: Vec::new(),
            capabilities: vec!["read_only_inventory".to_string()],
            trust: McpTrustState::ReviewRequired,
            auth_state: if auth_references.is_empty() {
                AuthReferenceState::None
            } else {
                AuthReferenceState::ReferenceConfigured
            },
            health: McpHealthState::Unknown,
            last_checked: "2026-08-20T00:00:00Z".to_string(),
            state: InventoryState::ManagedCurrent,
        },
        binding_state,
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
        if let Some(reference) = table.get(key).and_then(TomlValue::as_str) {
            references.push(AuthReference {
                kind: AuthReferenceKind::TokenAlias,
                reference: reference.to_string(),
            });
        }
    }

    for key in ["authFile", "auth_file"] {
        if let Some(reference) = table.get(key).and_then(TomlValue::as_str) {
            references.push(AuthReference {
                kind: AuthReferenceKind::FileReference,
                reference: reference.to_string(),
            });
        }
    }

    references
}

fn entry_disabled(table: &toml::map::Map<String, TomlValue>) -> bool {
    table.get("disabled").and_then(TomlValue::as_bool) == Some(true)
        || table.get("enabled").and_then(TomlValue::as_bool) == Some(false)
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

fn client_name_label(client: &McpClientName) -> &'static str {
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

    fn fixture(path: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/feasibility")
            .join(path)
    }

    #[test]
    fn parses_mcp_configs_and_redacts_secret_values() {
        let codex = parse_codex_config(&fixture("mcp/codex/config.toml")).expect("codex config");
        assert_eq!(codex.servers.len(), 2);
        assert!(codex
            .warnings
            .iter()
            .any(|warning| warning.contains("deduplicated")));

        let claude = parse_json_client_config(
            McpClientName::ClaudeCode,
            &fixture("mcp/claude-code/config.json"),
        )
        .expect("claude config");
        assert!(claude
            .servers
            .iter()
            .any(|server| server.command_or_url.starts_with("https://")));
        assert!(claude
            .servers
            .iter()
            .all(|server| !server.command_or_url.contains("secret")));
        assert!(claude.servers.iter().any(|server| server
            .auth_references
            .iter()
            .any(|reference| reference.kind == AuthReferenceKind::HeaderName)));

        let cursor =
            parse_json_client_config(McpClientName::Cursor, &fixture("mcp/cursor/mcp.json"))
                .expect("cursor config");
        assert!(!cursor.malformed_entries.is_empty());
        assert!(cursor.servers.iter().any(|server| server
            .clients
            .iter()
            .any(|binding| binding.state == McpBindingState::Disabled)));

        let merged = merge_reports([codex, claude, cursor]);
        let github = merged
            .servers
            .iter()
            .find(|server| server.id == "github")
            .expect("merged github server");
        assert!(github.clients.len() >= 2);
    }

    #[test]
    fn rejects_unsupported_client_schema() {
        let error = parse_json_client_config(
            McpClientName::Cursor,
            &fixture("mcp/cursor/unsupported-config.json"),
        )
        .expect_err("unsupported schema should fail");

        assert!(matches!(error, CoreError::UnsupportedSchema(_)));
    }
}
