import type {
  ActionDisabledReasonCode,
  McpPresentationAction,
  ProductUpdateAction,
  SkillPresentationAction,
  ToolPresentationAction,
  UpdateSelectionAction,
} from "../../contracts/ui/action-contract";
import type {
  McpServerViewModel,
  SkillViewModel,
  ToolViewModel,
  UpdateViewModel,
} from "../../contracts/ui/view-model-contract";

type ToolActionSeed = Pick<
  ToolViewModel,
  "availableVersion" | "executionMode" | "installedVersion" | "ownershipKind" | "state"
>;

type SkillActionSeed = Pick<SkillViewModel, "availableRevision" | "state" | "targets">;
type McpActionSeed = Pick<McpServerViewModel, "authState" | "clients" | "trust">;


type UpdateActionOptions = {
  conflictResolutionRequired?: boolean;
};

export function buildToolPrimaryAction(tool: ToolActionSeed): ToolPresentationAction {
  const installAction = tool.state === "missing";
  const currentAction = tool.state === "managed_current";
  const baseActionId = installAction ? "tool.review_install" : currentAction ? "tool.inspect_current" : "tool.review_managed_update";
  const baseLabel = installAction ? "Preview Install" : currentAction ? "Installed" : "Preview Managed Update";

  if (tool.executionMode === "vendor_handoff") {
    return enabledToolAction("tool.review_vendor_handoff", "Review Handoff");
  }

  if (tool.executionMode === "detect_only") {
    return enabledToolAction(
      "tool.review_guidance",
      installAction ? "View Install Guidance" : "View Update Guidance",
    );
  }

  switch (tool.state) {
    case "unsupported":
      return disabledToolAction(baseActionId, baseLabel, "action.mapping.unsupported");
    case "blocked":
      return disabledToolAction(baseActionId, baseLabel, "action.mapping.blocked");
    case "manager_unavailable":
      return disabledToolAction(baseActionId, baseLabel, "action.manager.unavailable");
    case "external":
      return disabledToolAction(baseActionId, baseLabel, "action.execution.external");
    case "unknown":
      return disabledToolAction(baseActionId, baseLabel, "action.execution.unknown");
    default:
      break;
  }

  if (tool.ownershipKind === "system_owned") {
    return disabledToolAction(baseActionId, baseLabel, "action.execution.system_owned");
  }

  return enabledToolAction(baseActionId, baseLabel);
}

export function buildSkillPrimaryAction(skill: SkillActionSeed): SkillPresentationAction {
  switch (skill.state) {
    case "modified":
      return enabledSkillAction("skill.resolve_local_modification", "Resolve Conflict");
    case "conflict":
      return enabledSkillAction("skill.review_partial_failure", "Review Partial Failure");
    case "missing":
      return enabledSkillAction("skill.review_install", "Review Install");
    case "managed_update_available":
      return enabledSkillAction("skill.review_update", "Review Update");
    default:
      return enabledSkillAction("skill.inspect_receipt", "Inspect Receipt");
  }
}

export function buildSkillResolutionActions(
  skill: SkillActionSeed,
  sideBySideSupported: boolean,
): SkillPresentationAction[] {
  if (skill.state === "modified") {
    return [
      enabledSkillAction("skill.keep_local", "Keep Local"),
      enabledSkillAction("skill.export_diff", "Export Diff"),
      enabledSkillAction("skill.restore_managed", "Restore Managed"),
      sideBySideSupported
        ? enabledSkillAction("skill.install_side_by_side", "Install Side by Side")
        : disabledSkillAction(
            "skill.install_side_by_side",
            "Install Side by Side",
            "action.skill.side_by_side_unsupported",
          ),
    ];
  }

  if (skill.state === "conflict") {
    return [
      enabledSkillAction("skill.rollback_completed_target", "Roll Back Completed Target"),
      enabledSkillAction("skill.retry_failed_target", "Retry Failed Target"),
      enabledSkillAction("skill.keep_partial_result", "Keep Partial Result"),
    ];
  }

  return [];
}
export function buildMcpPrimaryAction(server: McpActionSeed): McpPresentationAction {
  if (server.trust === "blocked") {
    return disabledMcpAction(
      "mcp.review_configuration",
      "Review Configuration",
      "action.source.untrusted",
    );
  }
  if (server.authState === "reference_missing") {
    return disabledMcpAction(
      "mcp.review_configuration",
      "Review Configuration",
      "action.mcp.auth_reference_missing",
    );
  }
  return enabledMcpAction("mcp.review_configuration", "Review Configuration");
}

