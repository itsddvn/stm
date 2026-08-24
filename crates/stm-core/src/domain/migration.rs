use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationRecipe {
    pub id: String,
    pub resource_id: String,
    pub source_mapping_id: String,
    pub target_mapping_id: String,
    pub target_executable_paths: Vec<String>,
    pub shared_config_ids: Vec<String>,
    pub cleanup_old_owner_default: bool,
}

pub fn codex_npm_to_homebrew_recipe() -> MigrationRecipe {
    MigrationRecipe {
        id: "codex-npm-to-homebrew".to_string(),
        resource_id: "codex-cli".to_string(),
        source_mapping_id: "npm:@openai/codex".to_string(),
        target_mapping_id: "homebrew:codex".to_string(),
        target_executable_paths: vec![
            "/opt/homebrew/bin/codex".to_string(),
            "/usr/local/bin/codex".to_string(),
        ],
        shared_config_ids: vec!["codex-home".to_string()],
        cleanup_old_owner_default: true,
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationCandidate {
    pub recipe: MigrationRecipe,
    pub source_owner: String,
    pub target_owner: String,
    pub cleanup_old_owner: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MigrationStage {
    Preflight,
    TargetInstalled,
    TargetVerified,
    ActiveSwitched,
    CleanupReviewed,
    SourceRemoved,
    Completed,
    PartialRecoverable,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationEvent {
    TargetInstalled,
    TargetVerified,
    ActiveSwitched,
    CleanupApproved,
    SourceRemoved,
    Complete,
    FailTarget,
    FailCleanup,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationStateMachine {
    stage: MigrationStage,
    cleanup_required: bool,
}

impl MigrationStateMachine {
    pub fn new(cleanup_required: bool) -> Self {
        Self {
            stage: MigrationStage::Preflight,
            cleanup_required,
        }
    }

    pub fn stage(&self) -> MigrationStage {
        self.stage
    }

    pub fn transition(&mut self, event: MigrationEvent) -> Result<MigrationStage, String> {
        self.stage = match (self.stage, event) {
            (_, MigrationEvent::FailTarget) => MigrationStage::Failed,
            (MigrationStage::CleanupReviewed, MigrationEvent::FailCleanup)
            | (MigrationStage::SourceRemoved, MigrationEvent::FailCleanup) => {
                MigrationStage::PartialRecoverable
            }
            (MigrationStage::Preflight, MigrationEvent::TargetInstalled) => {
                MigrationStage::TargetInstalled
            }
            (MigrationStage::TargetInstalled, MigrationEvent::TargetVerified) => {
                MigrationStage::TargetVerified
            }
            (MigrationStage::TargetVerified, MigrationEvent::ActiveSwitched) => {
                MigrationStage::ActiveSwitched
            }
            (MigrationStage::ActiveSwitched, MigrationEvent::CleanupApproved)
                if self.cleanup_required =>
            {
                MigrationStage::CleanupReviewed
            }
            (MigrationStage::CleanupReviewed, MigrationEvent::SourceRemoved) => {
                MigrationStage::SourceRemoved
            }
            (MigrationStage::SourceRemoved, MigrationEvent::Complete) => MigrationStage::Completed,
            (MigrationStage::ActiveSwitched, MigrationEvent::Complete)
                if !self.cleanup_required =>
            {
                MigrationStage::Completed
            }
            _ => {
                return Err(format!(
                    "migration event {event:?} is not allowed from {:?}",
                    self.stage
                ))
            }
        };
        Ok(self.stage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_cannot_precede_target_verification_and_switch() {
        let mut migration = MigrationStateMachine::new(true);
        assert!(migration
            .transition(MigrationEvent::CleanupApproved)
            .is_err());
        migration
            .transition(MigrationEvent::TargetInstalled)
            .unwrap();
        assert!(migration
            .transition(MigrationEvent::CleanupApproved)
            .is_err());
        migration
            .transition(MigrationEvent::TargetVerified)
            .unwrap();
        assert!(migration
            .transition(MigrationEvent::CleanupApproved)
            .is_err());
        migration
            .transition(MigrationEvent::ActiveSwitched)
            .unwrap();
        migration
            .transition(MigrationEvent::CleanupApproved)
            .unwrap();
        migration.transition(MigrationEvent::SourceRemoved).unwrap();
        migration.transition(MigrationEvent::Complete).unwrap();
        assert_eq!(migration.stage(), MigrationStage::Completed);
    }

    #[test]
    fn cleanup_failure_is_partial_recoverable() {
        let mut migration = MigrationStateMachine::new(true);
        migration
            .transition(MigrationEvent::TargetInstalled)
            .unwrap();
        migration
            .transition(MigrationEvent::TargetVerified)
            .unwrap();
        migration
            .transition(MigrationEvent::ActiveSwitched)
            .unwrap();
        migration
            .transition(MigrationEvent::CleanupApproved)
            .unwrap();
        migration.transition(MigrationEvent::FailCleanup).unwrap();
        assert_eq!(migration.stage(), MigrationStage::PartialRecoverable);
    }
}
