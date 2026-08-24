use std::{
    collections::BTreeSet,
    env,
    path::{Component, Path},
    sync::LazyLock,
};

use crate::domain::{
    inventory::InventoryState,
    mcp::{
        AuthReference, AuthReferenceKind, AuthReferenceState, McpBindingScope, McpBindingState,
        McpClientBindingRecord, McpClientName, McpHealthState, McpServerRecord, McpTransport,
        McpTrustState,
    },
};
use serde::Deserialize;

const MAX_ARGUMENTS: usize = 64;
const MAX_ARGUMENT_BYTES: usize = 4 * 1024;
const APPROVED_MAPPINGS_JSON: &str = include_str!("../../../../catalog/mcp/approved.json");
static APPROVED_MAPPINGS: LazyLock<Result<ApprovedCatalog, String>> =
    LazyLock::new(load_approved_mappings);
const KNOWN_CAPABILITIES: &[&str] = &[
    "resources",
    "tools",
    "prompts",
    "logging",
    "completions",
    "roots",
    "sampling",
    "elicitation",
];
pub(crate) fn validate_inventory_fields(
    transport: &McpTransport,
    command_or_url: &str,
    args: &[String],
    capabilities: &[String],
) -> Result<(), String> {
    if capabilities
        .iter()
        .any(|capability| !KNOWN_CAPABILITIES.contains(&capability.as_str()))
    {
        return Err("MCP entry declares an unsupported capability".into());
    }
    match transport {
        McpTransport::StreamableHttp | McpTransport::Sse => {
            validate_remote_endpoint(command_or_url)
        }
        McpTransport::Stdio => validate_stdio_fields(command_or_url, args),
    }
}

pub(crate) fn approved_mapping_id(
    name: &str,
    transport: &McpTransport,
    command_or_url: &str,
    args: &[String],
    capabilities: &[String],
) -> Option<String> {
    approved_mappings().and_then(|catalog| {
        catalog
            .mappings
            .iter()
            .find(|mapping| {
                mapping_matches(mapping, name, transport, command_or_url, args, capabilities)
            })
            .map(|mapping| mapping.id.clone())
    })
}

pub(crate) fn trust_state(
    name: &str,
    transport: &McpTransport,
    command_or_url: &str,
    args: &[String],
    capabilities: &[String],
) -> McpTrustState {
    if approved_mapping_id(name, transport, command_or_url, args, capabilities).is_some() {
        McpTrustState::Verified
    } else {
        McpTrustState::ReviewRequired
    }
}

pub(crate) fn validate_lifecycle_server(server: &McpServerRecord) -> Result<(), String> {
    validate_inventory_fields(
        &server.transport,
        &server.command_or_url,
        &server.args,
        &server.capabilities,
    )?;
    if server.trust != McpTrustState::Verified
        || trust_state(
            &server.name,
            &server.transport,
            &server.command_or_url,
            &server.args,
            &server.capabilities,
        ) != McpTrustState::Verified
    {
        return Err("MCP lifecycle requires an exact trusted declarative mapping".into());
    }
    if server.auth_state == AuthReferenceState::ReferenceMissing
        || server
            .auth_references
            .iter()
            .any(|reference| !auth_reference_available(reference))
    {
        return Err("MCP lifecycle credential references are unavailable".into());
    }
    Ok(())
}

fn validate_remote_endpoint(raw: &str) -> Result<(), String> {
    let url = url::Url::parse(raw).map_err(|_| "MCP endpoint is not a valid URL".to_string())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.host_str().is_none()
    {
        return Err("MCP endpoint must be credential-free HTTPS without query or fragment".into());
    }
    Ok(())
}

