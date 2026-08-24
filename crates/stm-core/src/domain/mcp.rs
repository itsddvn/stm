use serde::{Deserialize, Serialize};

use super::inventory::InventoryState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpClientBindingRecord {
    pub client: McpClientName,
    pub state: McpBindingState,
    pub scope: McpBindingScope,
    #[serde(default)]
    pub entry_name: String,
    #[serde(default = "default_mcp_transport")]
    pub transport: McpTransport,
    #[serde(default)]
    pub command_or_url: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub auth_references: Vec<AuthReference>,
    #[serde(default)]
    pub auth_required: bool,
    #[serde(default = "default_auth_reference_state")]
    pub auth_state: AuthReferenceState,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default = "default_mcp_trust_state")]
    pub trust: McpTrustState,
    #[serde(default = "default_mcp_health_state")]
    pub health: McpHealthState,
    #[serde(default)]
    pub last_checked: String,
}

impl McpClientBindingRecord {
    pub fn from_server(
        client: McpClientName,
        state: McpBindingState,
        scope: McpBindingScope,
        entry_name: impl Into<String>,
        server: &McpServerRecord,
    ) -> Self {
        Self {
            client,
            state,
            scope,
            entry_name: entry_name.into(),
            transport: server.transport.clone(),
            command_or_url: server.command_or_url.clone(),
            args: server.args.clone(),
            auth_references: server.auth_references.clone(),
            auth_required: server.auth_required,
            auth_state: server.auth_state.clone(),
            capabilities: server.capabilities.clone(),
            trust: server.trust.clone(),
            health: server.health.clone(),
            last_checked: server.last_checked.clone(),
        }
    }

    pub fn project_server(&self, aggregate: &McpServerRecord) -> McpServerRecord {
        let mut server = aggregate.clone();
        server.transport = self.transport.clone();
        server.command_or_url = self.command_or_url.clone();
        server.args = self.args.clone();
        server.auth_references = self.auth_references.clone();
        server.auth_required = self.auth_required;
        server.auth_state = self.auth_state.clone();
        server.capabilities = self.capabilities.clone();
        server.trust = self.trust.clone();
        server.health = self.health.clone();
        server.last_checked = self.last_checked.clone();
        server
    }
}

fn default_mcp_transport() -> McpTransport {
    McpTransport::Stdio
}

fn default_auth_reference_state() -> AuthReferenceState {
    AuthReferenceState::None
}

fn default_mcp_trust_state() -> McpTrustState {
    McpTrustState::ReviewRequired
}

fn default_mcp_health_state() -> McpHealthState {
    McpHealthState::Unknown
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
    #[serde(default)]
    pub auth_required: bool,
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
