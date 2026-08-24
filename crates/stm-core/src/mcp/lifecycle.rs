use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::domain::mcp::{McpClientName, McpHealthState, McpServerRecord};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpMutationAction {
    Add,
    Update,
    Enable,
    Disable,
    Remove,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpConfigTarget {
    pub client: McpClientName,
    pub config_path: PathBuf,
    pub entry_name: String,
    pub expected_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreparedMcpMutation {
    pub operation_id: String,
    pub server: McpServerRecord,
    pub action: McpMutationAction,
    pub targets: Vec<McpConfigTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PreparedMcpAction {
    Mutate(Box<PreparedMcpMutation>),
    RestoreBackup(String),
    KeepPartial,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpTargetStatus {
    Success,
    Failed,
    NoOp,
    Restored,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpTargetOutcome {
    pub client: McpClientName,
    pub status: McpTargetStatus,
    pub receipt_id: Option<String>,
    pub backup_id: Option<String>,
    pub health: McpHealthState,
    pub redacted_detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpMutationOutcome {
    pub operation_id: String,
    pub completed: usize,
    pub failed: usize,
    pub targets: Vec<McpTargetOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpBackupState {
    Available,
    Restored,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpBackupReceipt {
    pub backup_id: String,
    pub operation_id: String,
    pub server_id: String,
    pub client: McpClientName,
    pub backup_file_name: String,
    pub target_existed: bool,
    pub original_sha256: Option<String>,
    #[serde(default)]
    pub replacement_sha256: String,
    #[serde(default)]
    pub replacement_existed: bool,
    pub state: McpBackupState,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpRecoveryPhase {
    Prepared,
    BackupCreated,
    ReplacementActivated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpRecoveryRecord {
    pub operation_id: String,
    pub server_id: String,
    pub client: McpClientName,
    pub target_path: PathBuf,
    pub backup: McpBackupReceipt,
    pub replacement_sha256: String,
    pub phase: McpRecoveryPhase,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpLifecycleReceipt {
    pub receipt_id: String,
    pub operation_id: String,
    pub server_id: String,
    pub action: McpMutationAction,
    pub client: McpClientName,
    pub config_sha256: Option<String>,
    pub health: McpHealthState,
    pub recorded_at: String,
}
