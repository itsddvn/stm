use serde::{Deserialize, Serialize};

use crate::domain::{
    application_update::ApplicationUpdateRecord,
    inventory::Freshness,
    lifecycle::{LifecycleExecutionResult, LifecyclePlanRequest},
    mcp::McpServerRecord,
    operation::OperationReceipt,
    skill::SkillRecord,
    tool::ToolRecord,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationLogEntry {
    pub receipt: OperationReceipt,
    pub resource: String,
    pub action: String,
    pub owner: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_request: Option<LifecyclePlanRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_result: Option<LifecycleExecutionResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_process_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_process_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScanErrorEntry {
    pub scope: String,
    pub code: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotBundle {
    pub generated_at: String,
    pub catalog_version: String,
    pub freshness: Freshness,
    pub tools: Vec<ToolRecord>,
    pub skills: Vec<SkillRecord>,
    pub mcp_servers: Vec<McpServerRecord>,
    pub updates: Vec<ApplicationUpdateRecord>,
    pub operations: Vec<OperationLogEntry>,
    pub errors: Vec<ScanErrorEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StorageHealth {
    pub path: String,
    pub user_version: i64,
    pub recovered_from_corruption: bool,
    pub last_good_available: bool,
}
