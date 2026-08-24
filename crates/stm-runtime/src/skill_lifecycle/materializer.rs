use super::{
    validate_staged_tree, BackupState, LocalConflictChoice, ManagedSkillReceipt,
    PartialFailurePolicy, PreparedSkillMutation, PreparedTargetMutation, SkillBackupReceipt,
    SkillMutationAction, SkillMutationOutcome, SkillRecoveryPhase, SkillRecoveryRecord,
    SkillStagingEvidence, SkillTargetOutcome, SkillTargetSpec, TargetMutationStatus,
    TreeValidationPolicy,
};
use crate::storage::SqliteSnapshotStore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    fs::OpenOptions,
    io::Write,
    path::{Component, Path, PathBuf},
};
use stm_core::{domain::skill::SkillClientName, CoreError};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApprovedSkillRoot {
    pub client: SkillClientName,
    pub root: PathBuf,
}

#[derive(Debug, Clone)]
struct ResolvedRoot {
    client: SkillClientName,
    declared: PathBuf,
    expected: PathBuf,
}

pub struct SkillMaterializer {
    store: SqliteSnapshotStore,
    db_parent: PathBuf,
    project_root: PathBuf,
    roots: Vec<ResolvedRoot>,
    policy: TreeValidationPolicy,
}

struct CompletedWrite {
    outcomes: Vec<SkillTargetOutcome>,
    target_path: PathBuf,
    physical_key: String,
    receipt_keys: Vec<String>,
    expected_digest: String,
    backup: Option<SkillBackupReceipt>,
    operation_id: String,
}

impl SkillMaterializer {
    pub fn new(
        workspace_db_path: impl Into<PathBuf>,
        project_root: impl AsRef<Path>,
        approved_roots: Vec<ApprovedSkillRoot>,
        policy: TreeValidationPolicy,
    ) -> Result<Self, CoreError> {
        let db_path = workspace_db_path.into();
        let db_parent = absolute_lexical(db_path.parent().ok_or_else(|| {
            CoreError::InvalidPath("workspace database must have a parent".into())
        })?)?;
        let project_root = fs::canonicalize(project_root)?;
        let mut roots = Vec::new();
        for root in approved_roots {
            let declared = absolute_lexical(&root.root)?;
            if declared.exists() && fs::symlink_metadata(&declared)?.file_type().is_symlink() {
                return Err(CoreError::PathEscape(declared));
            }
            let expected = resolve_future_path(&declared)?;
            if expected.starts_with(&project_root) {
                return Err(CoreError::ProjectRootRejected(expected));
            }
            if roots
                .iter()
                .any(|candidate: &ResolvedRoot| candidate.client == root.client)
            {
                return Err(CoreError::MalformedInput(
                    "each skill client must have exactly one approved root".into(),
                ));
            }
            roots.push(ResolvedRoot {
                client: root.client,
                declared,
                expected,
            });
        }
        if roots.is_empty() {
            return Err(CoreError::InvalidPath(
                "at least one approved global skill root is required".into(),
            ));
        }
        let (store, _) = SqliteSnapshotStore::open(&db_path)?;
        Ok(Self {
            store,
            db_parent,
            project_root,
            roots,
            policy,
        })
    }

