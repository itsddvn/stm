import type { InventoryState } from "../../contracts/ui/state-contract";
import { useI18n, type MessageKey } from "../lib/i18n";

const labelKeys: Record<InventoryState, MessageKey> = {
  managed_current: "state.managed_current",
  managed_update_available: "state.managed_update_available",
  blocked: "state.blocked",
  external: "state.external",
  modified: "state.modified",
  missing: "state.missing",
  unsupported: "state.unsupported",
  manager_unavailable: "state.manager_unavailable",
  source_unavailable: "state.source_unavailable",
  invalid: "state.invalid",
  conflict: "state.conflict",
  unknown: "state.unknown",
};

export function StatusBadge({ state }: { state: InventoryState }) {
  const { t } = useI18n();
  return <span className={`status-badge status-${state}`}>{t(labelKeys[state])}</span>;
}