export function buildMcpToggleAction(server: McpActionSeed): McpPresentationAction {
  const hasEnabledClient = server.clients.some((binding) => binding.state === "enabled");
  if (hasEnabledClient) {
    return enabledMcpAction("mcp.review_disable", "Review Disable");
  }
  if (server.trust === "blocked") {
    return disabledMcpAction("mcp.review_enable", "Review Enable", "action.source.untrusted");
  }
  if (server.authState === "reference_missing") {
    return disabledMcpAction(
      "mcp.review_enable",
      "Review Enable",
      "action.mcp.auth_reference_missing",
    );
  }
  if (server.clients.every((binding) => binding.state === "unsupported")) {
    return disabledMcpAction(
      "mcp.review_enable",
      "Review Enable",
      "action.mcp.client_unsupported",
    );
  }
  return enabledMcpAction("mcp.review_enable", "Review Enable");
}

export function withMcpPresentationActions(
  server: Omit<McpServerViewModel, "primaryAction" | "removeAction" | "toggleAction">,
): McpServerViewModel {
  return {
    ...server,
    primaryAction: buildMcpPrimaryAction(server),
    toggleAction: buildMcpToggleAction(server),
    removeAction: enabledMcpAction("mcp.review_remove", "Review Removal"),
  };
}


export function buildUpdateSelectionAction(
  update: Pick<UpdateViewModel, "name" | "resourceType">,
  options?: UpdateActionOptions,
): UpdateSelectionAction | undefined {
  if (update.resourceType === "product") return undefined;
  if (options?.conflictResolutionRequired) {
    return disabledUpdateAction(
      "update.select_queue_item",
      `Select ${update.name}`,
      "action.update.conflict_resolution_required",
    );
  }
  return enabledUpdateAction("update.select_queue_item", `Select ${update.name}`);
}

export function buildProductReviewAction(): ProductUpdateAction {
  return {
    id: "product_update.preview",
    label: "Review Product Update",
    enabled: true,
    presentationOnly: true,
  };
}

export function withToolPresentationAction(
  tool: Omit<ToolViewModel, "primaryAction">,
): ToolViewModel {
  return { ...tool, primaryAction: buildToolPrimaryAction(tool) };
}

export function withSkillPresentationActions(
  skill: Omit<SkillViewModel, "primaryAction" | "resolutionActions">,
  sideBySideSupported: boolean,
): SkillViewModel {
  return {
    ...skill,
    primaryAction: buildSkillPrimaryAction(skill),
    resolutionActions: buildSkillResolutionActions(skill, sideBySideSupported),
  };
}

export function withUpdatePresentationActions(
  update: Omit<UpdateViewModel, "reviewAction" | "selectionAction">,
  options?: UpdateActionOptions,
): UpdateViewModel {
  return {
    ...update,
    selectionAction: buildUpdateSelectionAction(update, options),
    reviewAction: update.resourceType === "product" ? buildProductReviewAction() : undefined,
  };
}

function enabledToolAction(
  id: ToolPresentationAction["id"],
  label: string,
): ToolPresentationAction {
  return { id, label, enabled: true, presentationOnly: true };
}

function disabledToolAction(
  id: ToolPresentationAction["id"],
  label: string,
  disabledReasonCode: ActionDisabledReasonCode,
): ToolPresentationAction {
  return { id, label, enabled: false, disabledReasonCode, presentationOnly: true };
}

function enabledSkillAction(
  id: SkillPresentationAction["id"],
  label: string,
): SkillPresentationAction {
  return { id, label, enabled: true, presentationOnly: true };
}

function disabledSkillAction(
  id: SkillPresentationAction["id"],
  label: string,
  disabledReasonCode: ActionDisabledReasonCode,
): SkillPresentationAction {
  return { id, label, enabled: false, disabledReasonCode, presentationOnly: true };
}

function enabledMcpAction(
  id: McpPresentationAction["id"],
  label: string,
): McpPresentationAction {
  return { id, label, enabled: true, presentationOnly: true };
}

function disabledMcpAction(
  id: McpPresentationAction["id"],
  label: string,
  disabledReasonCode: ActionDisabledReasonCode,
): McpPresentationAction {
  return { id, label, enabled: false, disabledReasonCode, presentationOnly: true };
}

function enabledUpdateAction(
  id: UpdateSelectionAction["id"],
  label: string,
): UpdateSelectionAction {
  return { id, label, enabled: true, presentationOnly: true };
}

function disabledUpdateAction(
  id: UpdateSelectionAction["id"],
  label: string,
  disabledReasonCode: ActionDisabledReasonCode,
): UpdateSelectionAction {
  return { id, label, enabled: false, disabledReasonCode, presentationOnly: true };
}
