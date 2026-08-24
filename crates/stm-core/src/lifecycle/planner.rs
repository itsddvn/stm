use std::{path::PathBuf, time::SystemTime};

use serde_json::json;

use crate::{
    adapters::{compute_sha256, FixtureWorkspace},
    catalog::{load_tool_catalog, ToolCatalogEntry, ToolCatalogMapping},
    domain::{
        application_update::ApplicationUpdateKind,
        inventory::{ExecutionMode, MappingStatus, OwnershipKind},
        lifecycle::{
            LifecycleChildIntent, LifecycleExecution, LifecyclePlan, LifecyclePlanRequest,
            LifecyclePrivilege, LifecycleResourceKind, LifecycleRevalidation,
            LifecycleRevalidationState, MAX_BATCH_ITEMS,
        },
        migration::codex_npm_to_homebrew_recipe,
        recipe::{
            pinned_bun_archive, pinned_bun_source_url, VerifiedArchiveBinary,
            VerifiedInstallerArtifact, PINNED_BUN_VERSION,
        },
        source::{SourceAnalysisStatus, SourceTrust},
    },
    error::CoreError,
    inventory::{
        current_native_linux_manager, current_platform_slug, mapping_for_platform, scan_inventory,
    },
    ports::{HostExecutableResolver, McpLifecyclePort, SkillLifecyclePort},
    skill_lifecycle::PreparedSkillMutation,
    skills::scan_skills,
    versioning::{build_application_updates, load_version_catalog},
};

use super::{
    command::{lifecycle_privilege, ExecutableIdentity},
    evidence::{ManagerEvidencePort, ManagerStateEvidence},
    source_registry::SourceAnalysisBinding,
    time::{format_timestamp, PLAN_TTL},
};

#[derive(Debug, Clone)]
pub(crate) enum PreparedSkillAction {
    Materialize(Box<PreparedSkillMutation>),
    RestoreBackup(String),
    KeepPartial,
}
#[derive(Debug, Clone)]
pub(crate) struct PreparedPlan {
    pub dependency_key: String,
    pub plan: LifecyclePlan,
    pub evidence_fingerprint: String,
    pub recipe_fingerprint: String,
    pub executable_identities: Vec<ExecutableIdentity>,
    pub children: Vec<PreparedPlan>,
    pub depends_on: Vec<String>,
    pub exact_mapping: bool,
    pub preconditions: Vec<PreparedPlan>,
    pub precondition_executable_paths: Vec<String>,
    pub precondition_expected_version: Option<String>,
    pub postcondition_executable_paths: Vec<String>,
    pub staged: bool,
    pub skill_action: Option<PreparedSkillAction>,
    pub mcp_action: Option<crate::mcp::lifecycle::PreparedMcpAction>,
}

#[derive(Clone, Copy)]
pub(crate) struct PlannerContext<'a> {
    pub(crate) manager_evidence: &'a dyn ManagerEvidencePort,
    pub(crate) host: &'a dyn HostExecutableResolver,
    pub(crate) skill_lifecycle: &'a dyn SkillLifecyclePort,
    pub(crate) mcp_lifecycle: &'a dyn McpLifecyclePort,
}

struct ToolPlanContext<'a> {
    manager_evidence: &'a dyn ManagerEvidencePort,
    host: &'a dyn HostExecutableResolver,
    source: Option<&'a SourceAnalysisBinding>,
    sequence: u64,
    now: SystemTime,
    catalog_version: &'a str,
    expected_target: Option<&'a str>,
}

pub(crate) fn prepare_plan(
    workspace: &FixtureWorkspace,
    context: PlannerContext<'_>,
    request: LifecyclePlanRequest,
    source: Option<&SourceAnalysisBinding>,
    sequence: u64,
    now: SystemTime,
) -> Result<PreparedPlan, CoreError> {
    if request.resource_kind == LifecycleResourceKind::Tool && request.action == "review" {
        return prepare_review_only(
            request,
            sequence,
            now,
            "Imported or guidance-only tool intent requires explicit source review before any lifecycle action.",
        );
    }
    if request.resource_kind == LifecycleResourceKind::Operation
        && (request.action == "update-queue" || request.action == "setup-queue")
    {
        return prepare_batch(workspace, context, request, sequence, now);
    }
    if request.resource_kind == LifecycleResourceKind::Tool {
        return prepare_tool(
            workspace,
            context.manager_evidence,
            context.host,
            request,
            source,
            sequence,
            now,
        );
    }
    if request.resource_kind == LifecycleResourceKind::Skill {
        return super::skill_planner::prepare_skill(
            workspace,
            context.skill_lifecycle,
            request,
            source,
            sequence,
            now,
        );
    }
    if request.resource_kind == LifecycleResourceKind::Mcp {
        return super::mcp_planner::prepare_mcp(workspace, context, request, source, sequence, now);
    }
    prepare_review_only(
        request,
        sequence,
        now,
        "This lifecycle is not enabled in the current phase.",
    )
}

