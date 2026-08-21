use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleResourceKind {
    Tool,
    Skill,
    Mcp,
    Product,
    Operation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LifecyclePlanRequest {
    pub resource_kind: LifecycleResourceKind,
    pub action: String,
    pub resource_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_analysis_handle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifecyclePrivilege {
    None,
    UserConfirmation,
    ElevationRequired,
    VendorControlled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleRevalidationState {
    Fresh,
    Required,
    Expired,
    EvidenceChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleRevalidation {
    pub state: LifecycleRevalidationState,
    pub checked_at: String,
    pub checks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum LifecycleExecution {
    ManagedExecute {
        executable: String,
        argv: Vec<String>,
    },
    SignedProductUpdate {
        executable: String,
        argv: Vec<String>,
    },
    VendorHandoff {
        handoff_target: String,
    },
    DetectOnly {
        guidance: String,
    },
    Batch {
        items: Vec<LifecyclePlan>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LifecyclePlan {
    pub request: LifecyclePlanRequest,
    pub plan_id: String,
    pub canonical_id: String,
    pub mapping_id: String,
    pub resource_id: String,
    pub owner: String,
    pub source: String,
    pub current_version: String,
    pub target_version: String,
    pub privilege: LifecyclePrivilege,
    pub affected_paths: Vec<String>,
    pub affected_records: Vec<String>,
    pub confidence: String,
    pub limitations: Vec<String>,
    pub digest: String,
    pub expires_at: String,
    pub revalidation: LifecycleRevalidation,
    pub execution: LifecycleExecution,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleConsentAuthorization {
    pub plan_digest: String,
    pub plan_expires_at: String,
    pub granted_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleItemStatus {
    Pending,
    InProgress,
    Success,
    Failed,
    Cancelled,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleItemResult {
    pub id: String,
    pub label: String,
    pub status: LifecycleItemStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<String>,
    pub redacted_detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleFollowUpAction {
    pub id: String,
    pub label: String,
    pub plan_request: LifecyclePlanRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleExecutionStatus {
    InProgress,
    Success,
    Partial,
    Failed,
    Cancelled,
    Recoverable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleExecutionResult {
    pub operation_id: String,
    pub plan_digest: String,
    pub status: LifecycleExecutionStatus,
    pub completed_steps: usize,
    pub total_steps: usize,
    pub can_cancel: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<String>,
    pub redacted_detail: String,
    pub items: Vec<LifecycleItemResult>,
    pub retry_actions: Vec<LifecycleFollowUpAction>,
    pub recovery_actions: Vec<LifecycleFollowUpAction>,
}
