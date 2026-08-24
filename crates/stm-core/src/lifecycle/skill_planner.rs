use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde_json::json;

use crate::{
    adapters::{compute_sha256, FixtureWorkspace},
    domain::{
        lifecycle::{
            LifecycleExecution, LifecyclePlan, LifecyclePlanRequest, LifecyclePrivilege,
            LifecycleRevalidation, LifecycleRevalidationState,
        },
        skill::SkillClientName,
        source::{SourceAnalysisStatus, SourceTrust},
    },
    error::CoreError,
    feasibility::process_supervisor::CancelSignal,
    ports::SkillLifecyclePort,
    skill_catalog::TrustedSkillEntry,
    skill_lifecycle::{
        LocalConflictChoice, PreparedSkillMutation, PreparedTargetMutation, SkillMutationAction,
        SkillSourceSpec, SkillStagingEvidence, SkillTargetSpec,
    },
};

use super::{
    planner::{
        opaque_plan_id, plan_digest, prepare_review_only, PreparedPlan, PreparedSkillAction,
    },
    source_registry::SourceAnalysisBinding,
    time::{format_timestamp, PLAN_TTL},
};

pub(super) fn prepare_skill(
    workspace: &FixtureWorkspace,
    lifecycle: &dyn SkillLifecyclePort,
    request: LifecyclePlanRequest,
    source_analysis: Option<&SourceAnalysisBinding>,
    sequence: u64,
    now: SystemTime,
) -> Result<PreparedPlan, CoreError> {
    let action = normalized_action(&request.action).to_string();
    if action == "review" || action == "resolve-partial" {
        return prepare_review_only(
            request,
            sequence,
            now,
            "Choose a concrete skill lifecycle action before execution.",
        );
    }

    let receipts = lifecycle.load_managed_receipts(&request.resource_id)?;

    if action == "rollback" {
        let backups = lifecycle.load_available_backups(&request.resource_id)?;
        let Some(backup) = backups.first() else {
            return prepare_review_only(
                request,
                sequence,
                now,
                "No available receipt-backed skill backup can be restored.",
            );
        };
        return build_rollback_plan(request, sequence, now, backup.backup_id.clone(), receipts);
    }
    if action == "keep-partial" {
        return build_keep_partial_plan(request, sequence, now, receipts);
    }

    let verified = lifecycle.load_authenticated_catalog().map_err(|_| {
        CoreError::LifecycleEvidenceChanged(
            "no authenticated trusted skill catalog is available".to_string(),
        )
    })?;
    let entry = verified
        .catalog
        .skills
        .iter()
        .find(|entry| entry.id == request.resource_id)
        .ok_or_else(|| {
            CoreError::LifecyclePlanNotFound(format!(
                "trusted skill catalog has no entry for {}",
                request.resource_id
            ))
        })?;
    authorize_source(source_analysis, entry)?;

    if action == "install" && !receipts.is_empty() {
        return prepare_review_only(
            request,
            sequence,
            now,
            "This skill already has managed receipts; review an update instead.",
        );
    }
    if matches!(
        action.as_str(),
        "update" | "restore-managed" | "export-diff" | "keep-local" | "retry"
    ) && receipts.is_empty()
    {
        return prepare_review_only(
            request,
            sequence,
            now,
            "This action requires an existing receipt-backed managed skill.",
        );
    }

    let source = if action == "restore-managed" {
        receipt_source(&receipts)?
    } else {
        source_spec(entry)
    };
    let cancel = CancelSignal::default();
    let staging = lifecycle.resolve(&source, &cancel)?;
    let result = build_materialization_plan(
        workspace,
        request,
        sequence,
        now,
        verified.catalog.catalog_version,
        &verified.payload_sha256,
        entry,
        source,
        staging.clone(),
        receipts,
        &action,
    );
    if result.is_err() {
        let _ = lifecycle.cleanup(&staging);
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn build_materialization_plan(
    workspace: &FixtureWorkspace,
    request: LifecyclePlanRequest,
    sequence: u64,
    now: SystemTime,
    catalog_version: u64,
    catalog_payload_sha256: &str,
    entry: &TrustedSkillEntry,
    source: SkillSourceSpec,
    staging: SkillStagingEvidence,
    receipts: Vec<(String, crate::skill_lifecycle::ManagedSkillReceipt)>,
    action: &str,
) -> Result<PreparedPlan, CoreError> {
    let plan_id = opaque_plan_id(sequence, &request, now)?;
    let target_filter = request.item_ids.as_ref().map(|items| {
        items
            .iter()
            .map(|item| item.to_ascii_lowercase())
            .collect::<BTreeSet<_>>()
    });
    let mut targets = Vec::new();
    for target in &entry.targets {
        let client = client_label(&target.client);
        if let Some(filter) = &target_filter {
            let selected = filter.contains(&client.to_ascii_lowercase())
                || filter.contains(&target.relative_path.to_ascii_lowercase());
            if !selected {
                continue;
            }
        }
        let mut target_path = catalog_target_suffix(&target.client, &target.relative_path)?;
        if action == "install-side-by-side" {
            target_path = format!(
                "{target_path}-stm-{}",
                &entry.source.commit[..entry.source.commit.len().min(12)]
            );
        }
        targets.push(PreparedTargetMutation {
            target: SkillTargetSpec {
                client: target.client.clone(),
                target_path,
            },
            conflict_choice: match action {
                "keep-local" => LocalConflictChoice::KeepLocal,
                "restore-managed" => LocalConflictChoice::RestoreManaged,
                _ => LocalConflictChoice::Block,
            },
        });
    }
    if targets.is_empty() {
        return Err(CoreError::MalformedInput(
            "no approved skill targets were selected".to_string(),
        ));
    }

    let mutation_action = match action {
        "install" | "install-side-by-side" => SkillMutationAction::Install,
        "restore-managed" => SkillMutationAction::RestoreManaged,
        _ => SkillMutationAction::Update,
    };

    let prepared = PreparedSkillMutation {
        operation_id: plan_id.clone(),
        skill_id: entry.id.clone(),
        action: mutation_action,
        source,
        staging: staging.clone(),
        targets,
    };

    let mut affected_paths = prepared
        .targets
        .iter()
        .map(|target| resolved_target_path(workspace, &target.target))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    affected_paths.sort();
    affected_paths.dedup();
    let mut limitations = vec![
        format!(
            "Authenticated catalog {} payload {} selected immutable commit {}.",
            catalog_version, catalog_payload_sha256, prepared.source.commit
        ),
        format!(
            "Staged {} files ({} bytes) with tree digest {}.",
            staging.file_count, staging.total_bytes, staging.tree_sha256
        ),
    ];
    limitations.extend(file_diff_lines(&staging, &receipts));
    for script in &staging.risk.scripts {
        limitations.push(format!("Review script or executable content: {script}"));
    }
    for requirement in &staging.risk.requirements {
        limitations.push(format!("Review dependency declaration: {requirement}"));
    }
    limitations.push(
        "Revalidation checks catalog, staging digest, current targets, and managed receipts immediately before mutation."
            .to_string(),
    );

    let current_version =
        common_receipt_version(&receipts).unwrap_or_else(|| "Not installed".to_string());
    let label = match action {
        "install" => format!("Install {}", entry.name),
        "restore-managed" => format!("Restore managed {}", entry.name),
        "keep-local" => format!("Keep local {} and record the conflict", entry.name),
        "install-side-by-side" => format!("Install {} side by side", entry.name),
        "retry" => format!("Retry failed {} targets", entry.name),
        "export-diff" => format!("Export {} conflict diff", entry.name),
        _ => format!("Update {}", entry.name),
    };
    let fingerprint = compute_sha256([serde_json::to_vec(&json!({
        "catalogVersion": catalog_version,
        "catalogPayloadSha256": catalog_payload_sha256,
        "entry": entry,
        "request": request,
        "treeSha256": staging.tree_sha256,
        "files": staging.files,
        "targets": prepared.targets,
        "receipts": receipts,
    }))?]);
    let issued_at = format_timestamp(now)?;
    let expires_at = format_timestamp(now + PLAN_TTL)?;
    let mut plan = LifecyclePlan {
        request: request.clone(),
        plan_id,
        canonical_id: entry.id.clone(),
        mapping_id: format!("skill-catalog:{}:{}", catalog_version, entry.id),
        resource_id: entry.id.clone(),
        owner: "STM managed skill lifecycle".to_string(),
        source: format!(
            "{}#{}: {}",
            prepared.source.repository, prepared.source.commit, prepared.source.subpath
        ),
        current_version,
        target_version: format!(
            "Git {}",
            &entry.source.commit[..entry.source.commit.len().min(12)]
        ),
        privilege: LifecyclePrivilege::UserConfirmation,
        affected_paths,
        affected_records: prepared
            .targets
            .iter()
            .map(|target| {
                format!(
                    "managed-skill:{}:{}",
                    client_label(&target.target.client),
                    entry.id
                )
            })
            .collect(),
        confidence: "Authenticated catalog provenance and immutable Git tree verified".to_string(),
        limitations,
        digest: String::new(),
        expires_at: expires_at.clone(),
        revalidation: LifecycleRevalidation {
            state: LifecycleRevalidationState::Fresh,
            checked_at: issued_at,
            checks: vec![
                "Authenticated catalog version, manifest signature, and pinned source are unchanged."
                    .to_string(),
                "Staged tree digest, target ownership, and managed receipts are rechecked before mutation."
                    .to_string(),
            ],
        },
        execution: LifecycleExecution::DetectOnly { guidance: label },
    };
    plan.digest = plan_digest(&plan)?;
    Ok(PreparedPlan {
        dependency_key: plan.resource_id.clone(),
        plan,
        evidence_fingerprint: fingerprint.clone(),
        recipe_fingerprint: fingerprint,
        executable_identities: Vec::new(),
        children: Vec::new(),
        depends_on: Vec::new(),
        exact_mapping: false,
        preconditions: Vec::new(),
        precondition_executable_paths: Vec::new(),
        precondition_expected_version: None,
        postcondition_executable_paths: Vec::new(),
        staged: true,
        skill_action: Some(PreparedSkillAction::Materialize(Box::new(prepared))),
        mcp_action: None,
    })
}

fn build_rollback_plan(
    request: LifecyclePlanRequest,
    sequence: u64,
    now: SystemTime,
    backup_id: String,
    receipts: Vec<(String, crate::skill_lifecycle::ManagedSkillReceipt)>,
) -> Result<PreparedPlan, CoreError> {
    let plan_id = opaque_plan_id(sequence, &request, now)?;
    let issued_at = format_timestamp(now)?;
    let expires_at = format_timestamp(now + PLAN_TTL)?;
    let mut affected_paths = receipts
        .iter()
        .map(|(_, receipt)| receipt.target.target_path.clone())
        .collect::<Vec<_>>();
    affected_paths.sort();
    affected_paths.dedup();
    let mut plan = LifecyclePlan {
        request: request.clone(),
        plan_id,
        canonical_id: request.resource_id.clone(),
        mapping_id: format!("skill-backup:{backup_id}"),
        resource_id: request.resource_id.clone(),
        owner: "STM managed skill lifecycle".to_string(),
        source: "Receipt-backed managed skill backup".to_string(),
        current_version: common_receipt_version(&receipts)
            .unwrap_or_else(|| "Partial state".to_string()),
        target_version: "Previous managed revision".to_string(),
        privilege: LifecyclePrivilege::UserConfirmation,
        affected_paths,
        affected_records: vec![format!("skill-backup:{backup_id}")],
        confidence: "Backup receipt and recovery journal verified".to_string(),
        limitations: vec![
            "Restores only targets recorded by the selected backup receipt.".to_string(),
        ],
        digest: String::new(),
        expires_at: expires_at.clone(),
        revalidation: LifecycleRevalidation {
            state: LifecycleRevalidationState::Fresh,
            checked_at: issued_at,
            checks: vec!["Selected backup remains available and receipt-backed.".to_string()],
        },
        execution: LifecycleExecution::DetectOnly {
            guidance: "Restore the previous receipt-backed skill revision".to_string(),
        },
    };
    plan.digest = plan_digest(&plan)?;
    let fingerprint = compute_sha256([serde_json::to_vec(&json!({
        "request": request,
        "backupId": backup_id,
        "receipts": receipts,
    }))?]);
    Ok(PreparedPlan {
        dependency_key: plan.resource_id.clone(),
        plan,
        evidence_fingerprint: fingerprint.clone(),
        recipe_fingerprint: fingerprint,
        executable_identities: Vec::new(),
        children: Vec::new(),
        depends_on: Vec::new(),
        exact_mapping: false,
        preconditions: Vec::new(),
        precondition_executable_paths: Vec::new(),
        precondition_expected_version: None,
        postcondition_executable_paths: Vec::new(),
        staged: false,
        skill_action: Some(PreparedSkillAction::RestoreBackup(backup_id)),
        mcp_action: None,
    })
}

fn build_keep_partial_plan(
    request: LifecyclePlanRequest,
    sequence: u64,
    now: SystemTime,
    receipts: Vec<(String, crate::skill_lifecycle::ManagedSkillReceipt)>,
) -> Result<PreparedPlan, CoreError> {
    let mut prepared = prepare_review_only(
        request,
        sequence,
        now,
        "Keep the completed targets and acknowledge the receipt-backed partial result.",
    )?;
    prepared.plan.privilege = LifecyclePrivilege::UserConfirmation;
    prepared.plan.execution = LifecycleExecution::DetectOnly {
        guidance: "Keep the completed targets and acknowledge the partial result".to_string(),
    };
    prepared.plan.digest = plan_digest(&prepared.plan)?;
    prepared.evidence_fingerprint = compute_sha256([serde_json::to_vec(&receipts)?]);
    prepared.skill_action = Some(PreparedSkillAction::KeepPartial);
    Ok(prepared)
}

fn normalized_action(action: &str) -> &str {
    if action.contains("install_side_by_side") {
        "install-side-by-side"
    } else if action.contains("restore_managed") {
        "restore-managed"
    } else if action.contains("keep_local") {
        "keep-local"
    } else if action.contains("export_diff") {
        "export-diff"
    } else if action.contains("retry_failed") {
        "retry"
    } else if action.contains("rollback_completed") {
        "rollback"
    } else if action.contains("keep_partial") {
        "keep-partial"
    } else if action.contains("resolve_partial") {
        "resolve-partial"
    } else if action.contains("install") {
        "install"
    } else if action.contains("update") {
        "update"
    } else {
        "review"
    }
}

fn authorize_source(
    binding: Option<&SourceAnalysisBinding>,
    entry: &TrustedSkillEntry,
) -> Result<(), CoreError> {
    let Some(binding) = binding else {
        return Ok(());
    };
    if binding.record.status != SourceAnalysisStatus::ReviewReady
        || binding.record.trust != SourceTrust::CatalogMatch
        || binding.resource_id.as_deref() != Some(entry.id.as_str())
    {
        return Err(CoreError::LifecycleEvidenceChanged(
            "skill source is not authorized by the authenticated catalog".to_string(),
        ));
    }
    Ok(())
}

fn source_spec(entry: &TrustedSkillEntry) -> SkillSourceSpec {
    SkillSourceSpec {
        repository: entry.source.repository.clone(),
        subpath: entry.source.subpath.clone(),
        commit: entry.source.commit.clone(),
        tree_sha256: entry.source.tree_sha256.clone(),
    }
}

fn receipt_source(
    receipts: &[(String, crate::skill_lifecycle::ManagedSkillReceipt)],
) -> Result<SkillSourceSpec, CoreError> {
    let first = receipts.first().ok_or_else(|| {
        CoreError::LifecycleEvidenceChanged("managed skill receipt is missing".to_string())
    })?;
    if receipts
        .iter()
        .any(|(_, receipt)| receipt.source != first.1.source)
    {
        return Err(CoreError::LifecycleEvidenceChanged(
            "managed skill receipts disagree on immutable source provenance".to_string(),
        ));
    }
    Ok(first.1.source.clone())
}

fn catalog_target_suffix(client: &SkillClientName, relative: &str) -> Result<String, CoreError> {
    let legacy_prefix = match client {
        SkillClientName::Codex => "$HOME/.codex/skills/",
        SkillClientName::ClaudeCode => "$HOME/.claude/skills/",
        SkillClientName::AgentKit => "$HOME/.agents/skills/",
    };
    let suffix = relative.strip_prefix(legacy_prefix).unwrap_or(relative);
    if suffix
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
        || suffix.starts_with('/')
        || suffix.contains('\\')
    {
        return Err(CoreError::InvalidPath(
            "trusted skill target is not normalized".to_string(),
        ));
    }
    Ok(suffix.to_string())
}

fn resolved_target_path(
    workspace: &FixtureWorkspace,
    target: &SkillTargetSpec,
) -> Result<PathBuf, CoreError> {
    let home = workspace.skill_home()?;
    let root = match target.client {
        SkillClientName::Codex => home.join(".codex/skills"),
        SkillClientName::ClaudeCode => home.join(".claude/skills"),
        SkillClientName::AgentKit => home.join(".agents/skills"),
    };
    Ok(root.join(Path::new(&target.target_path)))
}

fn client_label(client: &SkillClientName) -> &'static str {
    match client {
        SkillClientName::Codex => "Codex",
        SkillClientName::ClaudeCode => "Claude Code",
        SkillClientName::AgentKit => "AgentKit",
    }
}

fn common_receipt_version(
    receipts: &[(String, crate::skill_lifecycle::ManagedSkillReceipt)],
) -> Option<String> {
    let first = receipts.first()?.1.source.commit.clone();
    receipts
        .iter()
        .all(|(_, receipt)| receipt.source.commit == first)
        .then(|| format!("Git {}", &first[..first.len().min(12)]))
}

fn file_diff_lines(
    staging: &SkillStagingEvidence,
    receipts: &[(String, crate::skill_lifecycle::ManagedSkillReceipt)],
) -> Vec<String> {
    let previous = receipts
        .first()
        .map(|(_, receipt)| {
            receipt
                .file_manifest
                .iter()
                .map(|file| (file.path.as_str(), file.sha256.as_str()))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let next = staging
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.sha256.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut lines = Vec::new();
    for (path, digest) in &next {
        match previous.get(path) {
            None => lines.push(format!("ADD {path}")),
            Some(previous_digest) if previous_digest != digest => {
                lines.push(format!("MODIFY {path}"));
            }
            _ => {}
        }
    }
    for path in previous.keys() {
        if !next.contains_key(path) {
            lines.push(format!("REMOVE {path}"));
        }
    }
    if lines.is_empty() {
        lines.push("No file content changes relative to the current managed receipt.".to_string());
    }
    lines
}