pub(crate) fn prepare_native_installer_plan(
    host: &dyn HostExecutableResolver,
    request: LifecyclePlanRequest,
    artifact: &VerifiedInstallerArtifact,
    sequence: u64,
    now: SystemTime,
) -> Result<PreparedPlan, CoreError> {
    if request.resource_kind != LifecycleResourceKind::Operation
        || request.action != "bootstrap"
        || request.resource_id != artifact.provider_id
        || artifact.provider_id != "homebrew"
        || !PathBuf::from(&artifact.path).is_absolute()
    {
        return Err(CoreError::MalformedInput(
            "native installer request does not match verified artifact".to_string(),
        ));
    }
    let opener = host.executable_identity(PathBuf::from("/usr/bin/open"))?;
    let installer_app = "/System/Library/CoreServices/Installer.app";
    let installer = host.executable_identity(PathBuf::from(
        "/System/Library/CoreServices/Installer.app/Contents/MacOS/Installer",
    ))?;
    let package = host.executable_identity(PathBuf::from(&artifact.path))?;
    if package.sha256 != artifact.sha256 {
        return Err(CoreError::LifecycleEvidenceChanged(
            "verified installer artifact digest changed".to_string(),
        ));
    }
    let checked_at = format_timestamp(now)?;
    let expires_at = format_timestamp(now + PLAN_TTL)?;
    let evidence_fingerprint = compute_sha256([
        artifact.provider_id.as_bytes().to_vec(),
        artifact.version.as_bytes().to_vec(),
        artifact.sha256.as_bytes().to_vec(),
        artifact.signer_team_id.as_bytes().to_vec(),
        artifact.package_id.as_bytes().to_vec(),
        package.sha256.as_bytes().to_vec(),
        installer.sha256.as_bytes().to_vec(),
    ]);
    let plan_id = opaque_plan_id(sequence, &request, now)?;
    let mut plan = LifecyclePlan {
        request,
        plan_id,
        canonical_id: format!("provider:{}", artifact.provider_id),
        mapping_id: format!(
            "pkg-installer:{}:{}",
            artifact.package_id, artifact.version
        ),
        resource_id: artifact.provider_id.clone(),
        owner: "Homebrew / macOS Installer".to_string(),
        source: artifact.source_url.clone(),
        current_version: "Not installed".to_string(),
        target_version: artifact.version.clone(),
        privilege: LifecyclePrivilege::UserConfirmation,
        affected_paths: artifact.expected_executable_paths.clone(),
        affected_records: vec![format!(
            "provider-bootstrap:{}:{}",
            artifact.provider_id, artifact.version
        )],
        confidence: format!(
            "Authoritative: SHA-256 pinned; Developer ID Installer team {} verified",
            artifact.signer_team_id
        ),
        limitations: vec![
            "macOS Installer owns the authorization prompt and package installation."
                .to_string(),
            "STM reports Failed unless a trusted Homebrew executable appears after the installer closes."
                .to_string(),
            "No shell installer or ambient PATH lookup is used.".to_string(),
        ],
        digest: String::new(),
        expires_at,
        revalidation: LifecycleRevalidation {
            state: LifecycleRevalidationState::Fresh,
            checked_at,
            checks: vec![
                "Pinned HTTPS release URL and SHA-256".to_string(),
                "Developer ID Installer signer and package identifier".to_string(),
                "Artifact, /usr/bin/open, and Apple Installer.app identities".to_string(),
            ],
        },
        execution: LifecycleExecution::NativeInstaller {
            executable: opener.canonical_path.display().to_string(),
            argv: vec![
                "-W".to_string(),
                "-a".to_string(),
                installer_app.to_string(),
                package.canonical_path.display().to_string(),
            ],
            artifact_sha256: artifact.sha256.clone(),
            signer_team_id: artifact.signer_team_id.clone(),
            package_id: artifact.package_id.clone(),
            expected_version: artifact.version.clone(),
            previous_receipt_install_time: artifact.previous_receipt_install_time,
        },
    };
    plan.digest = plan_digest(&plan)?;
    Ok(PreparedPlan {
        dependency_key: plan.resource_id.clone(),
        plan,
        recipe_fingerprint: evidence_fingerprint.clone(),
        evidence_fingerprint,
        executable_identities: vec![opener, installer, package],
        children: Vec::new(),
        depends_on: Vec::new(),
        staged: false,
        skill_action: None,
        mcp_action: None,
        exact_mapping: false,
        postcondition_executable_paths: Vec::new(),

        precondition_executable_paths: Vec::new(),
        precondition_expected_version: None,
        preconditions: Vec::new(),
    })
}
pub(crate) fn prepare_archive_installer_plan(
    host: &dyn HostExecutableResolver,
    request: LifecyclePlanRequest,
    artifact: &VerifiedArchiveBinary,
    sequence: u64,
    now: SystemTime,
) -> Result<PreparedPlan, CoreError> {
    let spec = pinned_bun_archive(current_platform_slug()).ok_or_else(|| {
        CoreError::MalformedInput("Bun bootstrap target is unsupported".to_string())
    })?;
    let expected_target = host.expected_stm_bun_binary_path();
    if request.resource_kind != LifecycleResourceKind::Operation
        || request.action != "bootstrap"
        || request.resource_id != artifact.provider_id
        || artifact.provider_id != "bun"
        || artifact.version != PINNED_BUN_VERSION
        || artifact.source_url != pinned_bun_source_url(spec)
        || artifact.archive_sha256 != spec.sha256
        || PathBuf::from(&artifact.target_binary_path) != expected_target
        || !PathBuf::from(&artifact.staged_binary_path).is_absolute()
    {
        return Err(CoreError::MalformedInput(
            "archive installer request does not match the pinned Bun artifact".to_string(),
        ));
    }
    let staged = host.executable_identity(PathBuf::from(&artifact.staged_binary_path))?;
    if staged.sha256 != artifact.binary_sha256 {
        return Err(CoreError::LifecycleEvidenceChanged(
            "verified Bun binary digest changed".to_string(),
        ));
    }
    let checked_at = format_timestamp(now)?;
    let expires_at = format_timestamp(now + PLAN_TTL)?;
    let evidence_fingerprint = compute_sha256([
        artifact.provider_id.as_bytes().to_vec(),
        artifact.version.as_bytes().to_vec(),
        artifact.archive_sha256.as_bytes().to_vec(),
        artifact.binary_sha256.as_bytes().to_vec(),
        staged.sha256.as_bytes().to_vec(),
        artifact.target_binary_path.as_bytes().to_vec(),
    ]);
    let plan_id = opaque_plan_id(sequence, &request, now)?;
    let mut plan = LifecyclePlan {
        request,
        plan_id,
        canonical_id: "provider:bun".to_string(),
        mapping_id: format!("archive-binary:bun:{}", artifact.version),
        resource_id: "bun".to_string(),
        owner: "STM user-scoped Bun provider".to_string(),
        source: artifact.source_url.clone(),
        current_version: "Not installed".to_string(),
        target_version: artifact.version.clone(),
        privilege: LifecyclePrivilege::UserConfirmation,
        affected_paths: vec![artifact.target_binary_path.clone()],
        affected_records: vec![format!("provider-bootstrap:bun:{}", artifact.version)],
        confidence: "Authoritative: pinned Bun archive and extracted binary hashes".to_string(),
        limitations: vec![
            "The Bun binary is installed only under STM user data; no shell installer or ambient PATH write is used.".to_string(),
            "Only the reviewed Bun bin directory is provided to Bun package commands.".to_string(),
        ],
        digest: String::new(),
        expires_at,
        revalidation: LifecycleRevalidation {
            state: LifecycleRevalidationState::Fresh,
            checked_at,
            checks: vec![
                "Pinned Bun release URL and archive SHA-256".to_string(),
                "Bounded symlink-free archive extraction".to_string(),
                "Extracted binary SHA-256 and exact target path".to_string(),
            ],
        },
        execution: LifecycleExecution::ArchiveInstaller {
            executable: "stm:archive-installer".to_string(),
            argv: vec![
                artifact.staged_binary_path.clone(),
                artifact.target_binary_path.clone(),
            ],
            archive_sha256: artifact.archive_sha256.clone(),
            binary_sha256: artifact.binary_sha256.clone(),
            target_path: artifact.target_binary_path.clone(),
            expected_version: artifact.version.clone(),
        },
    };
    plan.digest = plan_digest(&plan)?;
    Ok(PreparedPlan {
        dependency_key: "bun".to_string(),
        plan,
        recipe_fingerprint: evidence_fingerprint.clone(),
        evidence_fingerprint,
        executable_identities: vec![staged],
        children: Vec::new(),
        depends_on: Vec::new(),
        exact_mapping: false,
        preconditions: Vec::new(),
        precondition_executable_paths: Vec::new(),
        precondition_expected_version: None,
        postcondition_executable_paths: Vec::new(),
        staged: false,
        skill_action: None,
        mcp_action: None,
    })
}

pub(crate) fn prepare_setup_batch_with_bootstrap(
    workspace: &FixtureWorkspace,
    context: PlannerContext<'_>,
    request: LifecyclePlanRequest,
    artifact: &VerifiedInstallerArtifact,
    sequence: u64,
    now: SystemTime,
) -> Result<PreparedPlan, CoreError> {
    prepare_setup_batch_with_provider_bootstraps(
        workspace,
        context,
        request,
        ProviderBootstrapArtifacts {
            homebrew: Some(artifact),
            bun: None,
        },
        sequence,
        now,
    )
}

pub(crate) fn prepare_setup_batch_with_bun_bootstrap(
    workspace: &FixtureWorkspace,
    context: PlannerContext<'_>,
    request: LifecyclePlanRequest,
    artifact: &VerifiedArchiveBinary,
    sequence: u64,
    now: SystemTime,
) -> Result<PreparedPlan, CoreError> {
    prepare_setup_batch_with_provider_bootstraps(
        workspace,
        context,
        request,
        ProviderBootstrapArtifacts {
            homebrew: None,
            bun: Some(artifact),
        },
        sequence,
        now,
    )
}

pub(crate) struct ProviderBootstrapArtifacts<'a> {
    pub homebrew: Option<&'a VerifiedInstallerArtifact>,
    pub bun: Option<&'a VerifiedArchiveBinary>,
}

