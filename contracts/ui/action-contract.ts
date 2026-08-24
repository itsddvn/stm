export const actionIds = [
  "inventory.refresh",
  "tool.analyze_source",
  "tool.review_source_install",
  "tool.review_install",
  "tool.review_managed_update",
  "tool.inspect_current",
  "tool.review_vendor_handoff",
  "tool.review_guidance",
  "setup.review_install",
  "setup.change_provider",
  "setup.import_config",
  "setup.export_config",
  "skill.review_install",
  "skill.analyze_source",
  "skill.review_source_install",
  "skill.review_update",
  "skill.resolve_local_modification",
  "skill.review_partial_failure",
  "skill.inspect_receipt",
  "skill.keep_local",
  "skill.export_diff",
  "skill.restore_managed",
  "skill.install_side_by_side",
  "skill.rollback_completed_target",
  "skill.retry_failed_target",
  "skill.keep_partial_result",
  "mcp.analyze_source",
  "mcp.review_add",
  "mcp.review_configuration",
  "mcp.review_enable",
  "mcp.review_disable",
  "mcp.review_remove",
  "update.select_queue_item",
  "product_update.preview",
  "product_update.recover",
  "operation.cancel",
  "operation.retry",
] as const;

export type ActionId = (typeof actionIds)[number];

export const actionDisabledReasonCodes = [
  "action.mapping.unsupported",
  "action.mapping.blocked",
  "action.manager.unavailable",
  "action.execution.external",
  "action.execution.system_owned",
  "action.execution.unknown",
  "action.execution.detect_only",
  "action.execution.handoff_only",
  "action.skill.local_modification",
  "action.skill.side_by_side_unsupported",
  "action.update.conflict_resolution_required",
  "action.source.invalid",
  "action.source.untrusted",
  "action.mcp.auth_reference_missing",
  "action.mcp.client_unsupported",
] as const;

export type ActionDisabledReasonCode = (typeof actionDisabledReasonCodes)[number];
export type ToolActionId = Extract<ActionId, `tool.${string}`>;
export type SkillActionId = Extract<ActionId, `skill.${string}`>;
export type McpActionId = Extract<ActionId, `mcp.${string}`>;
export type UpdateActionId = Extract<ActionId, `update.${string}`>;
export type ProductUpdateActionId = Extract<ActionId, `product_update.${string}`>;

export interface PresentationAction<TId extends ActionId = ActionId> {
  id: TId;
  label: string;
  enabled: boolean;
  disabledReasonCode?: ActionDisabledReasonCode;
  presentationOnly: true;
}

export type ToolPresentationAction = PresentationAction<ToolActionId>;
export type SkillPresentationAction = PresentationAction<SkillActionId>;
export type McpPresentationAction = PresentationAction<McpActionId>;
export type UpdateSelectionAction = PresentationAction<UpdateActionId>;
export type ProductUpdateAction = PresentationAction<ProductUpdateActionId>;
