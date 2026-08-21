import { AppIcon } from "./app-icon";

export function OwnershipRail({ owner, mode, compact = false }: { owner: string; mode: string; compact?: boolean }) {
  return (
    <div className={`ownership-rail ${compact ? "ownership-rail-compact" : ""}`}>
      <span className="ownership-node"><AppIcon name="manager" size={16} /></span>
      <span className="ownership-line" aria-hidden="true" />
      <span className="ownership-copy">
        <small>Authority</small>
        <strong>{owner}</strong>
        {!compact ? <span>{mode.replaceAll("_", " ")}</span> : null}
      </span>
    </div>
  );
}