pub(crate) fn prepare_setup_batch_with_provider_bootstraps(
    workspace: &FixtureWorkspace,
    context: PlannerContext<'_>,
    request: LifecyclePlanRequest,
    artifacts: ProviderBootstrapArtifacts<'_>,
    sequence: u64,
    now: SystemTime,
) -> Result<PreparedPlan, CoreError> {
    let ProviderBootstrapArtifacts {
        homebrew: homebrew_artifact,
        bun: bun_artifact,
    } = artifacts;
    if homebrew_artifact.is_none() && bun_artifact.is_none() {
        return Err(CoreError::MalformedInput(
            "setup bootstrap requires at least one verified provider artifact".to_string(),
        ));
    }

    let mut next_sequence = sequence.saturating_add(1);
    let mut bootstraps = Vec::with_capacity(2);
    if let Some(artifact) = homebrew_artifact {
        let request = provider_bootstrap_request(&artifact.provider_id);
        bootstraps.push(prepare_native_installer_plan(
            context.host,
            request,
            artifact,
            next_sequence,
            now,
        )?);
        next_sequence = next_sequence.saturating_add(1);
    }
    if let Some(artifact) = bun_artifact {
        let request = provider_bootstrap_request(&artifact.provider_id);
        bootstraps.push(prepare_archive_installer_plan(
            context.host,
            request,
            artifact,
            next_sequence,
            now,
        )?);
        next_sequence = next_sequence.saturating_add(1);
    }

    let mut batch = prepare_batch(workspace, context, request, next_sequence, now)?;
    for child in &mut batch.children {
        let dependency =
            if homebrew_artifact.is_some() && child.plan.mapping_id.starts_with("homebrew:") {
                Some(("homebrew", "verified Homebrew bootstrap"))
            } else if bun_artifact.is_some() && child.plan.mapping_id.starts_with("bun:") {
                Some(("bun", "verified Bun bootstrap"))
            } else {
                None
            };
        if let Some((provider_id, description)) = dependency {
            if !child
                .depends_on
                .iter()
                .any(|dependency| dependency == provider_id)
            {
                child.depends_on.insert(0, provider_id.to_string());
            }
            child.staged = true;
            child.plan.confidence = format!("Staged: compile after {description}");
            child.plan.limitations.push(
                "The exact manager executable is compiled only after its provider postcondition."
                    .to_string(),
            );
            child.plan.digest = plan_digest(&child.plan)?;
        }
    }
    for child in &batch.children {
        if let Some(intent) = batch
            .plan
            .request
            .children
            .iter_mut()
            .find(|intent| intent.resource_id == child.plan.resource_id)
        {
            for dependency in &child.depends_on {
                if !intent.depends_on.contains(dependency) {
                    intent.depends_on.push(dependency.clone());
                }
            }
        }
    }

    let mut children = Vec::with_capacity(batch.children.len() + bootstraps.len());
    children.extend(bootstraps);
    children.extend(batch.children);
    batch.plan.execution = LifecycleExecution::Batch {
        items: children.iter().map(|child| child.plan.clone()).collect(),
    };
    batch.plan.affected_paths = children
        .iter()
        .flat_map(|child| child.plan.affected_paths.clone())
        .collect();
    batch.plan.affected_records = children
        .iter()
        .flat_map(|child| child.plan.affected_records.clone())
        .collect();
    batch.executable_identities = children
        .iter()
        .flat_map(|child| child.executable_identities.clone())
        .collect();
    batch.evidence_fingerprint = compute_sha256([serde_json::to_vec(
        &children
            .iter()
            .map(|child| &child.evidence_fingerprint)
            .collect::<Vec<_>>(),
    )?]);
    batch.recipe_fingerprint = compute_sha256([serde_json::to_vec(
        &children
            .iter()
            .map(|child| &child.recipe_fingerprint)
            .collect::<Vec<_>>(),
    )?]);
    batch.children = children;
    batch.plan.digest = plan_digest(&batch.plan)?;
    Ok(batch)
}

fn provider_bootstrap_request(provider_id: &str) -> LifecyclePlanRequest {
    LifecyclePlanRequest {
        resource_kind: LifecycleResourceKind::Operation,
        action: "bootstrap".to_string(),
        resource_id: provider_id.to_string(),
        source_analysis_handle: None,
        item_ids: None,
        children: Vec::new(),
        mapping_id: None,
    }
}

