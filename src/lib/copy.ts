import type { ActionDisabledReasonCode } from "../../contracts/ui/action-contract";
import type { ReasonCode, ScenarioId } from "../../contracts/ui/state-contract";

export const navCopy = {
  dashboard: "Dashboard",
  tools: "Tools",
  skills: "Skills",
  mcp: "MCP Servers",
  updates: "Updates",
  history: "Operation History",
  settings: "Settings",
} as const;

export const scenarioLabels: Record<ScenarioId, string> = {
  success: "Success",
  empty: "Empty",
  loading: "Loading",
  partial: "Partial",
  stale: "Stale",
  unsupported: "Unsupported",
  blocked: "Blocked",
  manager_unavailable: "Manager unavailable",
  offline: "Offline",
  cancelled: "Cancelled",
  failure: "Failure",
  recovery: "Recovery",
};

export const reasonCopy: Record<ReasonCode, { title: string; detail: string }> = {
  "inventory.empty": { title: "No inventory found", detail: "Run a fixture refresh or choose another scenario." },
  "inventory.loading": { title: "Loading inventory…", detail: "Fixture data is waiting at the typed IPC boundary." },
  "inventory.partial": { title: "Inventory is incomplete", detail: "Some adapters returned data. Review the affected rows before acting." },
  "inventory.stale": { title: "Inventory may be stale", detail: "The last fixture scan is outside the freshness window." },
  "mapping.unsupported": { title: "Mapping not supported", detail: "This platform mapping has no authorized lifecycle path." },
  "mapping.blocked": { title: "Mapping blocked", detail: "Policy rejected this mapping. No operation can start." },
  "manager.unavailable": { title: "Manager unavailable", detail: "The authoritative manager is not available on this fixture machine." },
  "network.offline": { title: "Update source offline", detail: "Installed inventory is still available. Retry metadata checks when connected." },
  "operation.cancelled": { title: "Operation cancelled", detail: "No further steps will run. Review the operation record for completed work." },
  "operation.failed": { title: "Operation failed", detail: "Review the failure boundary, then retry or keep the current installation." },
  "operation.recovery_available": { title: "Recovery available", detail: "A previous managed revision can be restored from the fixture receipt." },
  "skill.local_modification": { title: "Local changes detected", detail: "Choose how to preserve or replace local content before updating." },
  "skill.partial_failure": { title: "Some targets failed", detail: "Successful targets are recorded separately. Roll back or retry failed targets." },
  "product_update.recovery_available": { title: "Product recovery available", detail: "The signed product update can return to the previous fixture release." },
  "source.invalid": { title: "Source URL blocked", detail: "Enter an HTTPS URL without embedded credentials before review." },
  "source.review_required": { title: "Source review required", detail: "Review publisher, targets, capabilities, and risk before creating an installation preview." },
  "mcp.auth_reference_missing": { title: "Authentication reference missing", detail: "Choose an OS-managed credential reference before enabling this server." },
  "mcp.client_unsupported": { title: "Client unsupported", detail: "This client configuration schema cannot represent the reviewed server safely." },
  "mcp.health_degraded": { title: "MCP health degraded", detail: "The last fixture connection check did not complete cleanly." },
};

export const actionDisabledReasonCopy: Record<ActionDisabledReasonCode, { title: string; detail: string }> = {
  "action.mapping.unsupported": { title: "Unsupported mapping", detail: "This tool has no authorized lifecycle mapping for the current platform." },
  "action.mapping.blocked": { title: "Policy blocked", detail: "The authoritative policy blocked managed execution for this mapping." },
  "action.manager.unavailable": { title: "Manager unavailable", detail: "The authoritative manager is unavailable, so the preview cannot enter managed execution." },
  "action.execution.external": { title: "External ownership", detail: "This install has no trusted manager receipt. Review publisher guidance instead of managed execution." },
  "action.execution.system_owned": { title: "System-owned", detail: "The operating system owns this install. Managed execution stays disabled in this fixture." },
  "action.execution.unknown": { title: "Unknown authority", detail: "Ownership cannot be verified, so managed execution remains unavailable." },
  "action.execution.detect_only": { title: "Detect only", detail: "This mapping may be inspected, but it cannot enter managed execution." },
  "action.execution.handoff_only": { title: "Handoff only", detail: "This mapping can open a reviewed vendor handoff, not a managed execution plan." },
  "action.skill.local_modification": { title: "Conflict action required", detail: "Select Keep Local, Export Diff, Restore Managed, or Side by Side before including this skill in a generic review queue." },
  "action.skill.side_by_side_unsupported": { title: "Side by side unavailable", detail: "This fixture target cannot install another managed copy under a separate name." },
  "action.update.conflict_resolution_required": { title: "Queue selection blocked", detail: "Choose Keep Local, Export Diff, Restore Managed, or Side by Side from the skill conflict flow first." },
  "action.source.invalid": { title: "Invalid source", detail: "Use a complete HTTPS URL without embedded credentials." },
  "action.source.untrusted": { title: "Source not trusted", detail: "The source may be inspected, but configuration or installation review remains blocked." },
  "action.mcp.auth_reference_missing": { title: "Authentication required", detail: "Select an OS-managed credential reference before reviewing this MCP configuration." },
  "action.mcp.client_unsupported": { title: "Unsupported client", detail: "The selected client cannot represent this server configuration safely." },
};
