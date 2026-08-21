import type { ToolViewModel } from "../../../contracts/ui/view-model-contract";
import { ActionDisabledReason } from "../../components/action-disabled-reason";
import { AppIcon } from "../../components/app-icon";
import { OwnershipRail } from "../../components/ownership-rail";
import { StatusBadge } from "../../components/status-badge";

export function ToolDetailPanel({ tool, onPreview }: { tool: ToolViewModel; onPreview: () => void }) {
  const actionReasonId = `tool-action-${tool.id}`;
  return (
    <article className="detail-panel" aria-label={`${tool.name} details`}>
      <header className="detail-title">
        <span className="resource-glyph large"><AppIcon name={tool.kind === "CLI tool" ? "terminal" : "package"} size={24} /></span>
        <div><span className="detail-kicker">{tool.kind}</span><h2>{tool.name}</h2><p>{tool.summary}</p></div>
        <StatusBadge state={tool.state} />
      </header>
      <OwnershipRail owner={tool.owner} mode={tool.executionMode} />
      <dl className="detail-grid">
        <div><dt>Installed</dt><dd className="mono-data">{tool.installedVersion ?? "Not installed"}</dd></div>
        <div><dt>Available</dt><dd className="mono-data">{tool.availableVersion ?? "Not checked"}</dd></div>
        <div><dt>Manager</dt><dd>{tool.manager}</dd></div>
        <div><dt>Package ID</dt><dd className="mono-data">{tool.packageId}</dd></div>
        <div><dt>Platform</dt><dd>{tool.platform}</dd></div>
        <div><dt>Privilege</dt><dd>{tool.privilege}</dd></div>
        <div><dt>Lifecycle confidence</dt><dd>{tool.lifecycleConfidence}</dd></div>
        <div><dt>Groups</dt><dd>{tool.groups.join(", ")}</dd></div>
      </dl>
      <section className="mapping-matrix">
        <h3>Platform Mapping</h3>
        <div className="matrix-row"><span>macOS · arm64</span><strong>{tool.executionMode.replaceAll("_", " ")}</strong><span>{tool.lifecycleConfidence}</span></div>
        <div className="matrix-row muted"><span>Windows · x64</span><strong>Review pending</strong><span>Detect only</span></div>
        <div className="matrix-row muted"><span>Linux · x64</span><strong>Mapping varies</strong><span>Unsupported fixture</span></div>
      </section>
      <footer className="detail-actions">
        <button className="primary-button" type="button" disabled={!tool.primaryAction.enabled} aria-describedby={tool.primaryAction.disabledReasonCode ? actionReasonId : undefined} onClick={onPreview}>{tool.primaryAction.label}</button>
        <button className="secondary-button" type="button"><AppIcon name="external" />Publisher Details</button>
      </footer>
      <ActionDisabledReason id={actionReasonId} reasonCode={tool.primaryAction.disabledReasonCode} />
    </article>
  );
}