pub(crate) fn prepare_codex_migration_plan(
    workspace: &FixtureWorkspace,
    manager_evidence: &dyn ManagerEvidencePort,
    host: &dyn HostExecutableResolver,
    request: LifecyclePlanRequest,
    cleanup_old_owner: bool,
    sequence: u64,
    now: SystemTime,
) -> Result<PreparedPlan, CoreError> {
    let recipe = codex_npm_to_homebrew_recipe();
    if request.resource_kind != LifecycleResourceKind::Operation
        || request.resource_id != recipe.resource_id
        || !matches!(
            request.action.as_str(),
            "migrate-with-cleanup" | "migrate-keep-source"
        )
    {
        return Err(CoreError::MalformedInput(
            "migration request does not match an approved recipe".to_string(),
        ));
    }
    let catalog = load_tool_catalog(workspace)?;
    let versions = load_version_catalog(workspace)?;
    let entry = catalog
        .tools
        .iter()
        .find(|entry| entry.id == recipe.resource_id)
        .ok_or_else(|| CoreError::MalformedInput("Codex catalog entry missing".to_string()))?;
    let target_mapping = requested_mapping(entry, Some(&recipe.target_mapping_id))
        .ok_or_else(|| CoreError::MalformedInput("migration target mapping missing".to_string()))?;
    let source_mapping = requested_mapping(entry, Some(&recipe.source_mapping_id))
        .ok_or_else(|| CoreError::MalformedInput("migration source mapping missing".to_string()))?;
    let brew_executable = host
        .manager_evidence_executable(target_mapping, "install")?
        .ok_or_else(|| {
            CoreError::MalformedInput("approved Homebrew executable missing".to_string())
        })?;
    let brew_text = brew_executable.to_string_lossy();
    let target_executable_paths = if brew_text.starts_with("/opt/homebrew/") {
        vec!["/opt/homebrew/bin/codex".to_string()]
    } else if brew_text.starts_with("/usr/local/") {
        vec!["/usr/local/bin/codex".to_string()]
    } else {
        return Err(CoreError::LifecycleEvidenceChanged(
            "Homebrew migration prefix is not approved".to_string(),
        ));
    };
    let expected_target = versions
        .tool_updates
        .get(&entry.id)
        .map(|update| update.target_version.as_str());
    let source_preflight = build_tool_plan(
        LifecyclePlanRequest {
            resource_kind: LifecycleResourceKind::Tool,
            action: "uninstall".to_string(),
            resource_id: entry.id.clone(),
            source_analysis_handle: None,
            item_ids: None,
            children: Vec::new(),
            mapping_id: Some(recipe.source_mapping_id.clone()),
        },
        entry,
        source_mapping,
        ToolPlanContext {
            manager_evidence,
            host,
            source: None,
            sequence: sequence + 1,
            now,
            catalog_version: &catalog.version,
            expected_target,
        },
    )?;
    if !matches!(
        source_preflight.plan.execution,
        LifecycleExecution::ManagedExecute { .. }
    ) {
        return Err(CoreError::MalformedInput(
            "migration source ownership is not live and authoritative".to_string(),
        ));
    }
    let mut target = build_tool_plan(
        LifecyclePlanRequest {
            resource_kind: LifecycleResourceKind::Tool,
            action: "install".to_string(),
            resource_id: entry.id.clone(),
            source_analysis_handle: None,
            item_ids: None,
            children: Vec::new(),
            mapping_id: Some(recipe.target_mapping_id.clone()),
        },
        entry,
        target_mapping,
        ToolPlanContext {
            manager_evidence,
            host,
            source: None,
            sequence: sequence + 1,
            now,
            catalog_version: &catalog.version,
            expected_target,
        },
    )?;
    if !matches!(
        target.plan.execution,
        LifecycleExecution::ManagedExecute { .. }
    ) {
        return Err(CoreError::MalformedInput(
            "migration target cannot be installed with current Homebrew evidence".to_string(),
        ));
    }
    target.dependency_key = format!("migration-target:{}", entry.id);
    target.postcondition_executable_paths = target_executable_paths.clone();
    target.preconditions = vec![source_preflight.clone()];
    target.plan.affected_records.push(format!(
        "migration-source-evidence:{}",
        source_preflight.evidence_fingerprint
    ));
    target.plan.digest = plan_digest(&target.plan)?;
    let mut children = vec![target];
    if cleanup_old_owner {
        let mut cleanup = source_preflight;
        cleanup.dependency_key = format!("migration-source:{}", entry.id);
        cleanup.depends_on = vec![format!("migration-target:{}", entry.id)];
        children.push(cleanup);
    }
    for child in &mut children {
        child.exact_mapping = true;
    }
    let child_plans = children
        .iter()
        .map(|child| child.plan.clone())
        .collect::<Vec<_>>();
    let evidence_fingerprint = compute_sha256([serde_json::to_vec(
        &children
            .iter()
            .map(|child| &child.evidence_fingerprint)
            .collect::<Vec<_>>(),
    )?]);
    let recipe_fingerprint = compute_sha256([serde_json::to_vec(
        &children
            .iter()
            .map(|child| &child.recipe_fingerprint)
            .collect::<Vec<_>>(),
    )?]);
    let checked_at = format_timestamp(now)?;
    let expires_at = format_timestamp(now + PLAN_TTL)?;
    let plan_id = opaque_plan_id(sequence, &request, now)?;
    let mut plan = LifecyclePlan {
        request,
        plan_id,
        canonical_id: format!("migration:{}", recipe.id),
        mapping_id: recipe.id.clone(),
        resource_id: recipe.resource_id.clone(),
        owner: "Codex provider migration".to_string(),
        source: recipe.source_mapping_id.clone(),
        current_version: "npm-owned Codex".to_string(),
        target_version: "Homebrew-owned Codex".to_string(),
        privilege: LifecyclePrivilege::UserConfirmation,
        affected_paths: target_executable_paths,
        affected_records: vec![
            format!("migration-recipe:{}", recipe.id),
            "shared-config:codex-home:unchanged".to_string(),
        ],
        confidence: "Authoritative: explicit Codex npm-to-Homebrew migration recipe".to_string(),
        limitations: vec![
            "The shared Codex home remains unchanged; no config bytes are copied.".to_string(),
            "npm cleanup cannot start until the Homebrew target executable verifies.".to_string(),
        ],
        digest: String::new(),
        expires_at,
        revalidation: LifecycleRevalidation {
            state: LifecycleRevalidationState::Fresh,
            checked_at,
            checks: vec![
                "Exact source and target mappings".to_string(),
                "Target executable activation before cleanup".to_string(),
                "Shared config remains untouched".to_string(),
            ],
        },
        execution: LifecycleExecution::Batch { items: child_plans },
    };
    plan.digest = plan_digest(&plan)?;
    Ok(PreparedPlan {
        dependency_key: format!("migration:{}", recipe.id),
        plan,
        evidence_fingerprint,
        recipe_fingerprint,
        executable_identities: children
            .iter()
            .flat_map(|child| child.executable_identities.clone())
            .collect(),
        children,
        depends_on: Vec::new(),
        postcondition_executable_paths: Vec::new(),
        precondition_executable_paths: Vec::new(),
        precondition_expected_version: None,
        preconditions: Vec::new(),
        staged: false,
        skill_action: None,
        mcp_action: None,

        exact_mapping: true,
    })
}
pub(crate) fn prepare_exact_tool_plan(
    workspace: &FixtureWorkspace,
    manager_evidence: &dyn ManagerEvidencePort,
    host: &dyn HostExecutableResolver,
    request: LifecyclePlanRequest,
    sequence: u64,
    now: SystemTime,
) -> Result<PreparedPlan, CoreError> {
    if request.resource_kind != LifecycleResourceKind::Tool {
        return Err(CoreError::MalformedInput(
            "exact mapping revalidation requires a tool request".to_string(),
        ));
    }
    let catalog = load_tool_catalog(workspace)?;
    let versions = load_version_catalog(workspace)?;
    let entry = catalog
        .tools
        .iter()
        .find(|entry| entry.id == request.resource_id)
        .ok_or_else(|| CoreError::MalformedInput("exact tool identity missing".to_string()))?;
    let mapping_id = request
        .mapping_id
        .as_deref()
        .ok_or_else(|| CoreError::MalformedInput("exact mapping ID missing".to_string()))?;
    let mapping = requested_mapping(entry, Some(mapping_id))
        .filter(|mapping| mapping.platform == current_platform_slug())
        .ok_or_else(|| CoreError::MalformedInput("exact platform mapping missing".to_string()))?;

    let expected_target = versions
        .tool_updates
        .get(&entry.id)
        .map(|update| update.target_version.as_str());
    let mut prepared = build_tool_plan(
        request,
        entry,
        mapping,
        ToolPlanContext {
            manager_evidence,
            host,
            source: None,
            sequence,
            now,
            catalog_version: &catalog.version,
            expected_target,
        },
    )?;
    prepared.exact_mapping = true;
    Ok(prepared)
}
pub(crate) fn prepare_codex_migration_inspection_plan(
    workspace: &FixtureWorkspace,
    request: LifecyclePlanRequest,
    sequence: u64,
    now: SystemTime,
) -> Result<PreparedPlan, CoreError> {
    if request.resource_kind != LifecycleResourceKind::Operation
        || request.action != "inspect-migration"
        || request.resource_id != "codex-cli"
    {
        return Err(CoreError::MalformedInput(
            "migration inspection request is invalid".to_string(),
        ));
    }
    let catalog = load_tool_catalog(workspace)?;
    let versions = load_version_catalog(workspace)?;
    let inventory = scan_inventory(workspace, &catalog, &versions)?;
    let tool = inventory
        .tools
        .iter()
        .find(|tool| tool.id == "codex-cli")
        .ok_or_else(|| CoreError::MalformedInput("Codex inventory is unavailable".to_string()))?;
    let recipe = codex_npm_to_homebrew_recipe();
    let active_targets = recipe
        .target_executable_paths
        .iter()
        .filter(|path| PathBuf::from(path).is_file())
        .cloned()
        .collect::<Vec<_>>();
    let evidence = json!({
        "tool": tool,
        "activeTargets": active_targets,
        "recipe": recipe,
    });
    let evidence_fingerprint = compute_sha256([serde_json::to_vec(&evidence)?]);
    let checked_at = format_timestamp(now)?;
    let expires_at = format_timestamp(now + PLAN_TTL)?;
    let plan_id = opaque_plan_id(sequence, &request, now)?;
    let mut plan = LifecyclePlan {
        request,
        plan_id,
        canonical_id: "migration-inspection:codex-npm-to-homebrew".to_string(),
        mapping_id: "migration-inspection:codex-npm-to-homebrew".to_string(),
        resource_id: "codex-cli".to_string(),
        owner: tool.owner.clone(),
        source: tool.manager.clone(),
        current_version: tool
            .installed_version
            .clone()
            .unwrap_or_else(|| "Not installed".to_string()),
        target_version: tool
            .available_version
            .clone()
            .unwrap_or_else(|| "No target".to_string()),
        privilege: LifecyclePrivilege::None,
        affected_paths: active_targets,
        affected_records: vec![
            format!("inventory-owner:{}", tool.owner),
            format!("inventory-package:{}", tool.package_id),
        ],
        confidence: "Fresh migration inspection; no mutation authority".to_string(),
        limitations: vec![
            "This plan only inspects the verified Homebrew target and remaining npm cleanup state."
                .to_string(),
            "Any cleanup requires a new eligible migration plan and native confirmation."
                .to_string(),
        ],
        digest: String::new(),
        expires_at,
        revalidation: LifecycleRevalidation {
            state: LifecycleRevalidationState::Fresh,
            checked_at,
            checks: vec![
                "Fresh Codex inventory owner and package mapping".to_string(),
                "Reviewed Homebrew target executable paths".to_string(),
            ],
        },
        execution: LifecycleExecution::DetectOnly {
            guidance:
                "Review the current provider state; this inspection cannot remove npm or change PATH."
                    .to_string(),
        },
    };
    plan.digest = plan_digest(&plan)?;
    Ok(PreparedPlan {
        dependency_key: "migration-inspection:codex-cli".to_string(),
        plan,
        evidence_fingerprint: evidence_fingerprint.clone(),
        recipe_fingerprint: evidence_fingerprint,
        executable_identities: Vec::new(),
        children: Vec::new(),
        depends_on: Vec::new(),
        exact_mapping: false,
        preconditions: Vec::new(),
        postcondition_executable_paths: Vec::new(),
        precondition_executable_paths: Vec::new(),
        precondition_expected_version: None,
        staged: false,
        skill_action: None,
        mcp_action: None,
    })
}
pub(crate) fn prepare_codex_cleanup_retry_plan(
    workspace: &FixtureWorkspace,
    manager_evidence: &dyn ManagerEvidencePort,
    host: &dyn HostExecutableResolver,
    request: LifecyclePlanRequest,
    sequence: u64,
    now: SystemTime,
) -> Result<PreparedPlan, CoreError> {
    if request.resource_kind != LifecycleResourceKind::Operation
        || request.action != "migrate-cleanup-retry"
        || request.resource_id != "codex-cli"
    {
        return Err(CoreError::MalformedInput(
            "migration cleanup retry request is invalid".to_string(),
        ));
    }
    let recipe = codex_npm_to_homebrew_recipe();
    let catalog = load_tool_catalog(workspace)?;
    let versions = load_version_catalog(workspace)?;
    let inventory = scan_inventory(workspace, &catalog, &versions)?;
    let tool = inventory
        .tools
        .iter()
        .find(|tool| tool.id == recipe.resource_id)
        .ok_or_else(|| CoreError::MalformedInput("Codex inventory is unavailable".to_string()))?;
    if !tool.manager.eq_ignore_ascii_case("homebrew")
        || tool.package_id != "codex"
        || tool.installed_version.is_none()
    {
        return Err(CoreError::LifecycleConsentDenied(
            "cleanup retry requires verified Homebrew Codex ownership".to_string(),
        ));
    }
    let entry = catalog
        .tools
        .iter()
        .find(|entry| entry.id == recipe.resource_id)
        .ok_or_else(|| CoreError::MalformedInput("Codex catalog entry missing".to_string()))?;
    let source_mapping = requested_mapping(entry, Some(&recipe.source_mapping_id))
        .ok_or_else(|| CoreError::MalformedInput("migration source mapping missing".to_string()))?;
    let target_mapping = requested_mapping(entry, Some(&recipe.target_mapping_id))
        .ok_or_else(|| CoreError::MalformedInput("migration target mapping missing".to_string()))?;
    let brew_executable = host
        .manager_evidence_executable(target_mapping, "install")?
        .ok_or_else(|| {
            CoreError::LifecycleConsentDenied("approved Homebrew executable missing".to_string())
        })?;
    let brew_text = brew_executable.to_string_lossy();
    let target_executable_path = if brew_text.starts_with("/opt/homebrew/") {
        "/opt/homebrew/bin/codex".to_string()
    } else if brew_text.starts_with("/usr/local/") {
        "/usr/local/bin/codex".to_string()
    } else {
        return Err(CoreError::LifecycleEvidenceChanged(
            "Homebrew migration retry prefix is not approved".to_string(),
        ));
    };
    let expected_target = versions
        .tool_updates
        .get(&entry.id)
        .map(|update| update.target_version.as_str());
    let mut cleanup = build_tool_plan(
        LifecyclePlanRequest {
            resource_kind: LifecycleResourceKind::Tool,
            action: "uninstall".to_string(),
            resource_id: entry.id.clone(),
            source_analysis_handle: None,
            item_ids: None,
            children: Vec::new(),
            mapping_id: Some(recipe.source_mapping_id.clone()),
        },
        entry,
        source_mapping,
        ToolPlanContext {
            manager_evidence,
            host,
            source: None,
            sequence: sequence + 1,
            now,
            catalog_version: &catalog.version,
            expected_target,
        },
    )?;
    if !matches!(
        cleanup.plan.execution,
        LifecycleExecution::ManagedExecute { .. }
    ) {
        return Err(CoreError::LifecycleConsentDenied(
            "npm cleanup is no longer live and authoritative".to_string(),
        ));
    }
    cleanup.dependency_key = "migration-source:codex-cli".to_string();
    cleanup.exact_mapping = true;
    cleanup.precondition_executable_paths = vec![target_executable_path.clone()];
    cleanup.precondition_expected_version = tool.installed_version.clone();
    let child_plan = cleanup.plan.clone();
    let evidence_fingerprint = cleanup.evidence_fingerprint.clone();
    let recipe_fingerprint = cleanup.recipe_fingerprint.clone();
    let checked_at = format_timestamp(now)?;
    let expires_at = format_timestamp(now + PLAN_TTL)?;
    let plan_id = opaque_plan_id(sequence, &request, now)?;
    let mut plan = LifecyclePlan {
        request,
        plan_id,
        canonical_id: "migration-cleanup-retry:codex-npm-to-homebrew".to_string(),
        mapping_id: recipe.id.clone(),
        resource_id: recipe.resource_id.clone(),
        owner: "Codex provider migration cleanup".to_string(),
        source: recipe.source_mapping_id.clone(),
        current_version: "Verified Homebrew target".to_string(),
        target_version: "Remove remaining npm owner".to_string(),
        privilege: LifecyclePrivilege::UserConfirmation,
        affected_paths: vec![target_executable_path],
        affected_records: vec![
            "migration-target:homebrew:codex".to_string(),
            "migration-cleanup:npm:@openai/codex".to_string(),
        ],
        confidence: "Authoritative: Homebrew target retained; npm cleanup revalidated".to_string(),
        limitations: vec![
            "Cleanup rechecks the exact Homebrew target version before npm uninstall.".to_string(),
            "Shared Codex configuration remains untouched.".to_string(),
        ],
        digest: String::new(),
        expires_at,
        revalidation: LifecycleRevalidation {
            state: LifecycleRevalidationState::Fresh,
            checked_at,
            checks: vec![
                "Fresh Homebrew Codex owner".to_string(),
                "Exact target executable and version".to_string(),
                "Live npm source package before cleanup".to_string(),
            ],
        },
        execution: LifecycleExecution::Batch {
            items: vec![child_plan],
        },
    };
    plan.digest = plan_digest(&plan)?;
    Ok(PreparedPlan {
        dependency_key: "migration-cleanup-retry:codex-cli".to_string(),
        plan,
        evidence_fingerprint,
        recipe_fingerprint,
        executable_identities: cleanup.executable_identities.clone(),
        children: vec![cleanup],
        depends_on: Vec::new(),
        exact_mapping: true,
        preconditions: Vec::new(),
        precondition_executable_paths: Vec::new(),
        precondition_expected_version: None,
        postcondition_executable_paths: Vec::new(),
        staged: false,
        skill_action: None,
        mcp_action: None,
    })
}

