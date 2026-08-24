use serde::{Deserialize, Serialize};

use crate::{
    application::storage::OperationLogEntry,
    domain::{
        application_update::{ApplicationUpdateKind, ApplicationUpdateRecord, UpdateExecutionMode},
        inventory::{
            Freshness, InventoryState, LoadState, OwnershipKind, PrivilegeRequirement,
            SurfaceStateContract,
        },
        lifecycle::{LifecyclePlanRequest, LifecycleResourceKind},
        mcp::{
            AuthReferenceState, McpBindingScope, McpBindingState, McpClientName, McpHealthState,
            McpServerRecord, McpTransport, McpTrustState,
        },
        operation::OperationStatus,
        skill::{SkillClientName, SkillDiffKind, SkillRecord, SkillTargetState},
        source::{SourceAnalysisRecord, SourceAnalysisStatus, SourceKind, SourceTrust},
        tool::ToolRecord,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppViewModelDto {
    pub surface: SurfaceStateDto,
    pub tools: Vec<ToolViewModelDto>,
    pub skills: Vec<SkillViewModelDto>,
    pub mcp_servers: Vec<McpServerViewModelDto>,
    pub updates: Vec<UpdateViewModelDto>,
    pub operations: Vec<OperationViewModelDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RefreshStatusDto {
    pub surface: SurfaceStateDto,
    pub last_snapshot_at: String,
    pub warning_count: usize,
    pub warnings: Vec<String>,
    pub in_progress: bool,
    pub can_cancel: bool,
    pub operation_id: Option<String>,
    pub current_step: Option<String>,
    pub steps_completed: usize,
    pub total_steps: usize,
    pub snapshot: Option<AppViewModelDto>,
    pub result: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PresentationActionDto {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub disabled_reason_code: Option<String>,
    pub presentation_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceStateDto {
    pub load_state: LoadState,
    pub reason_code: Option<String>,
    pub freshness: Freshness,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolViewModelDto {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub kind: String,
    pub groups: Vec<String>,
    pub recommended: bool,
    pub state: InventoryState,
    pub owner: String,
    pub ownership_kind: OwnershipKind,
    pub execution_mode: crate::domain::inventory::ExecutionMode,
    pub installed_version: Option<String>,
    pub available_version: Option<String>,
    pub manager: String,
    pub package_id: String,
    pub platform: String,
    pub privilege: PrivilegeRequirement,
    pub lifecycle_confidence: String,
    pub reason_code: Option<String>,
    pub primary_action: PresentationActionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillTargetViewModelDto {
    pub client: SkillClientName,
    pub path: String,
    pub state: SkillTargetState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillDiffViewModelDto {
    pub file: String,
    pub change: SkillDiffKind,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillViewModelDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub revision: String,
    pub available_revision: Option<String>,
    pub digest: String,
    pub state: InventoryState,
    pub purposes: Vec<String>,
    pub targets: Vec<SkillTargetViewModelDto>,
    pub risk_flags: Vec<String>,
    pub diff: Vec<SkillDiffViewModelDto>,
    pub primary_action: PresentationActionDto,
    pub resolution_actions: Vec<PresentationActionDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpClientBindingViewModelDto {
    pub client: McpClientName,
    pub state: McpBindingState,
    pub scope: McpBindingScope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerViewModelDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub transport: McpTransport,
    pub command_or_url: String,
    pub clients: Vec<McpClientBindingViewModelDto>,
    pub capabilities: Vec<String>,
    pub trust: McpTrustState,
    pub auth_state: AuthReferenceState,
    pub health: McpHealthState,
    pub last_checked: String,
    pub state: InventoryState,
    pub primary_action: PresentationActionDto,
    pub toggle_action: PresentationActionDto,
    pub remove_action: PresentationActionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateViewModelDto {
    pub id: String,
    pub resource_type: ApplicationUpdateKind,
    pub name: String,
    pub current: String,
    pub target: String,
    pub execution_mode: UpdateExecutionMode,
    pub selected: bool,
    pub risk: String,
    pub selection_action: Option<PresentationActionDto>,
    pub review_action: Option<PresentationActionDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationViewModelDto {
    pub id: String,
    pub resource: String,
    pub action: String,
    pub status: OperationStatus,
    pub started_at: String,
    pub owner: String,
    pub detail: String,
    pub receipt: String,
    pub details: Vec<String>,
    pub lifecycle_request: LifecyclePlanRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceAnalysisViewModelDto {
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
    pub lifecycle_request: LifecyclePlanRequest,
}

impl From<SurfaceStateContract> for SurfaceStateDto {
    fn from(value: SurfaceStateContract) -> Self {
        Self {
            load_state: value.load_state,
            reason_code: value.reason_code,
            freshness: value.freshness,
        }
    }
}

impl From<&ToolRecord> for ToolViewModelDto {
    fn from(value: &ToolRecord) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            summary: value.summary.clone(),
            kind: value.kind.clone(),
            groups: value.groups.clone(),
            recommended: value.recommended,
            state: value.state.clone(),
            owner: value.owner.clone(),
            ownership_kind: value.ownership_kind.clone(),
            execution_mode: value.execution_mode.clone(),
            installed_version: value.installed_version.clone(),
            available_version: value.available_version.clone(),
            manager: value.manager.clone(),
            package_id: value.package_id.clone(),
            platform: value.platform.clone(),
            privilege: value.privilege.clone(),
            lifecycle_confidence: value.lifecycle_confidence.clone(),
            reason_code: value.reason_code.clone(),
            primary_action: build_tool_primary_action(value),
        }
    }
}

impl From<&SkillRecord> for SkillViewModelDto {
    fn from(value: &SkillRecord) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            description: value.description.clone(),
            source: value.source.clone(),
            revision: value.revision.clone(),
            available_revision: value.available_revision.clone(),
            digest: value.digest.clone(),
            state: value.state.clone(),
            purposes: value.purposes.clone(),
            targets: value
                .targets
                .iter()
                .map(|target| SkillTargetViewModelDto {
                    client: target.client.clone(),
                    path: target.path.clone(),
                    state: target.state.clone(),
                })
                .collect(),
            risk_flags: value.risk_flags.clone(),
            diff: value
                .diff
                .iter()
                .map(|diff| SkillDiffViewModelDto {
                    file: diff.file.clone(),
                    change: diff.change.clone(),
                    summary: diff.summary.clone(),
                })
                .collect(),
            primary_action: build_skill_primary_action(value),
            resolution_actions: build_skill_resolution_actions(value),
        }
    }
}

impl From<&McpServerRecord> for McpServerViewModelDto {
    fn from(value: &McpServerRecord) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            description: value.description.clone(),
            source: value.source.clone(),
            transport: value.transport.clone(),
            command_or_url: value.command_or_url.clone(),
            clients: value
                .clients
                .iter()
                .map(|client| McpClientBindingViewModelDto {
                    client: client.client.clone(),
                    state: client.state.clone(),
                    scope: client.scope.clone(),
                })
                .collect(),
            capabilities: value.capabilities.clone(),
            trust: value.trust.clone(),
            auth_state: value.auth_state.clone(),
            health: value.health.clone(),
            last_checked: value.last_checked.clone(),
            state: value.state.clone(),
            primary_action: build_mcp_primary_action(value),
            toggle_action: build_mcp_toggle_action(value),
            remove_action: enabled_action("mcp.review_remove", "Review Removal"),
        }
    }
}

impl From<&ApplicationUpdateRecord> for UpdateViewModelDto {
    fn from(value: &ApplicationUpdateRecord) -> Self {
        let selection_action = match value.resource_type {
            ApplicationUpdateKind::Product => None,
            _ if value.risk.contains("Blocked by local modification") => Some(disabled_action(
                "update.select_queue_item",
                &format!("Select {}", value.name),
                "action.update.conflict_resolution_required",
            )),
            _ => Some(enabled_action(
                "update.select_queue_item",
                &format!("Select {}", value.name),
            )),
        };
        let review_action = match value.resource_type {
            ApplicationUpdateKind::Product => Some(enabled_action(
                "product_update.preview",
                "Review Product Update",
            )),
            _ => None,
        };
        Self {
            id: value.id.clone(),
            resource_type: value.resource_type.clone(),
            name: value.name.clone(),
            current: value.current.clone(),
            target: value.target.clone(),
            execution_mode: value.execution_mode.clone(),
            selected: value.selected,
            risk: value.risk.clone(),
            selection_action,
            review_action,
        }
    }
}

impl From<&OperationLogEntry> for OperationViewModelDto {
    fn from(value: &OperationLogEntry) -> Self {
        Self {
            id: value.receipt.operation_id.clone(),
            resource: value.resource.clone(),
            action: value.action.clone(),
            status: value.receipt.status.clone(),
            started_at: value.receipt.started_at.clone(),
            owner: value.owner.clone(),
            detail: value.receipt.summary.clone(),
            receipt: value.receipt.id.clone(),
            details: value.receipt.details.clone(),
            lifecycle_request: value.lifecycle_request.clone().unwrap_or_else(|| {
                LifecyclePlanRequest {
                    resource_kind: LifecycleResourceKind::Operation,
                    action: if matches!(
                        value.receipt.status,
                        OperationStatus::Failed
                            | OperationStatus::Cancelled
                            | OperationStatus::Recoverable
                    ) {
                        "recover"
                    } else {
                        "inspect-receipt"
                    }
                    .to_string(),
                    resource_id: value.receipt.operation_id.clone(),
                    source_analysis_handle: None,
                    item_ids: None,
                    children: Vec::new(),
                    mapping_id: None,
                }
            }),
        }
    }
}

impl SourceAnalysisViewModelDto {
    pub fn from_analysis(
        value: SourceAnalysisRecord,
        lifecycle_request: LifecyclePlanRequest,
    ) -> Self {
        Self {
            kind: value.kind,
            submitted_url: value.submitted_url,
            normalized_url: value.normalized_url,
            status: value.status,
            detected_name: value.detected_name,
            source_host: value.source_host,
            source_type: value.source_type,
            publisher: value.publisher,
            target: value.target,
            trust: value.trust,
            risk_flags: value.risk_flags,
            notes: value.notes,
            lifecycle_request,
        }
    }
}

fn build_tool_primary_action(tool: &ToolRecord) -> PresentationActionDto {
    let install_action = tool.state == InventoryState::Missing;
    let current_action = tool.state == InventoryState::ManagedCurrent;
    let base_id = if install_action {
        "tool.review_install"
    } else if current_action {
        "tool.inspect_current"
    } else {
        "tool.review_managed_update"
    };
    let base_label = if install_action {
        "Preview Install"
    } else if current_action {
        "Installed"
    } else {
        "Preview Managed Update"
    };

    if tool.execution_mode == crate::domain::inventory::ExecutionMode::VendorHandoff {
        return enabled_action("tool.review_vendor_handoff", "Review Handoff");
    }
    if tool.execution_mode == crate::domain::inventory::ExecutionMode::DetectOnly {
        return enabled_action(
            "tool.review_guidance",
            if install_action {
                "View Install Guidance"
            } else {
                "View Update Guidance"
            },
        );
    }

    let disabled_reason = match tool.state {
        InventoryState::Unsupported => Some("action.mapping.unsupported"),
        InventoryState::Blocked => Some("action.mapping.blocked"),
        InventoryState::ManagerUnavailable => Some("action.manager.unavailable"),
        InventoryState::External => Some("action.execution.external"),
        InventoryState::Unknown => Some("action.execution.unknown"),
        _ => None,
    }
    .or_else(|| {
        (tool.ownership_kind == OwnershipKind::SystemOwned)
            .then_some("action.execution.system_owned")
    });

    match disabled_reason {
        Some(reason) => disabled_action(base_id, base_label, reason),
        None => enabled_action(base_id, base_label),
    }
}

fn build_skill_primary_action(skill: &SkillRecord) -> PresentationActionDto {
    match skill.state {
        InventoryState::Modified => {
            enabled_action("skill.resolve_local_modification", "Resolve Conflict")
        }
        InventoryState::Conflict => {
            enabled_action("skill.review_partial_failure", "Review Partial Failure")
        }
        InventoryState::Missing => enabled_action("skill.review_install", "Review Install"),
        InventoryState::ManagedUpdateAvailable => {
            enabled_action("skill.review_update", "Review Update")
        }
        _ => enabled_action("skill.inspect_receipt", "Inspect Receipt"),
    }
}

fn build_skill_resolution_actions(skill: &SkillRecord) -> Vec<PresentationActionDto> {
    match skill.state {
        InventoryState::Modified => vec![
            enabled_action("skill.keep_local", "Keep Local"),
            enabled_action("skill.export_diff", "Export Diff"),
            enabled_action("skill.restore_managed", "Restore Managed"),
            disabled_action(
                "skill.install_side_by_side",
                "Install Side by Side",
                "action.skill.side_by_side_unsupported",
            ),
        ],
        InventoryState::Conflict => vec![
            enabled_action(
                "skill.rollback_completed_target",
                "Roll Back Completed Target",
            ),
            enabled_action("skill.retry_failed_target", "Retry Failed Target"),
            enabled_action("skill.keep_partial_result", "Keep Partial Result"),
        ],
        _ => Vec::new(),
    }
}

fn build_mcp_primary_action(server: &McpServerRecord) -> PresentationActionDto {
    if server.trust == McpTrustState::Blocked {
        return disabled_action(
            "mcp.review_configuration",
            "Review Configuration",
            "action.source.untrusted",
        );
    }
    if server.auth_state == AuthReferenceState::ReferenceMissing {
        return disabled_action(
            "mcp.review_configuration",
            "Review Configuration",
            "action.mcp.auth_reference_missing",
        );
    }
    enabled_action("mcp.review_configuration", "Review Configuration")
}

fn build_mcp_toggle_action(server: &McpServerRecord) -> PresentationActionDto {
    if server
        .clients
        .iter()
        .any(|binding| binding.state == McpBindingState::Enabled)
    {
        return enabled_action("mcp.review_disable", "Review Disable");
    }
    if server.trust == McpTrustState::Blocked {
        return disabled_action(
            "mcp.review_enable",
            "Review Enable",
            "action.source.untrusted",
        );
    }
    if server.auth_state == AuthReferenceState::ReferenceMissing {
        return disabled_action(
            "mcp.review_enable",
            "Review Enable",
            "action.mcp.auth_reference_missing",
        );
    }
    if server
        .clients
        .iter()
        .all(|binding| binding.state == McpBindingState::Unsupported)
    {
        return disabled_action(
            "mcp.review_enable",
            "Review Enable",
            "action.mcp.client_unsupported",
        );
    }
    enabled_action("mcp.review_enable", "Review Enable")
}

fn enabled_action(id: &str, label: &str) -> PresentationActionDto {
    PresentationActionDto {
        id: id.to_string(),
        label: label.to_string(),
        enabled: true,
        disabled_reason_code: None,
        presentation_only: true,
    }
}

fn disabled_action(id: &str, label: &str, reason: &str) -> PresentationActionDto {
    PresentationActionDto {
        id: id.to_string(),
        label: label.to_string(),
        enabled: false,
        disabled_reason_code: Some(reason.to_string()),
        presentation_only: true,
    }
}
