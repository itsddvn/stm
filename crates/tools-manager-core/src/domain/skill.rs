use serde::{Deserialize, Serialize};

use super::inventory::InventoryState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillTargetRecord {
    pub client: SkillClientName,
    pub path: String,
    pub state: SkillTargetState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillDiffRecord {
    pub file: String,
    pub change: SkillDiffKind,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub revision: String,
    pub available_revision: Option<String>,
    pub digest: String,
    pub state: InventoryState,
    pub purposes: Vec<String>,
    pub targets: Vec<SkillTargetRecord>,
    pub risk_flags: Vec<String>,
    pub diff: Vec<SkillDiffRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GlobalSkillEntry {
    pub client: SkillClientName,
    pub slug: String,
    pub root: String,
    pub manifest_path: String,
    pub rejected_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillRootResolution {
    pub client: SkillClientName,
    pub declared_root: String,
    pub canonical_root: Option<String>,
    pub accepted: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillScanReport {
    pub roots: Vec<SkillRootResolution>,
    pub skills: Vec<GlobalSkillEntry>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SkillClientName {
    #[serde(rename = "Codex")]
    Codex,
    #[serde(rename = "Claude Code")]
    ClaudeCode,
    #[serde(rename = "AgentKit")]
    AgentKit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillTargetState {
    Current,
    Modified,
    Failed,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillDiffKind {
    Added,
    Modified,
    Removed,
}