fn prepare_tool(
    workspace: &FixtureWorkspace,
    manager_evidence: &dyn ManagerEvidencePort,
    host: &dyn HostExecutableResolver,
    request: LifecyclePlanRequest,
    source: Option<&SourceAnalysisBinding>,
    sequence: u64,
    now: SystemTime,
) -> Result<PreparedPlan, CoreError> {
    let catalog = load_tool_catalog(workspace)?;
    let resource_id = source
        .and_then(|binding| binding.resource_id.as_deref())
        .unwrap_or(&request.resource_id);
    let entry = catalog.tools.iter().find(|entry| {
        entry.id == resource_id
            || entry
                .mappings
                .iter()
                .any(|mapping| mapping.package_id == resource_id)
    });
    let Some(entry) = entry else {
        return prepare_review_only(
            request,
            sequence,
            now,
            "No authoritative tool mapping matched the reviewed source.",
        );
    };
    if !source_authorizes_entry(source, entry) {
        return prepare_review_only(request, sequence, now, "Source analysis did not match the locked catalog identity; managed execution remains unavailable.");
    }

    let platform = current_platform_slug();
    let platform_mapping = mapping_for_platform(entry, platform, current_native_linux_manager());
    let versions = load_version_catalog(workspace).ok();
    let requested = requested_mapping(entry, request.mapping_id.as_deref())
        .filter(|mapping| mapping.platform == platform);
    let owner_mapping = if requested.is_none() {
        live_owner_mapping(entry, platform, manager_evidence, host)?
    } else {
        None
    };
    let mapping = requested.or(owner_mapping).or(platform_mapping);
    let Some(mapping) = mapping else {
        return prepare_review_only(
            request,
            sequence,
            now,
            "This tool has no lifecycle mapping for the current platform.",
        );
    };
    let expected_target = versions
        .as_ref()
        .and_then(|versions| versions.tool_updates.get(&entry.id))
        .map(|update| update.target_version.as_str());
    build_tool_plan(
        request,
        entry,
        mapping,
        ToolPlanContext {
            manager_evidence,
            host,
            source,
            sequence,
            now,
            catalog_version: &catalog.version,
            expected_target,
        },
    )
}