    pub fn materialize(
        &self,
        prepared: &PreparedSkillMutation,
        partial_policy: PartialFailurePolicy,
        recorded_at: &str,
    ) -> Result<SkillMutationOutcome, CoreError> {
        self.revalidate_prepared(prepared)?;
        let mut grouped = BTreeMap::new();
        for target in &prepared.targets {
            let path = self.resolve_target(&target.target)?;
            grouped.entry(path).or_insert_with(Vec::new).push(target);
        }
        if grouped.is_empty() {
            return Err(CoreError::MalformedInput(
                "skill mutation has no targets".into(),
            ));
        }

        let groups = grouped.into_iter().collect::<Vec<_>>();
        let mut outcomes = Vec::new();
        let mut completed = Vec::new();
        let mut failed = false;
        for (group_index, (target_path, targets)) in groups.iter().enumerate() {
            match self.apply_group(prepared, target_path, targets, recorded_at) {
                Ok((group_outcomes, write)) => {
                    outcomes.extend(group_outcomes);
                    if let Some(write) = write {
                        completed.push(write);
                    }
                }
                Err(_) => {
                    failed = true;
                    outcomes.extend(targets.iter().map(|item| SkillTargetOutcome {
                        target: item.target.clone(),
                        status: TargetMutationStatus::Failed,
                        receipt_id: None,
                        backup_id: None,
                        redacted_detail: "The target was not changed because lifecycle evidence or filesystem state was unsafe.".into(),
                    }));
                    if partial_policy == PartialFailurePolicy::RollbackCompleted {
                        for (_, pending_targets) in groups.iter().skip(group_index + 1) {
                            outcomes.extend(pending_targets.iter().map(|item| SkillTargetOutcome {
                                target: item.target.clone(),
                                status: TargetMutationStatus::Skipped,
                                receipt_id: None,
                                backup_id: None,
                                redacted_detail: "The target was not attempted after an earlier target failed.".into(),
                            }));
                        }
                        break;
                    }
                }
            }
        }

        if failed && partial_policy == PartialFailurePolicy::RollbackCompleted {
            for write in completed.iter().rev() {
                let rolled_back = self.rollback_completed(write, recorded_at).is_ok();
                for original in &write.outcomes {
                    if let Some(outcome) = outcomes
                        .iter_mut()
                        .find(|value| value.target == original.target)
                    {
                        outcome.status = if rolled_back {
                            TargetMutationStatus::RolledBack
                        } else {
                            TargetMutationStatus::RecoveryRequired
                        };
                        outcome.redacted_detail = if rolled_back {
                            "The completed target was rolled back after another target failed."
                                .into()
                        } else {
                            "Automatic rollback could not complete; startup recovery remains recorded.".into()
                        };
                    }
                }
            }
        }

        let failed_count = outcomes
            .iter()
            .filter(|value| {
                matches!(
                    value.status,
                    TargetMutationStatus::Failed | TargetMutationStatus::RecoveryRequired
                )
            })
            .count();
        let skipped_count = outcomes
            .iter()
            .filter(|value| value.status == TargetMutationStatus::Skipped)
            .count();
        let completed_count = outcomes
            .iter()
            .filter(|value| {
                matches!(
                    value.status,
                    TargetMutationStatus::Installed
                        | TargetMutationStatus::Updated
                        | TargetMutationStatus::Restored
                        | TargetMutationStatus::NoOp
                        | TargetMutationStatus::KeptLocal
                        | TargetMutationStatus::DiffExported
                )
            })
            .count();
        Ok(SkillMutationOutcome {
            operation_id: prepared.operation_id.clone(),
            targets: outcomes,
            completed: completed_count,
            failed: failed_count,
            partial_state_kept: failed
                && partial_policy == PartialFailurePolicy::KeepPartial
                && !completed.is_empty(),
            skipped: skipped_count,
        })
    }

    pub fn recover_interrupted(
        &self,
        recorded_at: &str,
    ) -> Result<Vec<SkillTargetOutcome>, CoreError> {
        let mut outcomes = Vec::new();
        for mut recovery in self.store.load_skill_recoveries()? {
            let target_path = self.resolve_target(&recovery.target)?;
            if target_path != PathBuf::from(&recovery.target_path) {
                return Err(CoreError::PathEscape(target_path));
            }
            self.validate_sibling(
                &target_path,
                Path::new(&recovery.sibling_staging_path),
                ".stm-stage-",
            )?;
            let status = match recovery.phase {
                SkillRecoveryPhase::Prepared => {
                    remove_path_if_exists(Path::new(&recovery.sibling_staging_path))?;
                    self.store.commit_skill_removal(
                        &[],
                        &recovery.operation_id,
                        &recovery.target_key,
                    )?;
                    TargetMutationStatus::RolledBack
                }
                SkillRecoveryPhase::ExistingMovedToBackup
                | SkillRecoveryPhase::ReplacementActivated => {
                    let current = digest_if_valid(&target_path, self.policy);
                    if current.as_deref() == Some(&recovery.expected_tree_sha256) {
                        self.store.commit_skill_replacement(
                            &recovery.pending_receipts,
                            recovery.backup.as_ref(),
                            &recovery.operation_id,
                            &recovery.target_key,
                        )?;
                        TargetMutationStatus::Restored
                    } else if let Some(mut backup) = recovery.backup.take() {
                        backup.recorded_at = recorded_at.to_string();
                        self.restore_backup_record(&target_path, &mut backup)?;
                        TargetMutationStatus::RolledBack
                    } else if !target_path.exists() {
                        remove_path_if_exists(Path::new(&recovery.sibling_staging_path))?;
                        self.store.commit_skill_removal(
                            &[],
                            &recovery.operation_id,
                            &recovery.target_key,
                        )?;
                        TargetMutationStatus::RolledBack
                    } else {
                        TargetMutationStatus::RecoveryRequired
                    }
                }
            };
            outcomes.push(SkillTargetOutcome {
                target: recovery.target,
                status,
                receipt_id: None,
                backup_id: recovery
                    .backup
                    .as_ref()
                    .map(|value| value.backup_id.clone()),
                redacted_detail:
                    "Interrupted skill replacement was reconciled from durable recovery evidence."
                        .into(),
            });
        }
        Ok(outcomes)
    }

