use std::time::SystemTime;

use serde_json::json;

use crate::{
    adapters::{compute_sha256, FixtureWorkspace},
    catalog::{load_tool_catalog, ToolCatalogEntry, ToolCatalogMapping},
    domain::{
        application_update::ApplicationUpdateKind,
        inventory::{ExecutionMode, MappingStatus, OwnershipKind},
        lifecycle::{
            LifecycleExecution, LifecyclePlan, LifecyclePlanRequest, LifecyclePrivilege,
            LifecycleResourceKind, LifecycleRevalidation, LifecycleRevalidationState,
        },
        source::{SourceAnalysisStatus, SourceTrust},
    },
    error::CoreError,
    inventory::{
        current_native_linux_manager, current_platform_slug, mapping_for_platform, scan_inventory,
    },
    skills::scan_skills,
    versioning::{build_application_updates, load_version_catalog},
};

use super::{
    command::{
        compile_manager_command, lifecycle_privilege, manager_evidence_executable,
        ExecutableIdentity,
    },
    evidence::{ManagerEvidencePort, ManagerStateEvidence},
    source_registry::SourceAnalysisBinding,
    time::{format_timestamp, PLAN_TTL},
};

#[derive(Debug, Clone)]
pub(crate) struct PreparedPlan {
    pub plan: LifecyclePlan,
    pub evidence_fingerprint: String,
    pub executable_identities: Vec<ExecutableIdentity>,
    pub children: Vec<PreparedPlan>,
}

struct ToolPlanContext<'a> {
    manager_evidence: &'a dyn ManagerEvidencePort,
    source: Option<&'a SourceAnalysisBinding>,
    sequence: u64,
    now: SystemTime,
    catalog_version: &'a str,
}

pub(crate) fn prepare_plan(
    workspace: &FixtureWorkspace,
    manager_evidence: &dyn ManagerEvidencePort,
    request: LifecyclePlanRequest,
    source: Option<&SourceAnalysisBinding>,
    sequence: u64,
    now: SystemTime,
) -> Result<PreparedPlan, CoreError> {
    if request.resource_kind == LifecycleResourceKind::Operation && request.action == "update-queue"
    {
        return prepare_batch(workspace, manager_evidence, request, sequence, now);
    }
    if request.resource_kind == LifecycleResourceKind::Tool {
        return prepare_tool(workspace, manager_evidence, request, source, sequence, now);
    }
    prepare_review_only(
        request,
        sequence,
        now,
        "This lifecycle is not enabled in the current phase.",
    )
}

fn prepare_tool(
    workspace: &FixtureWorkspace,
    manager_evidence: &dyn ManagerEvidencePort,
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
    let Some(mapping) = mapping_for_platform(entry, platform, current_native_linux_manager())
    else {
        return prepare_review_only(
            request,
            sequence,
            now,
            "This tool has no lifecycle mapping for the current platform.",
        );
    };
    build_tool_plan(
        request,
        entry,
        mapping,
        ToolPlanContext {
            manager_evidence,
            source,
            sequence,
            now,
            catalog_version: &catalog.version,
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
        source,
        sequence,
        now,
        catalog_version,
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
            match manager_evidence_executable(mapping, &request.action)? {
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
                        match compile_manager_command(mapping, &request.action, Some(&target))? {
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
        .unwrap_or_else(|| "Unavailable without live manager evidence".to_string());
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
        plan,
        evidence_fingerprint,
        executable_identities: identities,
        children: Vec::new(),
    })
}

fn prepare_batch(
    workspace: &FixtureWorkspace,
    manager_evidence: &dyn ManagerEvidencePort,
    request: LifecyclePlanRequest,
    sequence: u64,
    now: SystemTime,
) -> Result<PreparedPlan, CoreError> {
    let catalog = load_tool_catalog(workspace)?;
    let versions = load_version_catalog(workspace)?;
    let inventory = scan_inventory(workspace, &catalog, &versions)?;
    let skills = scan_skills(workspace, &versions)?;
    let updates = build_application_updates(&inventory.tools, &skills.skills, &versions);
    let item_ids = request.item_ids.clone().unwrap_or_default();
    if item_ids.is_empty() {
        return Err(CoreError::MalformedInput(
            "batch lifecycle request requires itemIds".to_string(),
        ));
    }

    let mut children = Vec::new();
    let mut prepared_children = Vec::new();
    let mut fingerprints = Vec::new();
    let mut identities = Vec::new();
    for (index, item_id) in item_ids.iter().enumerate() {
        let child_request = match updates.iter().find(|update| update.id == *item_id) {
            Some(update) if update.resource_type == ApplicationUpdateKind::Tool => {
                LifecyclePlanRequest {
                    resource_kind: LifecycleResourceKind::Tool,
                    action: "update".to_string(),
                    resource_id: item_id.trim_start_matches("update-").to_string(),
                    source_analysis_handle: None,
                    item_ids: None,
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
            },
            None => LifecyclePlanRequest {
                resource_kind: LifecycleResourceKind::Operation,
                action: "review".to_string(),
                resource_id: item_id.clone(),
                source_analysis_handle: None,
                item_ids: None,
            },
        };
        let prepared = prepare_plan(
            workspace,
            manager_evidence,
            child_request,
            None,
            sequence.saturating_mul(1000) + index as u64 + 1,
            now,
        )?;
        fingerprints.push(prepared.evidence_fingerprint.clone());
        identities.extend(prepared.executable_identities.clone());
        children.push(prepared.plan.clone());
        prepared_children.push(prepared);
    }

    let checked_at = format_timestamp(now)?;
    let expires_at = format_timestamp(now + PLAN_TTL)?;
    let evidence_fingerprint = compute_sha256([serde_json::to_vec(&fingerprints)?]);
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
        plan,
        evidence_fingerprint,
        executable_identities: identities,
        children: prepared_children,
    })
}

fn prepare_review_only(
    request: LifecyclePlanRequest,
    sequence: u64,
    now: SystemTime,
    guidance: &str,
) -> Result<PreparedPlan, CoreError> {
    let checked_at = format_timestamp(now)?;
    let expires_at = format_timestamp(now + PLAN_TTL)?;
    let evidence_fingerprint = compute_sha256([serde_json::to_vec(&request)?]);
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
        plan,
        evidence_fingerprint,
        executable_identities: Vec::new(),
        children: Vec::new(),
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

fn opaque_plan_id(
    sequence: u64,
    request: &LifecyclePlanRequest,
    now: SystemTime,
) -> Result<String, CoreError> {
    let value =
        json!({ "sequence": sequence, "request": request, "issuedAt": format_timestamp(now)? });
    let digest = compute_sha256([serde_json::to_vec(&value)?]);
    Ok(format!("lifecycle-plan-{}", &digest[7..23]))
}

fn plan_digest(plan: &LifecyclePlan) -> Result<String, CoreError> {
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

fn resource_kind_label(kind: &LifecycleResourceKind) -> &'static str {
    match kind {
        LifecycleResourceKind::Tool => "tool",
        LifecycleResourceKind::Skill => "skill",
        LifecycleResourceKind::Mcp => "mcp",
        LifecycleResourceKind::Product => "product",
        LifecycleResourceKind::Operation => "operation",
    }
}
