use std::path::{Path, PathBuf};

use stm_core::{
    domain::{
        mcp::{McpClientName, McpHealthState},
        skill::SkillClientName,
    },
    feasibility::process_supervisor::CancelSignal,
    lifecycle::{CompiledManagerCommand, ExecutableIdentity},
    mcp::lifecycle::{
        McpBackupReceipt, McpMutationAction, McpMutationOutcome, PreparedMcpMutation,
    },
    ports::{McpLifecyclePort, SkillLifecyclePort},
    skill_catalog::VerifiedSkillCatalog,
    skill_lifecycle::{
        ManagedSkillReceipt, PartialFailurePolicy, PreparedSkillMutation, SkillBackupReceipt,
        SkillMutationOutcome, SkillSourceSpec, SkillStagingEvidence, SkillTargetOutcome,
        TreeValidationPolicy,
    },
    CoreError,
};

use crate::{
    host::{compile_mcp_stdio, RealHostExecutableResolver},
    mcp::lifecycle::{client_config_path, config_digest, McpConfigMaterializer},
    skill_catalog::load_current_authenticated_catalog,
    skill_lifecycle::{
        cleanup_abandoned_private_staging, cleanup_private_staging, ApprovedSkillRoot,
        GitResolverLimits, PublicGithubSkillResolver, ReviewedGitExecutable, SkillMaterializer,
    },
    storage::SqliteSnapshotStore,
};
use stm_core::ports::HostExecutableResolver;

pub struct RuntimeSkillLifecycle {
    database_path: PathBuf,
    runtime_root: PathBuf,
    home: PathBuf,
}

impl RuntimeSkillLifecycle {
    pub fn new(database_path: PathBuf, runtime_root: PathBuf, home: PathBuf) -> Self {
        Self {
            database_path,
            runtime_root,
            home,
        }
    }

    fn store(&self) -> Result<SqliteSnapshotStore, CoreError> {
        SqliteSnapshotStore::open(self.database_path.clone()).map(|(store, _)| store)
    }

    fn materializer(&self) -> Result<SkillMaterializer, CoreError> {
        SkillMaterializer::new(
            self.database_path.clone(),
            &self.runtime_root,
            vec![
                ApprovedSkillRoot {
                    client: SkillClientName::Codex,
                    root: self.home.join(".codex/skills"),
                },
                ApprovedSkillRoot {
                    client: SkillClientName::ClaudeCode,
                    root: self.home.join(".claude/skills"),
                },
                ApprovedSkillRoot {
                    client: SkillClientName::AgentKit,
                    root: self.home.join(".agents/skills"),
                },
            ],
            TreeValidationPolicy::default(),
        )
    }
}

impl SkillLifecyclePort for RuntimeSkillLifecycle {
    fn load_authenticated_catalog(&self) -> Result<VerifiedSkillCatalog, CoreError> {
        load_current_authenticated_catalog(&self.database_path)
            .map_err(|error| CoreError::LifecycleEvidenceChanged(error.to_string()))
    }

    fn load_managed_receipts(
        &self,
        skill_id: &str,
    ) -> Result<Vec<(String, ManagedSkillReceipt)>, CoreError> {
        Ok(self
            .store()?
            .load_managed_skill_receipts()?
            .into_iter()
            .filter(|(_, receipt)| receipt.skill_id == skill_id)
            .collect())
    }

    fn load_available_backups(&self, skill_id: &str) -> Result<Vec<SkillBackupReceipt>, CoreError> {
        self.store()?.load_available_skill_backups(skill_id)
    }

    fn resolve(
        &self,
        source: &SkillSourceSpec,
        cancel: &CancelSignal,
    ) -> Result<SkillStagingEvidence, CoreError> {
        let executable = RealHostExecutableResolver
            .resolve_executable("git")
            .ok_or_else(|| {
                CoreError::CommandDenied("reviewed Git executable was not found".into())
            })?;
        let git = ReviewedGitExecutable::new(executable)?;
        PublicGithubSkillResolver::new(
            git,
            self.database_path.clone(),
            GitResolverLimits::default(),
        )?
        .resolve(source, cancel)
    }