    pub fn export_target_diff(
        &self,
        prepared: &PreparedSkillMutation,
        target: &PreparedTargetMutation,
        destination: &Path,
    ) -> Result<SkillTargetOutcome, CoreError> {
        self.revalidate_prepared(prepared)?;
        if !prepared.targets.iter().any(|candidate| candidate == target) {
            return Err(CoreError::LifecycleEvidenceChanged(
                "diff target is not part of the authorized skill plan".into(),
            ));
        }
        let target_path = self.resolve_target(&target.target)?;
        let current = if target_path.exists() {
            digest_if_valid(&target_path, self.policy)
        } else {
            None
        };
        self.export_diff(
            destination.to_string_lossy().as_ref(),
            &target_path,
            current.as_deref(),
            &prepared.staging,
        )?;
        Ok(SkillTargetOutcome {
            target: target.target.clone(),
            status: TargetMutationStatus::DiffExported,
            receipt_id: None,
            backup_id: None,
            redacted_detail: "A redacted file digest diff was exported; no target files changed."
                .into(),
        })
    }

    pub fn restore_backup(
        &self,
        backup_id: &str,
        recorded_at: &str,
    ) -> Result<SkillTargetOutcome, CoreError> {
        let mut backup = self
            .store
            .load_skill_backup(backup_id)?
            .ok_or_else(|| CoreError::LifecycleOperationNotFound("skill backup receipt".into()))?;
        if backup.state != BackupState::Available {
            return Err(CoreError::LifecycleEvidenceChanged(
                "skill backup is not available".into(),
            ));
        }
        let receipt = backup
            .previous_receipts
            .first()
            .ok_or_else(|| {
                CoreError::LifecycleEvidenceChanged("backup has no managed receipt".into())
            })?
            .1
            .clone();
        let target_path = self.resolve_target(&receipt.target)?;
        let mut current_receipts = Vec::new();
        for key in &backup.replaced_target_keys {
            let current = self.store.load_managed_skill_receipt(key)?.ok_or_else(|| {
                CoreError::LifecycleEvidenceChanged("current managed receipt is missing".into())
            })?;
            current_receipts.push(current);
        }
        let current_digest = current_receipts
            .first()
            .ok_or_else(|| {
                CoreError::LifecycleEvidenceChanged(
                    "backup has no replacement receipt identity".into(),
                )
            })?
            .tree_sha256
            .clone();
        if current_receipts
            .iter()
            .any(|current| current.tree_sha256 != current_digest)
            || digest_if_valid(&target_path, self.policy).as_deref()
                != Some(current_digest.as_str())
        {
            return Err(CoreError::LifecycleEvidenceChanged(
                "local modification blocks backup restore".into(),
            ));
        }
        backup.recorded_at = recorded_at.to_string();
        self.restore_backup_record(&target_path, &mut backup)?;
        Ok(SkillTargetOutcome {
            target: receipt.target,
            status: TargetMutationStatus::Restored,
            receipt_id: Some(receipt.receipt_id),
            backup_id: Some(backup.backup_id),
            redacted_detail: "The receipt-backed managed revision was restored.".into(),
        })
    }

