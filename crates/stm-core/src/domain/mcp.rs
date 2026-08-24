use serde::{Deserialize, Serialize};

use super::inventory::InventoryState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpClientBindingRecord {
    pub client: McpClientName,
    pub state: McpBindingState,
    pub scope: McpBindingScope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthReference {
    pub kind: AuthReferenceKind,
    pub reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub transport: McpTransport,
    pub command_or_url: String,
    pub args: Vec<String>,
    pub auth_references: Vec<AuthReference>,
    pub clients: Vec<McpClientBindingRecord>,
    pub capabilities: Vec<String>,
    pub trust: McpTrustState,
    pub auth_state: AuthReferenceState,
    pub health: McpHealthState,
    pub last_checked: String,
    pub state: InventoryState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MalformedMcpEntry {
    pub client: McpClientName,
    pub entry_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpDiscoveryReport {
    pub servers: Vec<McpServerRecord>,
    pub malformed_entries: Vec<MalformedMcpEntry>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum McpClientName {
    #[serde(rename = "Codex")]
    Codex,
    #[serde(rename = "Claude Code")]
    ClaudeCode,
    #[serde(rename = "Cursor")]
    Cursor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    Stdio,
    StreamableHttp,
    Sse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpBindingState {
    Enabled,
    Disabled,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpBindingScope {
    Global,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpTrustState {
    Verified,
    ReviewRequired,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthReferenceState {
    None,
    ReferenceConfigured,
    ReferenceMissing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpHealthState {
    Healthy,
    Degraded,
    Unreachable,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthReferenceKind {
    EnvVar,
    HeaderName,
    TokenAlias,
    FileReference,
}