    fn cleanup(&self, evidence: &SkillStagingEvidence) -> Result<(), CoreError> {
        cleanup_private_staging(&self.database_path, evidence)
    }

    fn materialize(
        &self,
        prepared: &PreparedSkillMutation,
        partial_policy: PartialFailurePolicy,
        recorded_at: &str,
    ) -> Result<SkillMutationOutcome, CoreError> {
        self.materializer()?
            .materialize(prepared, partial_policy, recorded_at)
    }

    fn export_diff(
        &self,
        prepared: &PreparedSkillMutation,
    ) -> Result<SkillMutationOutcome, CoreError> {
        let materializer = self.materializer()?;
        let export_root = self
            .database_path
            .parent()
            .ok_or_else(|| CoreError::InvalidPath("workspace database has no parent".into()))?
            .join(".stm-skill-exports");
        let mut targets = Vec::new();
        for (index, target) in prepared.targets.iter().enumerate() {
            let destination = export_root.join(format!("{}-{index}.json", prepared.operation_id));
            targets.push(materializer.export_target_diff(prepared, target, &destination)?);
        }
        Ok(SkillMutationOutcome {
            operation_id: prepared.operation_id.clone(),
            completed: targets.len(),
            failed: 0,
            partial_state_kept: false,
            skipped: 0,
            targets,
        })
    }

    fn restore_backup(
        &self,
        backup_id: &str,
        recorded_at: &str,
    ) -> Result<SkillTargetOutcome, CoreError> {
        self.materializer()?.restore_backup(backup_id, recorded_at)
    }

    fn recover_interrupted(&self, recorded_at: &str) -> Result<Vec<SkillTargetOutcome>, CoreError> {
        cleanup_abandoned_private_staging(&self.database_path)?;
        self.materializer()?.recover_interrupted(recorded_at)
    }
}

pub struct RuntimeMcpLifecycle {
    database_path: PathBuf,
    home: PathBuf,
}

impl RuntimeMcpLifecycle {
    pub fn new(database_path: PathBuf, home: PathBuf) -> Self {
        Self {
            database_path,
            home,
        }
    }

    fn store(&self) -> Result<SqliteSnapshotStore, CoreError> {
        SqliteSnapshotStore::open(self.database_path.clone()).map(|(store, _)| store)
    }

    fn materializer(&self) -> Result<McpConfigMaterializer, CoreError> {
        McpConfigMaterializer::new(&self.database_path, &self.home)
    }
}

impl McpLifecyclePort for RuntimeMcpLifecycle {
    fn client_config_path(&self, client: &McpClientName) -> PathBuf {
        client_config_path(&self.home, client)
    }

    fn compile_stdio(
        &self,
        command: &str,
        args: &[String],
    ) -> Result<Option<CompiledManagerCommand>, CoreError> {
        compile_mcp_stdio(command, args)
    }

    fn config_digest(&self, path: &Path) -> Result<Option<String>, CoreError> {
        config_digest(path)
    }

    fn load_available_backups(&self, server_id: &str) -> Result<Vec<McpBackupReceipt>, CoreError> {
        self.store()?.load_available_mcp_backups(server_id)
    }

    fn load_backup(&self, backup_id: &str) -> Result<Option<McpBackupReceipt>, CoreError> {
        self.store()?.load_mcp_backup(backup_id)
    }
    fn materialize(
        &self,
        prepared: &PreparedMcpMutation,
        executable_identities: &[ExecutableIdentity],
        recorded_at: &str,
    ) -> Result<McpMutationOutcome, CoreError> {
        let materializer = self.materializer()?;
        let mut outcome = materializer.materialize(prepared, recorded_at)?;
        if matches!(
            prepared.action,
            McpMutationAction::Add | McpMutationAction::Update | McpMutationAction::Enable
        ) {
            for target in &prepared.targets {
                let server = prepared
                    .server
                    .clients
                    .iter()
                    .find(|binding| binding.client == target.client)
                    .map_or_else(
                        || prepared.server.clone(),
                        |binding| binding.project_server(&prepared.server),
                    );
                let health =
                    crate::mcp::health::check_protocol_health(&server, executable_identities);
                persist_post_activation_health(
                    &mut outcome,
                    &target.client,
                    health,
                    |outcome, health| {
                        materializer.record_health(outcome, &target.client, health, recorded_at)
                    },
                );
            }
        }
        Ok(outcome)
    }

