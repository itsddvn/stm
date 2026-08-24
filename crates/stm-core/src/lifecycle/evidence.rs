use serde::{Deserialize, Serialize};

use crate::{catalog::ToolCatalogMapping, error::CoreError};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagerStateEvidence {
    pub installed: bool,
    pub current_version: Option<String>,
    pub target_version: String,
    pub update_available: bool,
    pub source: String,
}

pub trait ManagerEvidencePort: Send + Sync {
    fn inspect(
        &self,
        mapping: &ToolCatalogMapping,
        executable: &str,
    ) -> Result<ManagerStateEvidence, CoreError>;
}