    fn apply_group(
        &self,
        prepared: &PreparedSkillMutation,
        target_path: &Path,
        targets: &[&PreparedTargetMutation],
        recorded_at: &str,
    ) -> Result<(Vec<SkillTargetOutcome>, Option<CompletedWrite>), CoreError> {
        let choice = &targets[0].conflict_choice;
        if targets.iter().any(|item| &item.conflict_choice != choice) {
            return Err(CoreError::MalformedInput(
                "shared physical target has conflicting choices".into(),
            ));
        }
        self.revalidate_target_path(target_path, &targets[0].target)?;
        let physical_key = path_key(target_path);
        let keys: Vec<_> = targets
            .iter()
            .map(|item| logical_key(&item.target.client, target_path))
            .collect();
        let relative = normalized_target_path(&targets[0].target.target_path)?;
        let mut physical_receipt_keys = self
            .roots
            .iter()
            .filter(|root| root.expected.join(&relative) == target_path)
            .map(|root| logical_key(&root.client, target_path))
            .collect::<BTreeSet<_>>();
        physical_receipt_keys.extend(keys.iter().cloned());
        let mut previous = Vec::new();
        for key in physical_receipt_keys {
            if let Some(receipt) = self.store.load_managed_skill_receipt(&key)? {
                previous.push((key, receipt));
            }
        }
        if previous
            .iter()
            .any(|value| value.1.skill_id != prepared.skill_id)
        {
            return Err(CoreError::LifecycleEvidenceChanged(
                "managed target belongs to a different skill".into(),
            ));
        }
        let exists = target_path.exists();
        if exists && fs::symlink_metadata(target_path)?.file_type().is_symlink() {
            return Err(CoreError::PathEscape(target_path.to_path_buf()));
        }
        let current_digest = if exists {
            digest_if_valid(target_path, self.policy)
        } else {
            None
        };
        let managed_digest = previous.first().map(|value| value.1.tree_sha256.clone());
        if previous
            .iter()
            .any(|value| Some(&value.1.tree_sha256) != managed_digest.as_ref())
        {
            return Err(CoreError::LifecycleEvidenceChanged(
                "managed receipts disagree".into(),
            ));
        }

        let receipts = self.new_receipts(prepared, targets, target_path, recorded_at);
        if exists
            && current_digest.as_deref() == Some(&prepared.source.tree_sha256)
            && managed_digest.is_some()
        {
            self.store.commit_skill_replacement(
                &receipts,
                None,
                &prepared.operation_id,
                &physical_key,
            )?;
            return Ok((
                outcomes_for(
                    targets,
                    TargetMutationStatus::NoOp,
                    &receipts,
                    None,
                    "The selected revision is already present; no files changed.",
                ),
                None,
            ));
        }
        if previous.iter().any(|(key, _)| !keys.contains(key)) {
            return Err(CoreError::LifecycleEvidenceChanged(
                "shared managed target has unselected logical clients".into(),
            ));
        }
        if exists && previous.is_empty() {
            return Err(CoreError::LifecycleEvidenceChanged(
                "target is not owned by a managed receipt".into(),
            ));
        }
        if !exists && previous.is_empty() && prepared.action != SkillMutationAction::Install {
            return Err(CoreError::LifecycleEvidenceChanged(
                "update target has no managed receipt".into(),
            ));
        }
        if exists && prepared.action == SkillMutationAction::Install {
            return Err(CoreError::LifecycleEvidenceChanged(
                "install target already has a different managed revision".into(),
            ));
        }

        if exists && current_digest.as_ref() != managed_digest.as_ref() {
            match choice {
                LocalConflictChoice::Block | LocalConflictChoice::RestoreManaged
                    if previous.is_empty() =>
                {
                    return Err(CoreError::LifecycleEvidenceChanged(
                        "local modification blocks replacement".into(),
                    ))
                }
                LocalConflictChoice::Block => {
                    return Err(CoreError::LifecycleEvidenceChanged(
                        "local modification blocks replacement".into(),
                    ))
                }
                LocalConflictChoice::KeepLocal => {
                    return Ok((
                        outcomes_for(
                            targets,
                            TargetMutationStatus::KeptLocal,
                            &[],
                            None,
                            "Local changes were kept; no files changed.",
                        ),
                        None,
                    ))
                }
                LocalConflictChoice::ExportDiff { destination } => {
                    self.export_diff(
                        destination,
                        target_path,
                        current_digest.as_deref(),
                        &prepared.staging,
                    )?;
                    return Ok((
                        outcomes_for(
                            targets,
                            TargetMutationStatus::DiffExported,
                            &[],
                            None,
                            "A redacted file digest diff was exported; no target files changed.",
                        ),
                        None,
                    ));
                }
                LocalConflictChoice::RestoreManaged => {}
            }
        }

        let parent = target_path
            .parent()
            .ok_or_else(|| CoreError::InvalidPath("target has no parent".into()))?;
        fs::create_dir_all(parent)?;
        self.revalidate_target_path(target_path, &targets[0].target)?;
        let token = short_id(&format!("{}\0{}", prepared.operation_id, physical_key));
        let sibling = parent.join(format!(".stm-stage-{token}"));
        let backup_path = parent.join(format!(".stm-backup-{token}"));
        remove_path_if_exists(&sibling)?;
        copy_staged_tree(
            Path::new(&prepared.staging.private_staging_path),
            &sibling,
            &prepared.staging,
        )?;
        let copied = validate_staged_tree(&sibling, self.policy)?;
        if copied.tree_sha256 != prepared.source.tree_sha256 {
            remove_path_if_exists(&sibling)?;
            return Err(CoreError::LifecycleEvidenceChanged(
                "sibling staging revalidation failed".into(),
            ));
        }

        let backup = if exists {
            Some(SkillBackupReceipt {
                backup_id: format!("skill-backup-{token}"),
                operation_id: prepared.operation_id.clone(),
                target_key: physical_key.clone(),
                backup_path: backup_path.display().to_string(),
                backup_tree_sha256: backup_tree_digest(target_path, self.policy)?,
                previous_receipts: previous.clone(),
                replaced_target_keys: keys.clone(),
                state: BackupState::Available,
                recorded_at: recorded_at.into(),
            })
        } else {
            None
        };
        let mut recovery = SkillRecoveryRecord {
            operation_id: prepared.operation_id.clone(),
            target_key: physical_key.clone(),
            target: targets[0].target.clone(),
            target_path: target_path.display().to_string(),
            sibling_staging_path: sibling.display().to_string(),
            backup: backup.clone(),
            pending_receipts: receipts.clone(),
            expected_tree_sha256: prepared.source.tree_sha256.clone(),
            phase: SkillRecoveryPhase::Prepared,
            recorded_at: recorded_at.into(),
        };
        self.store.persist_skill_recovery(&recovery)?;
        if exists {
            remove_path_if_exists(&backup_path)?;
            fs::rename(target_path, &backup_path)?;
            recovery.phase = SkillRecoveryPhase::ExistingMovedToBackup;
            self.store.persist_skill_recovery(&recovery)?;
        }
        if let Err(error) = fs::rename(&sibling, target_path) {
            if backup_path.exists() && !target_path.exists() {
                let _ = fs::rename(&backup_path, target_path);
            }
            return Err(error.into());
        }
        recovery.phase = SkillRecoveryPhase::ReplacementActivated;
        self.store.persist_skill_recovery(&recovery)?;
        if digest_if_valid(target_path, self.policy).as_deref()
            != Some(&prepared.source.tree_sha256)
        {
            return Err(CoreError::LifecycleEvidenceChanged(
                "activated target digest changed".into(),
            ));
        }
        self.store.commit_skill_replacement(
            &receipts,
            backup.as_ref(),
            &prepared.operation_id,
            &physical_key,
        )?;
        let status = match prepared.action {
            SkillMutationAction::Install => TargetMutationStatus::Installed,
            SkillMutationAction::Update => TargetMutationStatus::Updated,
            SkillMutationAction::RestoreManaged => TargetMutationStatus::Restored,
        };
        let group_outcomes = outcomes_for(
            targets,
            status,
            &receipts,
            backup.as_ref(),
            "The managed skill target was atomically replaced and verified.",
        );
        Ok((
            group_outcomes.clone(),
            Some(CompletedWrite {
                outcomes: group_outcomes,
                target_path: target_path.to_path_buf(),
                physical_key,
                receipt_keys: keys,
                expected_digest: prepared.source.tree_sha256.clone(),
                backup,
                operation_id: prepared.operation_id.clone(),
            }),
        ))
    }

