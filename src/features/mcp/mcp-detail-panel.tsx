import type { McpServerViewModel } from "../../../contracts/ui/view-model-contract";
import { ActionDisabledReason } from "../../components/action-disabled-reason";
import { AppIcon } from "../../components/app-icon";
import { StatusBadge } from "../../components/status-badge";

export function McpDetailPanel({
  server,
  onConfigure,
  onRemove,
  onToggle,
}: {
  server: McpServerViewModel;
  onConfigure: () => void;
  onRemove: () => void;
  onToggle: () => void;
}) {
  return (
    <section className="detail-panel" aria-label={`${server.name} details`}>
      <div className="detail-title">
        <span className="resource-glyph large"><AppIcon name="mcp" /></span>
        <div><span className="detail-kicker">MCP server</span><h2>{server.name}</h2><p>{server.description}</p></div>
        <StatusBadge state={server.state} />
      </div>
      <dl className="detail-grid">
        <div><dt>Transport</dt><dd>{server.transport.replaceAll("_", " ")}</dd></div>
        <div><dt>Health</dt><dd>{server.health}</dd></div>
        <div><dt>Trust</dt><dd>{server.trust.replaceAll("_", " ")}</dd></div>
        <div><dt>Authentication</dt><dd>{server.authState.replaceAll("_", " ")}</dd></div>
        <div><dt>Source</dt><dd>{server.source}</dd></div>
        <div><dt>Last checked</dt><dd>{server.lastChecked}</dd></div>
      </dl>
      <section className="mcp-endpoint">
        <span className="detail-kicker">Command or endpoint</span>
        <code>{server.commandOrUrl}</code>
      </section>
      <section className="mcp-capabilities">
        <div><span className="detail-kicker">Capabilities</span><div className="tag-list">{server.capabilities.map((capability) => <span key={capability}>{capability}</span>)}</div></div>
        <div><span className="detail-kicker">Global clients</span><div className="client-binding-list">{server.clients.map((binding) => <span className={`client-binding binding-${binding.state}`} key={binding.client}><strong>{binding.client}</strong><small>{binding.state}</small></span>)}</div></div>
      </section>
      <div className="detail-actions">
        <button className="primary-button" type="button" disabled={!server.primaryAction.enabled} onClick={onConfigure}><AppIcon name="settings" />{server.primaryAction.label}</button>
        <button className="secondary-button" type="button" disabled={!server.toggleAction.enabled} onClick={onToggle}><AppIcon name="run" />{server.toggleAction.label}</button>
        <button className="secondary-button" type="button" disabled={!server.removeAction.enabled} onClick={onRemove}><AppIcon name="failure" />{server.removeAction.label}</button>
      </div>
      <ActionDisabledReason reasonCode={server.primaryAction.disabledReasonCode ?? server.toggleAction.disabledReasonCode} />
      <div className="info-callout"><AppIcon name="info" /><div><strong>Credential boundary</strong><p>STM stores only credential references in this fixture. Secret values never appear in inventory, history, or exported diagnostics.</p></div></div>
    </section>
  );
}