fn build_tool_plan(
    request: LifecyclePlanRequest,
    entry: &ToolCatalogEntry,
    mapping: &ToolCatalogMapping,
    context: ToolPlanContext<'_>,
) -> Result<PreparedPlan, CoreError> {
    let ToolPlanContext {
        manager_evidence,
        host,
        source,
        sequence,
        now,
        catalog_version,
        expected_target,
    } = context;
    let mapping_id = format!("{}:{}", mapping.manager, mapping.package_id);
    let checked_at = format_timestamp(now)?;
    let expires_at = format_timestamp(
        source
            .map(|binding| std::cmp::min(now + PLAN_TTL, binding.expires_at))
            .unwrap_or(now + PLAN_TTL),
    )?;
    let owner = manager_label(&mapping.manager);
    let mut limitations = vec![
        "Recheck canonical identity, live manager ownership, versions, privilege, and executable identity before execution.".to_string(),
    ];
    let mut live_evidence = None;
    let (execution, privilege, identities) = match mapping.execution_mode {
        ExecutionMode::VendorHandoff => (
            LifecycleExecution::VendorHandoff {
                handoff_target: entry.homepage.clone(),
            },
            LifecyclePrivilege::VendorControlled,
            Vec::new(),
        ),
        ExecutionMode::ManagedExecute => {
            match host.manager_evidence_executable(mapping, &request.action)? {
                Some(evidence_executable)
                    if mapping.mapping_status == MappingStatus::Supported
                        && mapping.ownership_kind == OwnershipKind::ManagerOwned =>
                {
                    let evidence_path = evidence_executable.to_str().ok_or_else(|| {
                        CoreError::CommandDenied(
                            "reviewed manager executable path is not UTF-8".to_string(),
                        )
                    })?;
                    let evidence = manager_evidence.inspect(mapping, evidence_path)?;
                    let target = evidence.target_version.clone();
                    let action_allowed = managed_action_allowed(&request.action, &evidence);
                    live_evidence = Some(evidence);
                    if !action_allowed {
                        limitations.push(
                            "The requested state transition is not authorized by current manager evidence."
                                .to_string(),
                        );
                        (
                            LifecycleExecution::DetectOnly {
                                guidance:
                                    "Refresh inventory and select an action allowed by the current manager state."
                                        .to_string(),
                            },
                            LifecyclePrivilege::None,
                            Vec::new(),
                        )
                    } else {
                        match host.compile_manager_command(
                            mapping,
                            &request.action,
                            Some(&target),
                        )? {
                            Some(command) => {
                                let executable = command.executable.display().to_string();
                                let argv = command.argv;
                                let identities = command.identities;
                                (
                                    LifecycleExecution::ManagedExecute { executable, argv },
                                    lifecycle_privilege(mapping),
                                    identities,
                                )
                            }
                            None => {
                                limitations.push(
                                    "The reviewed manager is available, but the required privilege broker or executable boundary is unavailable; this mapping remains detect-only."
                                        .to_string(),
                                );
                                (
                                    LifecycleExecution::DetectOnly {
                                        guidance:
                                            "Use the authoritative manager directly after reviewing the displayed target and privilege requirement."
                                                .to_string(),
                                    },
                                    LifecyclePrivilege::None,
                                    Vec::new(),
                                )
                            }
                        }
                    }
                }
                Some(_) => {
                    limitations.push(
                        "Mapping is not approved for managed execution; authoritative evidence is shown without mutation."
                            .to_string(),
                    );
                    (
                        LifecycleExecution::DetectOnly {
                            guidance: "Use the recorded owner or package manager manually."
                                .to_string(),
                        },
                        LifecyclePrivilege::None,
                        Vec::new(),
                    )
                }
                None => {
                    limitations.push(
                        "Authoritative manager evidence is unavailable on this machine."
                            .to_string(),
                    );
                    (
                        LifecycleExecution::DetectOnly {
                            guidance: "Install or repair the authoritative manager, then refresh."
                                .to_string(),
                        },
                        LifecyclePrivilege::None,
                        Vec::new(),
                    )
                }
            }
        }
        _ => {
            limitations.push(
                "The requested state transition is not authorized by current inventory evidence."
                    .to_string(),
            );
            (
                LifecycleExecution::DetectOnly {
                    guidance: "Refresh inventory or use the recorded owner handoff.".to_string(),
                },
                LifecyclePrivilege::None,
                Vec::new(),
            )
        }
    };

    let affected_records = vec![
        format!("manager-package:{}:{}", mapping.manager, mapping.package_id),
        format!("inventory:tool:{}", entry.id),
        format!("receipt:tool:{}", entry.id),
    ];
    if mapping.adapter == "pacman_package"
        && matches!(request.action.as_str(), "install" | "update")
    {
        limitations.push(
            "Pacman install and update remain detect-only because -Syu refreshes repository metadata after consent and cannot preserve the reviewed target version."
                .to_string(),
        );
    }
    if mapping.adapter.starts_with("homebrew_")
        && matches!(&execution, LifecycleExecution::ManagedExecute { .. })
    {
        limitations.push(
            "Execution disables Homebrew auto-update, cleanup, and installed-dependent checks so the reviewed metadata and selected package boundary remain stable."
                .to_string(),
        );
    }

    let current_version = live_evidence
        .as_ref()
        .and_then(|evidence| evidence.current_version.clone())
        .unwrap_or_else(|| "Not installed".to_string());
    let target_version = live_evidence
        .as_ref()
        .map(|evidence| evidence.target_version.clone())
        .or_else(|| expected_target.map(str::to_string))
        .unwrap_or_else(|| "Unavailable without live manager evidence".to_string());
    let recipe = json!({
        "catalogVersion": catalog_version,
        "entryId": &entry.id,
        "mapping": mapping,
        "action": &request.action,
        "expectedTarget": &target_version,
        "source": source.and_then(|binding| binding.record.normalized_url.as_ref()),
    });
    let recipe_fingerprint = compute_sha256([serde_json::to_vec(&recipe)?]);
    let evidence = json!({
        "catalogVersion": catalog_version,
        "entry": entry,
        "mapping": mapping,
        "liveManagerEvidence": live_evidence,
        "execution": execution,
        "identities": identities,
        "sourceAnalysis": source.map(|binding| &binding.record),
    });
    let evidence_fingerprint = compute_sha256([serde_json::to_vec(&evidence)?]);
    let plan_id = opaque_plan_id(sequence, &request, now)?;
    let mut plan = LifecyclePlan {
        request,
        plan_id,
        canonical_id: format!("tool:{}", entry.id),
        mapping_id,
        resource_id: entry.id.clone(),
        owner,
        source: source
            .and_then(|binding| binding.record.normalized_url.clone())
            .unwrap_or_else(|| mapping.manager.clone()),
        current_version,
        target_version,
        privilege,
        affected_paths: Vec::new(),
        affected_records,
        confidence: live_evidence
            .as_ref()
            .map(|evidence| format!("Authoritative: {}", evidence.source))
            .unwrap_or_else(|| "Blocked: live manager evidence unavailable".to_string()),
        limitations,
        digest: String::new(),
        expires_at,
        revalidation: LifecycleRevalidation {
            state: LifecycleRevalidationState::Fresh,
            checked_at,
            checks: vec![
                "Canonical catalog identity".to_string(),
                "Exact platform mapping and ownership".to_string(),
                "Current and target versions".to_string(),
                "Compiled executable and argument vector".to_string(),
                "Executable path, owner, size, and modification time".to_string(),
            ],
        },
        execution,
    };
    plan.digest = plan_digest(&plan)?;
    Ok(PreparedPlan {
        dependency_key: plan.resource_id.clone(),
        plan,
        evidence_fingerprint,
        recipe_fingerprint,
        executable_identities: identities,
        children: Vec::new(),
        depends_on: Vec::new(),
        staged: false,
        skill_action: None,
        mcp_action: None,
        exact_mapping: false,
        postcondition_executable_paths: Vec::new(),
        precondition_executable_paths: Vec::new(),
        precondition_expected_version: None,
        preconditions: Vec::new(),
    })
}