fn validate_stdio_fields(command: &str, args: &[String]) -> Result<(), String> {
    if command.is_empty()
        || command.chars().any(char::is_whitespace)
        || command
            .chars()
            .any(|character| matches!(character, ';' | '|' | '&' | '>' | '<' | '`'))
    {
        return Err("MCP stdio command must be one executable token".into());
    }
    if args.len() > MAX_ARGUMENTS
        || args.iter().map(String::len).sum::<usize>() > MAX_ARGUMENT_BYTES
    {
        return Err("MCP stdio argument vector exceeds policy bounds".into());
    }
    for (index, argument) in args.iter().enumerate() {
        if argument.chars().any(char::is_control) || contains_secret_assignment(argument) {
            return Err("MCP stdio arguments contain unsafe or secret-bearing input".into());
        }
        if secret_flag(argument)
            && args
                .get(index + 1)
                .is_some_and(|value| !is_reference_placeholder(value))
        {
            return Err("MCP stdio secret arguments must use a reference placeholder".into());
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApprovedCatalog {
    schema_version: u64,
    mappings: Vec<ApprovedMapping>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApprovedAuthReference {
    kind: AuthReferenceKind,
    reference: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApprovedMapping {
    id: String,
    name: String,
    aliases: Vec<String>,
    transport: String,
    command_or_url: String,
    args_prefix: Vec<String>,
    allow_absolute_trailing_args: bool,
    capabilities: Vec<String>,
    clients: Vec<String>,
    auth_required: bool,
    auth_references: Vec<ApprovedAuthReference>,
}

fn load_approved_mappings() -> Result<ApprovedCatalog, String> {
    let catalog: ApprovedCatalog =
        serde_json::from_str(APPROVED_MAPPINGS_JSON).map_err(|error| error.to_string())?;
    if catalog.schema_version != 2
        || catalog.mappings.is_empty()
        || catalog.mappings.iter().any(|mapping| {
            mapping.id.is_empty()
                || mapping.name.is_empty()
                || mapping.aliases.is_empty()
                || mapping.clients.is_empty()
                || (mapping.auth_required && mapping.auth_references.is_empty())
        })
    {
        return Err("approved MCP mapping catalog is invalid".into());
    }
    Ok(catalog)
}

fn approved_mappings() -> Option<&'static ApprovedCatalog> {
    APPROVED_MAPPINGS.as_ref().ok()
}

pub(crate) fn approved_server(
    id: &str,
    trailing_args: &[String],
    clients: &[McpClientName],
) -> Result<Option<McpServerRecord>, String> {
    let Some(mapping) = approved_mappings()
        .and_then(|catalog| catalog.mappings.iter().find(|mapping| mapping.id == id))
    else {
        return Ok(None);
    };
    if clients.is_empty()
        || clients.iter().any(|client| {
            !mapping
                .clients
                .iter()
                .any(|value| value == client_key(client))
        })
    {
        return Err("approved MCP mapping does not support every selected client".into());
    }
    let mut args = mapping.args_prefix.clone();
    args.extend_from_slice(trailing_args);
    let transport = parse_transport(&mapping.transport)?;
    if !mapping_matches(
        mapping,
        &mapping.name,
        &transport,
        &mapping.command_or_url,
        &args,
        &mapping.capabilities,
    ) {
        return Err("approved MCP mapping arguments do not satisfy policy".into());
    }
    let auth_references = mapping
        .auth_references
        .iter()
        .map(|reference| AuthReference {
            kind: reference.kind.clone(),
            reference: reference.reference.clone(),
        })
        .collect::<Vec<_>>();
    let auth_state = if mapping.auth_required
        && auth_references
            .iter()
            .any(|reference| !auth_reference_available(reference))
    {
        AuthReferenceState::ReferenceMissing
    } else if auth_references.is_empty() {
        AuthReferenceState::None
    } else {
        AuthReferenceState::ReferenceConfigured
    };
    let mut server = McpServerRecord {
        id: mapping.id.clone(),
        name: mapping.name.clone(),
        description: "Approved MCP catalog mapping".into(),
        source: "approved-catalog".into(),
        transport,
        command_or_url: mapping.command_or_url.clone(),
        args,
        auth_references,
        clients: Vec::new(),
        auth_required: mapping.auth_required,
        capabilities: mapping.capabilities.clone(),
        trust: McpTrustState::Verified,
        auth_state,
        health: McpHealthState::Unknown,
        last_checked: "not_checked".into(),
        state: InventoryState::ManagedCurrent,
    };
    server.clients = clients
        .iter()
        .cloned()
        .map(|client| {
            McpClientBindingRecord::from_server(
                client,
                McpBindingState::Enabled,
                McpBindingScope::Global,
                mapping.name.clone(),
                &server,
            )
        })
        .collect();
    Ok(Some(server))
}

pub(crate) fn approved_server_for_endpoint(
    endpoint: &str,
    clients: &[McpClientName],
) -> Result<Option<McpServerRecord>, String> {
    let Some(id) = approved_mappings().and_then(|catalog| {
        catalog
            .mappings
            .iter()
            .find(|mapping| mapping.transport != "stdio" && mapping.command_or_url == endpoint)
            .map(|mapping| mapping.id.clone())
    }) else {
        return Ok(None);
    };
    approved_server(&id, &[], clients)
}

pub(crate) fn auth_references_available(references: &[AuthReference]) -> bool {
    references.iter().all(auth_reference_available)
}

fn auth_reference_available(reference: &AuthReference) -> bool {
    match reference.kind {
        AuthReferenceKind::EnvVar => {
            env::var_os(&reference.reference).is_some_and(|value| !value.is_empty())
        }
        AuthReferenceKind::TokenAlias
        | AuthReferenceKind::HeaderName
        | AuthReferenceKind::FileReference => false,
    }
}

fn parse_transport(value: &str) -> Result<McpTransport, String> {
    match value {
        "stdio" => Ok(McpTransport::Stdio),
        "streamable_http" => Ok(McpTransport::StreamableHttp),
        "sse" => Ok(McpTransport::Sse),
        _ => Err("approved MCP mapping uses an unsupported transport".into()),
    }
}

fn client_key(client: &McpClientName) -> &'static str {
    match client {
        McpClientName::Codex => "codex",
        McpClientName::ClaudeCode => "claude_code",
        McpClientName::Cursor => "cursor",
    }
}

fn mapping_matches(
    mapping: &ApprovedMapping,
    name: &str,
    transport: &McpTransport,
    command_or_url: &str,
    args: &[String],
    capabilities: &[String],
) -> bool {
    let identity_matches = mapping.name.eq_ignore_ascii_case(name)
        || mapping
            .aliases
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case(name));
    let transport_matches = mapping.transport == transport_label(transport);
    let capabilities_match = mapping
        .capabilities
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        == capabilities
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
    let arguments_match = args.starts_with(&mapping.args_prefix)
        && if mapping.allow_absolute_trailing_args {
            args.len() > mapping.args_prefix.len()
                && args[mapping.args_prefix.len()..].iter().all(|root| {
                    let path = Path::new(root);
                    path.is_absolute()
                        && !path
                            .components()
                            .any(|component| component == Component::ParentDir)
                })
        } else {
            args.len() == mapping.args_prefix.len()
        };
    identity_matches
        && transport_matches
        && command_or_url == mapping.command_or_url
        && capabilities_match
        && arguments_match
}

fn transport_label(transport: &McpTransport) -> &'static str {
    match transport {
        McpTransport::Stdio => "stdio",
        McpTransport::StreamableHttp => "streamable_http",
        McpTransport::Sse => "sse",
    }
}

fn contains_secret_assignment(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    ["token=", "password=", "secret=", "api_key=", "api-key="]
        .iter()
        .any(|marker| lower.contains(marker))
}

fn secret_flag(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "--token" | "--password" | "--secret" | "--api-key" | "--api_key"
    )
}

