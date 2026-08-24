import { useI18n, type MessageKey } from "../lib/i18n";
import { AppIcon } from "./app-icon";

const modeKeys: Record<string, MessageKey> = {
  managed_execute: "mode.managed_execute",
  vendor_handoff: "mode.vendor_handoff",
  detect_only: "mode.detect_only",
};

export function OwnershipRail({ owner, mode, compact = false }: { owner: string; mode: string; compact?: boolean }) {
  const { t } = useI18n();
  const localizedOwner = owner === "External"
    ? t("owner.external")
    : owner.toLowerCase().includes("updater")
      ? t("owner.vendor")
      : owner;
  return (
    <div className={`ownership-rail ${compact ? "ownership-rail-compact" : ""}`}>
      <span className="ownership-node"><AppIcon name="manager" size={16} /></span>
      <span className="ownership-line" aria-hidden="true" />
      <span className="ownership-copy">
        <small>{t("owner.authority")}</small>
        <strong>{localizedOwner}</strong>
        {!compact ? <span>{modeKeys[mode] ? t(modeKeys[mode]) : mode.replaceAll("_", " ")}</span> : null}
      </span>
    </div>
  );
}