fn prepare_batch(
    workspace: &FixtureWorkspace,
    context: PlannerContext<'_>,
    request: LifecyclePlanRequest,
    sequence: u64,
    now: SystemTime,
) -> Result<PreparedPlan, CoreError> {
    let catalog = load_tool_catalog(workspace)?;
    let versions = load_version_catalog(workspace)?;
    let inventory = scan_inventory(workspace, &catalog, &versions)?;
    let skills = scan_skills(workspace, &versions)?;
    let updates = build_application_updates(&inventory.tools, &skills.skills, &versions);
    let typed_children = request.children.clone();
    let item_ids = request.item_ids.clone().unwrap_or_default();
    let requested_count = if typed_children.is_empty() {
        item_ids.len()
    } else {
        typed_children.len()
    };
    if requested_count > MAX_BATCH_ITEMS {
        return Err(CoreError::MalformedInput(format!(
            "batch lifecycle request exceeds {MAX_BATCH_ITEMS} items"
        )));
    }
    let duplicate = if typed_children.is_empty() {
        item_ids
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != item_ids.len()
    } else {
        typed_children
            .iter()
            .map(|child| {
                (
                    resource_kind_label(&child.resource_kind),
                    &child.resource_id,
                )
            })
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != typed_children.len()
    };
    if duplicate {
        return Err(CoreError::MalformedInput(
            "batch lifecycle request contains duplicate items".to_string(),
        ));
    }
    let typed_children = order_typed_children(typed_children)?;
    if typed_children.is_empty() && item_ids.is_empty() {
        return Err(CoreError::MalformedInput(
            "batch lifecycle request requires children or itemIds".to_string(),
        ));
    }
    if !typed_children.is_empty() && !item_ids.is_empty() {
        let child_ids: std::collections::BTreeSet<String> = typed_children
            .iter()
            .flat_map(|child| {
                [
                    child.resource_id.clone(),
                    format!("update-{}", child.resource_id),
                ]
            })
            .collect();
        if item_ids.iter().any(|id| !child_ids.contains(id)) {
            return Err(CoreError::MalformedInput(
                "batch children and itemIds disagree".to_string(),
            ));
        }
    }
    if request.action == "setup-queue" {
        for raw_id in item_ids
            .iter()
            .chain(typed_children.iter().map(|child| &child.resource_id))
        {
            let tool_id = raw_id.trim_start_matches("update-");
            let known = inventory.tools.iter().any(|tool| tool.id == tool_id)
                || catalog.tools.iter().any(|entry| entry.id == tool_id)
                || updates.iter().any(|update| update.id == *raw_id);
            let explicit_review = typed_children.iter().any(|child| {
                child.resource_id == tool_id
                    && child.desired_action == "review"
                    && child.mapping_id.is_none()
            });
            if !known && !explicit_review {
                return Err(CoreError::MalformedInput(format!(
                    "unknown setup-queue item: {raw_id}"
                )));
            }
        }
    }

    let mut children = Vec::new();
    let mut prepared_children = Vec::new();
    let mut fingerprints = Vec::new();
    let mut identities = Vec::new();
    let child_requests: Vec<(LifecyclePlanRequest, Vec<String>)> = if !typed_children.is_empty() {
        typed_children
            .into_iter()
            .map(|child| {
                (
                    LifecyclePlanRequest {
                        resource_kind: child.resource_kind,
                        action: child.desired_action,
                        resource_id: child.resource_id,
                        source_analysis_handle: None,
                        item_ids: None,
                        children: Vec::new(),
                        mapping_id: child.mapping_id,
                    },
                    child.depends_on,
                )
            })
            .collect()
    } else {
        item_ids
            .iter()
            .map(
                |item_id| match updates.iter().find(|update| update.id == *item_id) {
                    Some(update) if update.resource_type == ApplicationUpdateKind::Tool => {
                        LifecyclePlanRequest {
                            resource_kind: LifecycleResourceKind::Tool,
                            action: "update".to_string(),
                            resource_id: item_id.trim_start_matches("update-").to_string(),
                            source_analysis_handle: None,
                            item_ids: None,
                            children: Vec::new(),
                            mapping_id: None,
                        }
                    }
                    Some(update) => LifecyclePlanRequest {
                        resource_kind: match update.resource_type {
                            ApplicationUpdateKind::Skill => LifecycleResourceKind::Skill,
                            ApplicationUpdateKind::Product => LifecycleResourceKind::Product,
                            ApplicationUpdateKind::Tool => LifecycleResourceKind::Tool,
                        },
                        action: if update.resource_type == ApplicationUpdateKind::Product {
                            "product-update"
                        } else {
                            "update"
                        }
                        .to_string(),
                        resource_id: item_id.trim_start_matches("update-").to_string(),
                        source_analysis_handle: None,
                        item_ids: None,
                        children: Vec::new(),
                        mapping_id: None,
                    },
                    None if request.action == "setup-queue" => {
                        let tool_id = item_id.trim_start_matches("update-");
                        let action = inventory
                            .tools
                            .iter()
                            .find(|tool| tool.id == tool_id)
                            .map(|tool| {
                                match tool.state {
                            crate::domain::inventory::InventoryState::ManagedUpdateAvailable => {
                                "update"
                            }
                            crate::domain::inventory::InventoryState::Missing => "install",
                            _ => "review",
                        }
                            })
                            .unwrap_or("install");
                        LifecyclePlanRequest {
                            resource_kind: LifecycleResourceKind::Tool,
                            action: action.to_string(),
                            resource_id: tool_id.to_string(),
                            source_analysis_handle: None,
                            item_ids: None,
                            children: Vec::new(),
                            mapping_id: None,
                        }
                    }
                    None => LifecyclePlanRequest {
                        resource_kind: LifecycleResourceKind::Operation,
                        action: "review".to_string(),
                        resource_id: item_id.clone(),
                        source_analysis_handle: None,
                        item_ids: None,
                        children: Vec::new(),
                        mapping_id: None,
                    },
                },
            )
            .map(|request| (request, Vec::new()))
            .collect()
    };
    for (index, (child_request, depends_on)) in child_requests.into_iter().enumerate() {
        let child_sequence = sequence.saturating_mul(1000) + index as u64 + 1;
        let fallback_request = child_request.clone();
        let mut prepared = match prepare_plan(
            workspace,
            context,
            child_request,
            None,
            child_sequence,
            now,
        ) {
            Ok(prepared) => prepared,
            Err(CoreError::LifecycleEvidenceChanged(detail))
                if fallback_request.resource_kind == LifecycleResourceKind::Skill
                    && detail == "no authenticated trusted skill catalog is available" =>
            {
                prepare_review_only(
                    fallback_request,
                    child_sequence,
                    now,
                    "Authenticated skill catalog is unavailable; this batch child remains review-only.",
                )?
            }
            Err(error) => return Err(error),
        };
        prepared.depends_on = depends_on;
        fingerprints.push(prepared.evidence_fingerprint.clone());
        identities.extend(prepared.executable_identities.clone());
        children.push(prepared.plan.clone());
        prepared_children.push(prepared);
    }

    let checked_at = format_timestamp(now)?;
    let expires_at = format_timestamp(now + PLAN_TTL)?;
    let evidence_fingerprint = compute_sha256([serde_json::to_vec(&fingerprints)?]);
    let recipe_fingerprint = compute_sha256([serde_json::to_vec(
        &prepared_children
            .iter()
            .map(|child| &child.recipe_fingerprint)
            .collect::<Vec<_>>(),
    )?]);
    let plan_id = opaque_plan_id(sequence, &request, now)?;
    let mut plan = LifecyclePlan {
        request,
        plan_id,
        canonical_id: "batch:selected-update-queue".to_string(),
        mapping_id: "batch:independent-child-plans".to_string(),
        resource_id: "selected-update-queue".to_string(),
        owner: "Independent authoritative owners".to_string(),
        source: "Per-item catalog and inventory evidence".to_string(),
        current_version: "See each child plan".to_string(),
        target_version: "See each child plan".to_string(),
        privilege: LifecyclePrivilege::UserConfirmation,
        affected_paths: children
            .iter()
            .flat_map(|child| child.affected_paths.clone())
            .collect(),
        affected_records: children
            .iter()
            .flat_map(|child| child.affected_records.clone())
            .collect(),
        confidence: "Each child plan resolved independently".to_string(),
        limitations: vec![
            "A failed child does not authorize or roll back any sibling plan.".to_string(),
        ],
        digest: String::new(),
        expires_at,
        revalidation: LifecycleRevalidation {
            state: LifecycleRevalidationState::Fresh,
            checked_at,
            checks: vec!["Revalidate every child digest and evidence boundary".to_string()],
        },
        execution: LifecycleExecution::Batch { items: children },
    };
    plan.digest = plan_digest(&plan)?;
    Ok(PreparedPlan {
        dependency_key: plan.resource_id.clone(),
        plan,
        recipe_fingerprint,
        evidence_fingerprint,
        executable_identities: identities,
        children: prepared_children,
        depends_on: Vec::new(),
        staged: false,
        skill_action: None,
        mcp_action: None,
        exact_mapping: false,
        postcondition_executable_paths: Vec::new(),
        precondition_executable_paths: Vec::new(),
        precondition_expected_version: None,
        preconditions: Vec::new(),
    })
}