    fn new_receipts(
        &self,
        prepared: &PreparedSkillMutation,
        targets: &[&PreparedTargetMutation],
        target_path: &Path,
        recorded_at: &str,
    ) -> Vec<(String, ManagedSkillReceipt)> {
        targets
            .iter()
            .map(|item| {
                let key = logical_key(&item.target.client, target_path);
                let receipt = ManagedSkillReceipt {
                    receipt_id: format!(
                        "skill-receipt-{}",
                        short_id(&format!(
                            "{}\0{}\0{}",
                            prepared.operation_id, key, prepared.source.tree_sha256
                        ))
                    ),
                    operation_id: prepared.operation_id.clone(),
                    skill_id: prepared.skill_id.clone(),
                    target: item.target.clone(),
                    source: prepared.source.clone(),
                    tree_sha256: prepared.source.tree_sha256.clone(),
                    file_manifest: prepared.staging.files.clone(),
                    recorded_at: recorded_at.into(),
                };
                (key, receipt)
            })
            .collect()
    }

    fn rollback_completed(
        &self,
        write: &CompletedWrite,
        _recorded_at: &str,
    ) -> Result<(), CoreError> {
        if digest_if_valid(&write.target_path, self.policy).as_deref()
            != Some(&write.expected_digest)
        {
            return Err(CoreError::LifecycleEvidenceChanged(
                "completed target changed before rollback".into(),
            ));
        }
        if let Some(mut backup) = write.backup.clone() {
            self.restore_backup_record(&write.target_path, &mut backup)
        } else {
            remove_path_if_exists(&write.target_path)?;
            self.store.commit_skill_removal(
                &write.receipt_keys,
                &write.operation_id,
                &write.physical_key,
            )
        }
    }

    fn restore_backup_record(
        &self,
        target_path: &Path,
        backup: &mut SkillBackupReceipt,
    ) -> Result<(), CoreError> {
        let backup_path = PathBuf::from(&backup.backup_path);
        self.validate_sibling(target_path, &backup_path, ".stm-backup-")?;
        if !backup_path.exists() {
            return Err(CoreError::LifecycleEvidenceChanged(
                "receipt-backed backup is missing".into(),
            ));
        }
        let receipt_expected = backup
            .previous_receipts
            .first()
            .ok_or_else(|| {
                CoreError::LifecycleEvidenceChanged(
                    "backup receipt has no previous revision".into(),
                )
            })?
            .1
            .tree_sha256
            .clone();
        if backup
            .previous_receipts
            .iter()
            .any(|value| value.1.tree_sha256 != receipt_expected)
        {
            return Err(CoreError::LifecycleEvidenceChanged(
                "backup receipts disagree about the previous revision".into(),
            ));
        }
        let expected = if backup.backup_tree_sha256.is_empty() {
            receipt_expected
        } else {
            backup.backup_tree_sha256.clone()
        };
        if backup_tree_digest(&backup_path, self.policy)? != expected {
            return Err(CoreError::LifecycleEvidenceChanged(
                "receipt-backed backup digest changed".into(),
            ));
        }
        let discard = target_path
            .parent()
            .unwrap()
            .join(format!(".stm-discard-{}", short_id(&backup.backup_id)));
        remove_path_if_exists(&discard)?;
        if target_path.exists() {
            fs::rename(target_path, &discard)?;
        }
        if let Err(error) = fs::rename(&backup_path, target_path) {
            if discard.exists() {
                let _ = fs::rename(&discard, target_path);
            }
            return Err(error.into());
        }
        if backup_tree_digest(target_path, self.policy)? != expected {
            let _ = fs::rename(target_path, &backup_path);
            if discard.exists() {
                let _ = fs::rename(&discard, target_path);
            }
            return Err(CoreError::LifecycleEvidenceChanged(
                "restored backup digest mismatch".into(),
            ));
        }
        remove_path_if_exists(&discard)?;
        backup.state = BackupState::Restored;
        self.store.commit_skill_backup_restore(
            &backup.replaced_target_keys,
            &backup.previous_receipts,
            backup,
        )
    }