    fn restore_backup(
        &self,
        backup_id: &str,
        recorded_at: &str,
    ) -> Result<McpMutationOutcome, CoreError> {
        self.materializer()?.restore_backup(backup_id, recorded_at)
    }

    fn recover_interrupted(&self, recorded_at: &str) -> Result<(), CoreError> {
        self.materializer()?.recover_interrupted(recorded_at)
    }
}

fn persist_post_activation_health(
    outcome: &mut McpMutationOutcome,
    client: &McpClientName,
    health: McpHealthState,
    persist: impl FnOnce(&mut McpMutationOutcome, McpHealthState) -> Result<(), CoreError>,
) {
    if persist(outcome, health).is_err() {
        if let Some(target) = outcome
            .targets
            .iter_mut()
            .find(|target| &target.client == client)
        {
            target.health = McpHealthState::Unknown;
            target.redacted_detail = "Client configuration changed atomically; protocol health is unknown because post-activation health could not be persisted.".into();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use stm_core::mcp::lifecycle::{McpTargetOutcome, McpTargetStatus};
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn runtime_skill_materializer_does_not_depend_on_removed_source_checkout() {
        let temp = TempDir::new().expect("temporary runtime");
        let removed_source_checkout = temp.path().join("removed-worktree");
        fs::create_dir_all(&removed_source_checkout).expect("create source checkout");
        fs::remove_dir_all(&removed_source_checkout).expect("remove source checkout");

        let runtime_root = temp.path().join("runtime-data");
        let database_path = runtime_root.join("stm.sqlite");
        SqliteSnapshotStore::open(&database_path).expect("initialize runtime data");
        let home = temp.path().join("home");
        fs::create_dir_all(&home).expect("create test home");

        let lifecycle = RuntimeSkillLifecycle::new(database_path, runtime_root, home);

        assert!(!removed_source_checkout.exists());
        lifecycle
            .materializer()
            .expect("construct materializer from runtime-owned root");
    }

    #[test]
    fn health_persistence_failure_preserves_successful_mutation_outcome() {
        let mut outcome = McpMutationOutcome {
            operation_id: "operation-1".into(),
            completed: 1,
            failed: 0,
            targets: vec![McpTargetOutcome {
                client: McpClientName::ClaudeCode,
                status: McpTargetStatus::Success,
                receipt_id: Some("receipt-1".into()),
                backup_id: Some("backup-1".into()),
                health: McpHealthState::Unknown,
                redacted_detail: "Client configuration changed atomically.".into(),
            }],
        };

        persist_post_activation_health(
            &mut outcome,
            &McpClientName::ClaudeCode,
            McpHealthState::Healthy,
            |_, _| Err(CoreError::Sqlite("simulated health write failure".into())),
        );

        assert_eq!(outcome.completed, 1);
        assert_eq!(outcome.failed, 0);
        assert_eq!(outcome.targets.len(), 1);
        assert_eq!(outcome.targets[0].status, McpTargetStatus::Success);
        assert_eq!(outcome.targets[0].receipt_id.as_deref(), Some("receipt-1"));
        assert_eq!(outcome.targets[0].backup_id.as_deref(), Some("backup-1"));
        assert_eq!(outcome.targets[0].health, McpHealthState::Unknown);
        assert_eq!(
            outcome.targets[0].redacted_detail,
            "Client configuration changed atomically; protocol health is unknown because post-activation health could not be persisted."
        );
    }

    #[test]
    fn lifecycle_migrations_report_schema_version_five() {
        let temp = TempDir::new().expect("temporary database");
        let database_path = temp.path().join("stm.sqlite");

        let (store, opening_health) =
            SqliteSnapshotStore::open(database_path).expect("apply migrations");

        assert_eq!(opening_health.user_version, 5);
        assert_eq!(store.health().user_version, 5);
    }
}
