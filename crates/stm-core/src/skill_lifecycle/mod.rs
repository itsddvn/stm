use serde::{Deserialize, Serialize};

use crate::domain::skill::SkillClientName;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeValidationPolicy {
    pub max_files: usize,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub max_depth: usize,
}

impl Default for TreeValidationPolicy {
    fn default() -> Self {
        Self {
            max_files: 256,
            max_file_bytes: 1024 * 1024,
            max_total_bytes: 8 * 1024 * 1024,
            max_depth: 16,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillSourceSpec {
    pub repository: String,
    pub subpath: String,
    pub commit: String,
    pub tree_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillTargetSpec {
    pub client: SkillClientName,
    pub target_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillMutationAction {
    Install,
    Update,
    RestoreManaged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LocalConflictChoice {
    Block,
    KeepLocal,
    ExportDiff { destination: String },
    RestoreManaged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PartialFailurePolicy {
    RollbackCompleted,
    KeepPartial,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillManifestEvidence {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StagedFileEvidence {
    pub path: String,
    pub git_mode: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SkillRiskEvidence {
    pub scripts: Vec<String>,
    pub requirements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillStagingEvidence {
    pub private_staging_path: String,
    pub tree_sha256: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub manifest: SkillManifestEvidence,
    pub files: Vec<StagedFileEvidence>,
    pub risk: SkillRiskEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreparedTargetMutation {
    pub target: SkillTargetSpec,
    pub conflict_choice: LocalConflictChoice,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreparedSkillMutation {
    pub operation_id: String,
    pub skill_id: String,
    pub action: SkillMutationAction,
    pub source: SkillSourceSpec,
    pub staging: SkillStagingEvidence,
    pub targets: Vec<PreparedTargetMutation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetMutationStatus {
    Installed,
    Updated,
    Restored,
    NoOp,
    KeptLocal,
    DiffExported,
    Failed,
    RolledBack,
    Skipped,
    RecoveryRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillTargetOutcome {
    pub target: SkillTargetSpec,
    pub status: TargetMutationStatus,
    pub receipt_id: Option<String>,
    pub backup_id: Option<String>,
    pub redacted_detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillMutationOutcome {
    pub operation_id: String,
    pub targets: Vec<SkillTargetOutcome>,
    pub completed: usize,
    pub failed: usize,
    pub partial_state_kept: bool,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSkillReceipt {
    pub receipt_id: String,
    pub operation_id: String,
    pub skill_id: String,
    pub target: SkillTargetSpec,
    pub source: SkillSourceSpec,
    pub tree_sha256: String,
    pub file_manifest: Vec<StagedFileEvidence>,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackupState {
    Available,
    Restored,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillBackupReceipt {
    pub backup_id: String,
    pub operation_id: String,
    pub target_key: String,
    pub backup_path: String,
    #[serde(default)]
    pub backup_tree_sha256: String,
    pub previous_receipts: Vec<(String, ManagedSkillReceipt)>,
    pub replaced_target_keys: Vec<String>,
    pub state: BackupState,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillRecoveryPhase {
    Prepared,
    ExistingMovedToBackup,
    ReplacementActivated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillRecoveryRecord {
    pub operation_id: String,
    pub target_key: String,
    pub target: SkillTargetSpec,
    pub target_path: String,
    pub sibling_staging_path: String,
    pub backup: Option<SkillBackupReceipt>,
    pub pending_receipts: Vec<(String, ManagedSkillReceipt)>,
    pub expected_tree_sha256: String,
    pub phase: SkillRecoveryPhase,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticatedCatalogStateRecord {
    pub channel: String,
    pub catalog_version: String,
    pub key_id: String,
    pub manifest_sha256: String,
    pub payload_sha256: String,
    pub expires_at: String,
    pub activated_at: String,
    pub manifest_json: String,
    pub catalog_json: String,
}
