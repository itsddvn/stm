import type { InventoryState } from "../../contracts/ui/state-contract";

const labels: Record<InventoryState, string> = {
  managed_current: "Current",
  managed_update_available: "Update available",
  blocked: "Blocked",
  external: "External",
  modified: "Modified",
  missing: "Not installed",
  unsupported: "Unsupported",
  manager_unavailable: "Manager unavailable",
  source_unavailable: "Source unavailable",
  invalid: "Invalid",
  conflict: "Conflict",
  unknown: "Unknown",
};

export function StatusBadge({ state }: { state: InventoryState }) {
  return <span className={`status-badge status-${state}`}>{labels[state]}</span>;
}
