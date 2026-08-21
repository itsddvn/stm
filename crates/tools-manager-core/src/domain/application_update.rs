use serde::{Deserialize, Serialize};

use super::inventory::ExecutionMode;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplicationUpdateRecord {
    pub id: String,
    pub resource_type: ApplicationUpdateKind,
    pub name: String,
    pub current: String,
    pub target: String,
    pub execution_mode: UpdateExecutionMode,
    pub selected: bool,
    pub risk: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ApplicationUpdateKind {
    #[serde(rename = "tool")]
    Tool,
    #[serde(rename = "skill")]
    Skill,
    #[serde(rename = "product")]
    Product,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UpdateExecutionMode {
    #[serde(rename = "managed_execute")]
    ManagedExecute,
    #[serde(rename = "vendor_handoff")]
    VendorHandoff,
    #[serde(rename = "detect_only")]
    DetectOnly,
    #[serde(rename = "signed_product_update")]
    SignedProductUpdate,
}

impl From<ExecutionMode> for UpdateExecutionMode {
    fn from(value: ExecutionMode) -> Self {
        match value {
            ExecutionMode::ManagedExecute => Self::ManagedExecute,
            ExecutionMode::VendorHandoff => Self::VendorHandoff,
            ExecutionMode::DetectOnly => Self::DetectOnly,
        }
    }
}