    fn revalidate_prepared(&self, prepared: &PreparedSkillMutation) -> Result<(), CoreError> {
        if prepared.operation_id.is_empty()
            || prepared.skill_id.is_empty()
            || prepared.source.tree_sha256 != prepared.staging.tree_sha256
        {
            return Err(CoreError::LifecycleEvidenceChanged(
                "prepared skill evidence is incomplete".into(),
            ));
        }
        let path = fs::canonicalize(&prepared.staging.private_staging_path)?;
        let staging_root = fs::canonicalize(self.db_parent.join(".stm-skill-staging"))?;
        let operation = path
            .parent()
            .ok_or_else(|| CoreError::InvalidPath("invalid private staging path".into()))?;
        if operation.parent() != Some(staging_root.as_path())
            || path.file_name().and_then(|value| value.to_str()) != Some("tree")
        {
            return Err(CoreError::PathEscape(path));
        }
        let current = validate_staged_tree(&path, self.policy)?;
        if current.tree_sha256 != prepared.source.tree_sha256
            || current.manifest != prepared.staging.manifest
            || current.files != prepared.staging.files
            || current.risk != prepared.staging.risk
        {
            return Err(CoreError::LifecycleEvidenceChanged(
                "private staging evidence changed".into(),
            ));
        }
        Ok(())
    }

    fn resolve_target(&self, target: &SkillTargetSpec) -> Result<PathBuf, CoreError> {
        let relative = normalized_target_path(&target.target_path)?;
        let root = self
            .roots
            .iter()
            .find(|root| root.client == target.client)
            .ok_or_else(|| CoreError::PathEscape(PathBuf::from(&target.target_path)))?;
        let path = root.expected.join(relative);
        if !path.starts_with(&root.expected) || path.starts_with(&self.project_root) {
            return Err(CoreError::ProjectRootRejected(path));
        }
        Ok(path)
    }

    fn revalidate_target_path(
        &self,
        target_path: &Path,
        target: &SkillTargetSpec,
    ) -> Result<(), CoreError> {
        let root = self
            .roots
            .iter()
            .find(|root| root.client == target.client)
            .ok_or_else(|| CoreError::PathEscape(target_path.to_path_buf()))?;
        if root.declared.exists() {
            let metadata = fs::symlink_metadata(&root.declared)?;
            if metadata.file_type().is_symlink()
                || fs::canonicalize(&root.declared)? != root.expected
            {
                return Err(CoreError::PathEscape(root.declared.clone()));
            }
        }
        if !target_path.starts_with(&root.expected) || target_path.starts_with(&self.project_root) {
            return Err(CoreError::ProjectRootRejected(target_path.to_path_buf()));
        }
        let relative = target_path
            .strip_prefix(&root.expected)
            .map_err(|_| CoreError::PathEscape(target_path.to_path_buf()))?;
        let mut cursor = root.expected.clone();
        for component in relative.components() {
            cursor.push(component);
            if cursor.exists() && fs::symlink_metadata(&cursor)?.file_type().is_symlink() {
                return Err(CoreError::PathEscape(cursor));
            }
        }
        Ok(())
    }

    fn validate_sibling(
        &self,
        target: &Path,
        sibling: &Path,
        prefix: &str,
    ) -> Result<(), CoreError> {
        if sibling.parent() != target.parent()
            || !sibling
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.starts_with(prefix))
        {
            return Err(CoreError::PathEscape(sibling.to_path_buf()));
        }
        Ok(())
    }

    fn export_diff(
        &self,
        destination: &str,
        target: &Path,
        current: Option<&str>,
        staging: &SkillStagingEvidence,
    ) -> Result<(), CoreError> {
        let destination = absolute_lexical(Path::new(destination))?;
        let export_root = self.db_parent.join(".stm-skill-exports");
        if !destination.starts_with(&export_root)
            || destination.starts_with(target)
            || self
                .roots
                .iter()
                .any(|root| destination.starts_with(&root.expected))
        {
            return Err(CoreError::PathEscape(destination));
        }
        fs::create_dir_all(&export_root)?;
        let payload = serde_json::json!({ "schemaVersion": 1, "currentTreeSha256": current, "preparedTreeSha256": staging.tree_sha256, "preparedFiles": staging.files });
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)?;
        file.write_all(&serde_json::to_vec_pretty(&payload)?)?;
        file.sync_all()?;
        Ok(())
    }
}

