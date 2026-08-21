use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppEvent {
    pub id: String,
    pub event_type: AppEventType,
    pub operation_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppEventType {
    RefreshStarted,
    CatalogValidated,
    InventoryScanned,
    SkillsScanned,
    McpDiscovered,
    SnapshotRecovered,
    SnapshotCommitted,
    DiagnosticsReady,
}
