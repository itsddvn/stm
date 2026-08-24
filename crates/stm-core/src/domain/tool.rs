use serde::{Deserialize, Serialize};

use super::inventory::{
    CatalogStatus, ExecutionMode, InventoryState, MappingStatus, OwnershipKind,
    PrivilegeRequirement,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolRecord {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub kind: String,
    pub groups: Vec<String>,
    pub recommended: bool,
    pub catalog_status: CatalogStatus,
    pub mapping_status: MappingStatus,
    pub state: InventoryState,
    pub owner: String,
    pub ownership_kind: OwnershipKind,
    pub execution_mode: ExecutionMode,
    pub installed_version: Option<String>,
    pub available_version: Option<String>,
    pub manager: String,
    pub package_id: String,
    pub platform: String,
    pub privilege: PrivilegeRequirement,
    pub lifecycle_confidence: String,
    pub reason_code: Option<String>,
}