fn outcomes_for(
    targets: &[&PreparedTargetMutation],
    status: TargetMutationStatus,
    receipts: &[(String, ManagedSkillReceipt)],
    backup: Option<&SkillBackupReceipt>,
    detail: &str,
) -> Vec<SkillTargetOutcome> {
    targets
        .iter()
        .map(|item| SkillTargetOutcome {
            target: item.target.clone(),
            status: status.clone(),
            receipt_id: receipts
                .iter()
                .find(|value| value.1.target == item.target)
                .map(|value| value.1.receipt_id.clone()),
            backup_id: backup.map(|value| value.backup_id.clone()),
            redacted_detail: detail.into(),
        })
        .collect()
}

fn backup_tree_digest(root: &Path, policy: TreeValidationPolicy) -> Result<String, CoreError> {
    let metadata = fs::symlink_metadata(root)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(CoreError::InvalidPath(
            "skill backup root must be a real directory".into(),
        ));
    }
    let mut files = Vec::new();
    let mut total_bytes = 0_u64;
    collect_backup_files(root, root, policy, &mut files, &mut total_bytes)?;
    files.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let mut hasher = Sha256::new();
    for (relative, path, size) in files {
        let bytes = fs::read(path)?;
        if bytes.len() as u64 != size {
            return Err(CoreError::LifecycleEvidenceChanged(
                "skill backup changed during verification".into(),
            ));
        }
        let relative_bytes = relative.as_bytes();
        let relative_len = u32::try_from(relative_bytes.len())
            .map_err(|_| CoreError::InvalidPath("skill backup path is too long".into()))?;
        hasher.update(relative_len.to_be_bytes());
        hasher.update(relative_bytes);
        hasher.update(size.to_be_bytes());
        hasher.update(bytes);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_backup_files(
    root: &Path,
    cursor: &Path,
    policy: TreeValidationPolicy,
    files: &mut Vec<(String, PathBuf, u64)>,
    total_bytes: &mut u64,
) -> Result<(), CoreError> {
    for entry in fs::read_dir(cursor)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(CoreError::PathEscape(path));
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| CoreError::PathEscape(path.clone()))?;
        let relative = normalized_target_path(
            relative
                .to_str()
                .ok_or_else(|| CoreError::InvalidPath("skill backup path is not UTF-8".into()))?,
        )?;
        if relative.components().count() > policy.max_depth {
            return Err(CoreError::InvalidPath(
                "skill backup exceeds path depth limit".into(),
            ));
        }
        if metadata.file_type().is_dir() {
            collect_backup_files(root, &path, policy, files, total_bytes)?;
        } else if metadata.file_type().is_file() {
            if metadata.len() > policy.max_file_bytes || files.len() >= policy.max_files {
                return Err(CoreError::MalformedInput(
                    "skill backup exceeds bounded file limits".into(),
                ));
            }
            *total_bytes = total_bytes
                .checked_add(metadata.len())
                .ok_or_else(|| CoreError::MalformedInput("skill backup size overflow".into()))?;
            if *total_bytes > policy.max_total_bytes {
                return Err(CoreError::MalformedInput(
                    "skill backup exceeds total size limit".into(),
                ));
            }
            files.push((relative.to_string_lossy().to_string(), path, metadata.len()));
        } else {
            return Err(CoreError::InvalidPath(
                "skill backup contains a special file".into(),
            ));
        }
    }
    Ok(())
}