fn is_reference_placeholder(value: &str) -> bool {
    (value.starts_with("${") && value.ends_with('}'))
        || (value.starts_with("$env:") && value.len() > 5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_declarative_mappings_are_verified_but_name_spoofs_are_not() {
        let args = vec![
            "-y".into(),
            "@modelcontextprotocol/server-filesystem".into(),
            "/tmp".into(),
        ];
        assert_eq!(
            trust_state(
                "Filesystem",
                &McpTransport::Stdio,
                "npx",
                &args,
                &["resources".into(), "tools".into()],
            ),
            McpTrustState::Verified
        );
        assert_eq!(
            trust_state(
                "Filesystem",
                &McpTransport::Stdio,
                "sh",
                &args,
                &["resources".into(), "tools".into()],
            ),
            McpTrustState::ReviewRequired
        );
    }

    #[test]
    fn endpoints_and_arguments_with_inline_credentials_fail_closed() {
        assert!(validate_inventory_fields(
            &McpTransport::StreamableHttp,
            "https://mcp.example.com/api?token=secret",
            &[],
            &["tools".into()]
        )
        .is_err());
        assert!(validate_inventory_fields(
            &McpTransport::Stdio,
            "npx",
            &["--token=secret".into()],
            &["tools".into()]
        )
        .is_err());
    }

    #[test]
    fn approved_catalog_builds_executable_stdio_and_auth_bound_remote_records() {
        let filesystem = approved_server("filesystem", &["/tmp".into()], &[McpClientName::Codex])
            .expect("catalog")
            .expect("filesystem");
        assert_eq!(filesystem.transport, McpTransport::Stdio);
        assert_eq!(
            filesystem.args,
            vec!["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
        );
        assert_eq!(filesystem.clients.len(), 1);
        assert_eq!(filesystem.clients[0].args, filesystem.args);
        assert!(approved_server("filesystem", &[], &[McpClientName::Codex]).is_err());

        let mut github = approved_server_for_endpoint(
            "https://api.githubcopilot.com/mcp/",
            &[McpClientName::ClaudeCode],
        )
        .expect("catalog")
        .expect("github");
        assert_eq!(github.capabilities, vec!["tools", "prompts"]);
        assert_eq!(github.auth_references.len(), 1);
        assert_eq!(github.auth_references[0].kind, AuthReferenceKind::EnvVar);
        assert_eq!(github.auth_references[0].reference, "GITHUB_COPILOT_TOKEN");
        github.auth_state = AuthReferenceState::ReferenceMissing;
        assert!(validate_lifecycle_server(&github).is_err());
    }
}