pub(super) fn prepare_review_only(
    request: LifecyclePlanRequest,
    sequence: u64,
    now: SystemTime,
    guidance: &str,
) -> Result<PreparedPlan, CoreError> {
    let checked_at = format_timestamp(now)?;
    let expires_at = format_timestamp(now + PLAN_TTL)?;
    let evidence_fingerprint = compute_sha256([serde_json::to_vec(&request)?]);
    let recipe_fingerprint = evidence_fingerprint.clone();
    let plan_id = opaque_plan_id(sequence, &request, now)?;
    let mut plan = LifecyclePlan {
        canonical_id: format!(
            "{}:{}",
            resource_kind_label(&request.resource_kind),
            request.resource_id
        ),
        mapping_id: format!("review-only:{}", request.resource_id),
        resource_id: request.resource_id.clone(),
        owner: "No authorized lifecycle owner".to_string(),
        source: "Read-only lifecycle review".to_string(),
        current_version: "Recorded state".to_string(),
        target_version: "No authorized target".to_string(),
        privilege: LifecyclePrivilege::None,
        affected_paths: Vec::new(),
        affected_records: Vec::new(),
        confidence: "Review only".to_string(),
        limitations: vec![guidance.to_string()],
        digest: String::new(),
        expires_at,
        revalidation: LifecycleRevalidation {
            state: LifecycleRevalidationState::Fresh,
            checked_at,
            checks: vec!["Confirm no mutation authority is available".to_string()],
        },
        execution: LifecycleExecution::DetectOnly {
            guidance: guidance.to_string(),
        },
        plan_id,
        request,
    };
    plan.digest = plan_digest(&plan)?;
    Ok(PreparedPlan {
        dependency_key: plan.resource_id.clone(),
        plan,
        evidence_fingerprint,
        recipe_fingerprint,
        executable_identities: Vec::new(),
        children: Vec::new(),
        depends_on: Vec::new(),
        staged: false,
        skill_action: None,
        mcp_action: None,
        exact_mapping: false,
        postcondition_executable_paths: Vec::new(),
        precondition_executable_paths: Vec::new(),
        precondition_expected_version: None,
        preconditions: Vec::new(),
    })
}

fn managed_action_allowed(action: &str, evidence: &ManagerStateEvidence) -> bool {
    match action {
        "install" => !evidence.installed,
        "update" => evidence.installed && evidence.update_available,
        "uninstall" => evidence.installed,
        _ => false,
    }
}

fn source_authorizes_entry(
    source: Option<&SourceAnalysisBinding>,
    entry: &ToolCatalogEntry,
) -> bool {
    let Some(source) = source else {
        return true;
    };
    source.record.status == SourceAnalysisStatus::ReviewReady
        && source.record.trust == SourceTrust::CatalogMatch
        && source.resource_id.as_deref() == Some(entry.id.as_str())
}

pub(super) fn opaque_plan_id(
    sequence: u64,
    request: &LifecyclePlanRequest,
    now: SystemTime,
) -> Result<String, CoreError> {
    let value =
        json!({ "sequence": sequence, "request": request, "issuedAt": format_timestamp(now)? });
    let digest = compute_sha256([serde_json::to_vec(&value)?]);
    Ok(format!("lifecycle-plan-{}", &digest[7..23]))
}

pub(super) fn plan_digest(plan: &LifecyclePlan) -> Result<String, CoreError> {
    let mut unsigned = plan.clone();
    unsigned.digest.clear();
    Ok(compute_sha256([serde_json::to_vec(&unsigned)?]))
}

fn manager_label(manager: &str) -> String {
    match manager {
        "homebrew" => "Homebrew",
        "winget" => "WinGet",
        "apt" => "APT",
        "vendor" => "Vendor updater",
        value => value,
    }
    .to_string()
}

fn live_owner_mapping<'a>(
    entry: &'a ToolCatalogEntry,
    platform: &str,
    manager_evidence: &dyn ManagerEvidencePort,
    host: &dyn HostExecutableResolver,
) -> Result<Option<&'a ToolCatalogMapping>, CoreError> {
    let mut first_error = None;
    for mapping in entry.mappings.iter().filter(|mapping| {
        mapping.platform == platform
            && mapping.mapping_status == MappingStatus::Supported
            && mapping.execution_mode == ExecutionMode::ManagedExecute
            && mapping.ownership_kind == OwnershipKind::ManagerOwned
    }) {
        let executable = match host.manager_evidence_executable(mapping, "install") {
            Ok(Some(executable)) => executable,
            Ok(None) => continue,
            Err(error) => {
                first_error.get_or_insert(error);
                continue;
            }
        };
        let Some(executable) = executable.to_str() else {
            first_error.get_or_insert_with(|| {
                CoreError::CommandDenied(
                    "reviewed manager executable path is not UTF-8".to_string(),
                )
            });
            continue;
        };
        match manager_evidence.inspect(mapping, executable) {
            Ok(evidence) if evidence.installed => return Ok(Some(mapping)),
            Ok(_) => {}
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    if let Some(error) = first_error {
        return Err(error);
    }
    Ok(None)
}

fn requested_mapping<'a>(
    entry: &'a crate::catalog::ToolCatalogEntry,
    mapping_id: Option<&str>,
) -> Option<&'a crate::catalog::ToolCatalogMapping> {
    let mapping_id = mapping_id?;
    entry
        .mappings
        .iter()
        .find(|mapping| format!("{}:{}", mapping.manager, mapping.package_id) == mapping_id)
}

fn order_typed_children(
    children: Vec<LifecycleChildIntent>,
) -> Result<Vec<LifecycleChildIntent>, CoreError> {
    let ids = children
        .iter()
        .map(|child| child.resource_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let mut id_counts = std::collections::BTreeMap::<&str, usize>::new();
    for child in &children {
        *id_counts.entry(child.resource_id.as_str()).or_default() += 1;
    }
    for child in &children {
        let dependencies = child
            .depends_on
            .iter()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        if dependencies.len() != child.depends_on.len() {
            return Err(CoreError::MalformedInput(format!(
                "batch child {} contains duplicate dependencies",
                child.resource_id
            )));
        }
        if let Some(ambiguous) = dependencies
            .iter()
            .find(|dependency| id_counts.get(**dependency).copied().unwrap_or(0) > 1)
        {
            return Err(CoreError::MalformedInput(format!(
                "batch dependency {ambiguous} is ambiguous across resource kinds"
            )));
        }
        if dependencies.contains(child.resource_id.as_str()) {
            return Err(CoreError::MalformedInput(format!(
                "batch child {} cannot depend on itself",
                child.resource_id
            )));
        }
        if let Some(missing) = dependencies
            .iter()
            .find(|dependency| !ids.contains(**dependency))
        {
            return Err(CoreError::MalformedInput(format!(
                "batch child {} references unknown dependency {}",
                child.resource_id, missing
            )));
        }
    }

    let mut remaining = children.into_iter().map(Some).collect::<Vec<_>>();
    let mut ordered = Vec::with_capacity(remaining.len());
    let mut completed = std::collections::BTreeSet::<String>::new();
    while ordered.len() < remaining.len() {
        let Some(index) = remaining.iter().position(|candidate| {
            candidate.as_ref().is_some_and(|child| {
                child
                    .depends_on
                    .iter()
                    .all(|dependency| completed.contains(dependency))
            })
        }) else {
            return Err(CoreError::MalformedInput(
                "batch dependency graph contains a cycle".to_string(),
            ));
        };
        let child = remaining[index].take().expect("selected dependency child");
        completed.insert(child.resource_id.clone());
        ordered.push(child);
    }
    Ok(ordered)
}

fn resource_kind_label(kind: &LifecycleResourceKind) -> &'static str {
    match kind {
        LifecycleResourceKind::Tool => "tool",
        LifecycleResourceKind::Skill => "skill",
        LifecycleResourceKind::Mcp => "mcp",
        LifecycleResourceKind::Product => "product",
        LifecycleResourceKind::Operation => "operation",
    }
}
