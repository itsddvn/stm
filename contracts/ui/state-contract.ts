export const scenarioIds = [
  "success",
  "empty",
  "loading",
  "partial",
  "stale",
  "unsupported",
  "blocked",
  "manager_unavailable",
  "offline",
  "cancelled",
  "failure",
  "recovery",
] as const;

export type ScenarioId = (typeof scenarioIds)[number];

export const reasonCodes = [
  "inventory.empty",
  "inventory.loading",
  "inventory.partial",
  "inventory.stale",
  "mapping.unsupported",
  "mapping.blocked",
  "manager.unavailable",
  "network.offline",
  "operation.cancelled",
  "operation.failed",
  "operation.recovery_available",
  "skill.local_modification",
  "skill.partial_failure",
  "product_update.recovery_available",
  "source.invalid",
  "source.review_required",
  "mcp.auth_reference_missing",
  "mcp.client_unsupported",
  "mcp.health_degraded",
] as const;

export type ReasonCode = (typeof reasonCodes)[number];
export type LoadState = "ready" | "empty" | "loading" | "partial" | "error";
export type InventoryState =
  | "managed_current"
  | "managed_update_available"
  | "blocked"
  | "external"
  | "modified"
  | "missing"
  | "unsupported"
  | "manager_unavailable"
  | "source_unavailable"
  | "invalid"
  | "conflict"
  | "unknown";

export interface SurfaceState {
  loadState: LoadState;
  reasonCode?: ReasonCode;
  freshness: "fresh" | "stale" | "unknown";
}
