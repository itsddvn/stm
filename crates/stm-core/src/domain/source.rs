use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceAnalysisRecord {
    pub kind: SourceKind,
    pub submitted_url: String,
    pub normalized_url: Option<String>,
    pub status: SourceAnalysisStatus,
    pub detected_name: String,
    pub source_host: String,
    pub source_type: String,
    pub publisher: String,
    pub target: String,
    pub trust: SourceTrust,
    pub risk_flags: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Tool,
    Skill,
    Mcp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceAnalysisStatus {
    ReviewReady,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceTrust {
    CatalogMatch,
    ReviewRequired,
    Blocked,
}
