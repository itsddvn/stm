use serde::{Deserialize, Serialize};

use super::inventory::{ExecutionMode, OwnershipKind};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationPlan {
    pub id: String,
    pub resource_type: OperationResourceType,
    pub resource_id: String,
    pub action: String,
    pub execution_mode: ExecutionMode,
    pub ownership_kind: OwnershipKind,
    pub requires_consent: bool,
    pub warnings: Vec<String>,
    pub steps: Vec<OperationPlanStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationPlanStep {
    pub id: String,
    pub label: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationReceipt {
    pub id: String,
    pub operation_id: String,
    pub status: OperationStatus,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub summary: String,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConsentRecord {
    pub operation_id: String,
    pub granted: bool,
    pub actor: String,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OperationResourceType {
    #[serde(rename = "tool")]
    Tool,
    #[serde(rename = "skill")]
    Skill,
    #[serde(rename = "mcp")]
    Mcp,
    #[serde(rename = "product")]
    Product,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Success,
    Partial,
    Cancelled,
    Failed,
    Recoverable,
    InProgress,
}