fn digest_if_valid(path: &Path, policy: TreeValidationPolicy) -> Option<String> {
    validate_staged_tree(path, policy)
        .ok()
        .map(|value| value.tree_sha256)
}
fn copy_staged_tree(
    source: &Path,
    destination: &Path,
    evidence: &SkillStagingEvidence,
) -> Result<(), CoreError> {
    fs::create_dir(destination)?;
    for file in &evidence.files {
        let relative = normalized_target_path(&file.path)?;
        let from = source.join(&relative);
        if !fs::symlink_metadata(&from)?.file_type().is_file() {
            return Err(CoreError::LifecycleEvidenceChanged(
                "staged source file type changed".into(),
            ));
        }
        let to = destination.join(relative);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(from, to)?;
    }
    Ok(())
}
fn normalized_target_path(value: &str) -> Result<PathBuf, CoreError> {
    if value.is_empty() || value.contains('\\') || value.contains('\0') {
        return Err(CoreError::InvalidPath(
            "target path is not normalized".into(),
        ));
    }
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(CoreError::InvalidPath(
            "target path contains traversal".into(),
        ));
    }
    if value
        .split('/')
        .any(|part| part.is_empty() || part.starts_with(".stm-"))
    {
        return Err(CoreError::InvalidPath(
            "target path contains a reserved component".into(),
        ));
    }
    Ok(path.to_path_buf())
}
fn absolute_lexical(path: &Path) -> Result<PathBuf, CoreError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(CoreError::InvalidPath(
            "path must be absolute and normalized".into(),
        ));
    }
    Ok(path.to_path_buf())
}
fn resolve_future_path(path: &Path) -> Result<PathBuf, CoreError> {
    let mut existing = path;
    let mut missing = Vec::new();
    while !existing.exists() {
        let name = existing
            .file_name()
            .ok_or_else(|| CoreError::InvalidPath("path has no existing ancestor".into()))?;
        missing.push(name.to_os_string());
        existing = existing
            .parent()
            .ok_or_else(|| CoreError::InvalidPath("path has no existing ancestor".into()))?;
    }
    let mut resolved = fs::canonicalize(existing)?;
    for part in missing.into_iter().rev() {
        resolved.push(part);
    }
    Ok(resolved)
}
fn logical_key(client: &SkillClientName, path: &Path) -> String {
    short_id(&format!("{}\0{}", client_label(client), path.display()))
}
fn path_key(path: &Path) -> String {
    short_id(&path.display().to_string())
}
fn client_label(client: &SkillClientName) -> &'static str {
    match client {
        SkillClientName::Codex => "Codex",
        SkillClientName::ClaudeCode => "Claude Code",
        SkillClientName::AgentKit => "AgentKit",
    }
}
fn short_id(value: &str) -> String {
    super::digest::hex_digest(Sha256::digest(value.as_bytes()).as_slice())
}
fn remove_path_if_exists(path: &Path) -> Result<(), CoreError> {
    if !path.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    fn staged(_temp: &TempDir, db: &Path) -> SkillStagingEvidence {
        let tree = db
            .parent()
            .unwrap()
            .join(".stm-skill-staging/resolve-test/tree");
        fs::create_dir_all(&tree).unwrap();
        fs::write(
            tree.join("SKILL.md"),
            "---\nname: safe\ndescription: Safe\n---\n# Safe\n",
        )
        .unwrap();
        validate_staged_tree(&tree, TreeValidationPolicy::default()).unwrap()
    }
    #[test]
    fn installs_updates_blocks_local_changes_and_rolls_back() {
        let temp = TempDir::new().unwrap();
        let db = temp.path().join("state/stm.sqlite");
        fs::create_dir_all(db.parent().unwrap()).unwrap();
        let root = temp.path().join("global");
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let materializer = SkillMaterializer::new(
            &db,
            &project,
            vec![ApprovedSkillRoot {
                client: SkillClientName::Codex,
                root: root.clone(),
            }],
            TreeValidationPolicy::default(),
        )
        .unwrap();
        let evidence = staged(&temp, &db);
        let source = super::super::SkillSourceSpec {
            repository: "https://github.com/o/r.git".into(),
            subpath: "skills/safe".into(),
            commit: "a".repeat(40),
            tree_sha256: evidence.tree_sha256.clone(),
        };
        let prepared = PreparedSkillMutation {
            operation_id: "install".into(),
            skill_id: "safe".into(),
            action: SkillMutationAction::Install,
            source,
            staging: evidence,
            targets: vec![PreparedTargetMutation {
                target: SkillTargetSpec {
                    client: SkillClientName::Codex,
                    target_path: "safe".into(),
                },
                conflict_choice: LocalConflictChoice::Block,
            }],
        };
        let result = materializer
            .materialize(&prepared, PartialFailurePolicy::RollbackCompleted, "now")
            .unwrap();
        assert_eq!(result.failed, 0);
        fs::write(root.join("safe/local.txt"), "changed").unwrap();
        let mut update = prepared.clone();
        update.operation_id = "update".into();
        update.action = SkillMutationAction::Update;
        assert_eq!(
            materializer
                .materialize(&update, PartialFailurePolicy::RollbackCompleted, "later")
                .unwrap()
                .failed,
            1
        );
    }
    #[test]
    fn rejects_project_root_and_target_symlink_escape() {
        let temp = TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let db = temp.path().join("stm.sqlite");
        assert!(SkillMaterializer::new(
            &db,
            &project,
            vec![ApprovedSkillRoot {
                client: SkillClientName::Codex,
                root: project.join("skills")
            }],
            TreeValidationPolicy::default()
        )
        .is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let root = temp.path().join("global");
            fs::create_dir(&root).unwrap();
            symlink(&project, root.join("escape")).unwrap();
            let m = SkillMaterializer::new(
                &db,
                &project,
                vec![ApprovedSkillRoot {
                    client: SkillClientName::Codex,
                    root,
                }],
                TreeValidationPolicy::default(),
            )
            .unwrap();
            assert!(m
                .resolve_target(&SkillTargetSpec {
                    client: SkillClientName::Codex,
                    target_path: "escape/x".into()
                })
                .is_ok());
            assert!(m
                .revalidate_target_path(
                    &m.resolve_target(&SkillTargetSpec {
                        client: SkillClientName::Codex,
                        target_path: "escape/x".into()
                    })
                    .unwrap(),
                    &SkillTargetSpec {
                        client: SkillClientName::Codex,
                        target_path: "escape/x".into()
                    }
                )
                .is_err());
        }
    }
}
