use serde::{Deserialize, Serialize};

use super::{
    lifecycle::LifecycleChildIntent,
    provider::{InstallProviderPreference, ProviderInventory},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SetupRowAction {
    Install,
    Update,
    Installed,
    Handoff,
    Guidance,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SetupRow {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub selected: bool,
    pub optional: bool,
    pub action: SetupRowAction,
    pub reason: Option<String>,
    pub owner: String,
    pub mapping_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuickSetupView {
    pub target: String,
    pub preference: InstallProviderPreference,
    pub dismissed: bool,
    pub providers: ProviderInventory,
    pub tools: Vec<SetupRow>,
    pub optional_skills: Vec<SetupRow>,
    pub optional_mcp: Vec<SetupRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SetupSelection {
    pub resource_ids: Vec<String>,
}

impl SetupSelection {
    pub fn to_intents(&self, rows: &[SetupRow]) -> Vec<LifecycleChildIntent> {
        self.resource_ids
            .iter()
            .filter_map(|id| {
                let row = rows.iter().find(|row| row.id == *id)?;
                let desired_action = match row.action {
                    SetupRowAction::Install => "install",
                    SetupRowAction::Update => "update",
                    SetupRowAction::Handoff => "update",
                    SetupRowAction::Guidance => "review",
                    SetupRowAction::Blocked | SetupRowAction::Installed => return None,
                };
                Some(LifecycleChildIntent {
                    resource_kind: super::lifecycle::LifecycleResourceKind::Tool,
                    resource_id: row.id.clone(),
                    desired_action: desired_action.to_string(),
                    mapping_id: row.mapping_id.clone(),
                    depends_on: Vec::new(),
                })
            })
            .collect()
    }
}
