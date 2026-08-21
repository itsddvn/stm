import type { SkillViewModel } from "../../../contracts/ui/view-model-contract";
import { ActionDisabledReason } from "../../components/action-disabled-reason";
import { AppIcon } from "../../components/app-icon";
import { StatusBadge } from "../../components/status-badge";

export function SkillDetailPanel({ skill, onReview }: { skill: SkillViewModel; onReview: () => void }) {
  const actionReasonId = `skill-action-${skill.id}`;
  return (
    <article className="detail-panel" aria-label={`${skill.name} details`}>
      <header className="detail-title">
        <span className="resource-glyph large"><AppIcon name="skills" size={24} /></span>
        <div><span className="detail-kicker">Trusted skill identity</span><h2>{skill.name}</h2><p>{skill.description}</p></div>
        <StatusBadge state={skill.state} />
      </header>
      <dl className="detail-grid">
        <div><dt>Catalog source</dt><dd>{skill.source}</dd></div>
        <div><dt>Installed revision</dt><dd className="mono-data">{skill.revision}</dd></div>
        <div><dt>Available revision</dt><dd className="mono-data">{skill.availableRevision ?? "Current"}</dd></div>
        <div><dt>Content digest</dt><dd className="mono-data">{skill.digest}</dd></div>
        <div><dt>Purposes</dt><dd>{skill.purposes.join(", ")}</dd></div>
        <div><dt>Risk flags</dt><dd>{skill.riskFlags.join(", ") || "None"}</dd></div>
      </dl>
      <section className="target-list">
        <h3>Installation Targets</h3>
        {skill.targets.map((target) => <div className="target-row" key={`${target.client}-${target.path}`}><span className={`target-mark target-${target.state}`}><AppIcon name={target.state === "current" ? "success" : target.state === "failed" ? "failure" : "warning"} size={16} /></span><strong>{target.client}</strong><span className="mono-data">{target.path}</span><small>{target.state}</small></div>)}
      </section>
      {skill.diff.length > 0 ? <section className="diff-preview"><h3>File Changes</h3>{skill.diff.map((entry) => <div className="diff-row" key={entry.file}><span className={`diff-kind diff-${entry.change}`}>{entry.change}</span><span className="mono-data">{entry.file}</span><p>{entry.summary}</p></div>)}</section> : null}
      <footer className="detail-actions">
        <button className="primary-button" type="button" disabled={!skill.primaryAction.enabled} aria-describedby={skill.primaryAction.disabledReasonCode ? actionReasonId : undefined} onClick={onReview}>{skill.primaryAction.label}</button>
        <button className="secondary-button" type="button"><AppIcon name="external" />Open Source</button>
      </footer>
      <ActionDisabledReason id={actionReasonId} reasonCode={skill.primaryAction.disabledReasonCode} />
    </article>
  );
}
