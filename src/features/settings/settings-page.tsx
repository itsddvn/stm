import { useState } from "react";
import type { AppViewModel } from "../../../contracts/ui/view-model-contract";
import { AppIcon } from "../../components/app-icon";
import { PageHeader } from "../../components/page-header";
import { StateNotice } from "../../components/state-notice";

const adapters = [
  { name: "Homebrew", scope: "Tool inventory and reviewed mappings", state: "Available" },
  { name: "npm", scope: "User-scoped CLI ownership", state: "Available" },
  { name: "Vendor updaters", scope: "Supported handoff boundaries", state: "Available" },
  { name: "MCP client configs", scope: "Codex, Claude Code, and Cursor global servers", state: "Fixture only" },
  { name: "WinGet", scope: "Windows package mappings", state: "Fixture unavailable" },
  { name: "Linux native manager", scope: "APT, DNF, or Pacman", state: "Fixture unavailable" },
];

const roots = [
  { client: "Codex", path: "$CODEX_HOME/skills", physical: "physical-root:01" },
  { client: "Claude Code", path: "$CLAUDE_HOME/skills", physical: "physical-root:02" },
  { client: "AgentKit", path: "$AGENTKIT_HOME/skills", physical: "physical-root:01" },
];

export function SettingsPage({ view }: { view: AppViewModel }) {
  const [autoCheck, setAutoCheck] = useState(true);
  const [diagnostics, setDiagnostics] = useState(false);
  return (
    <>
      <PageHeader title="Settings" description="Fixture adapters, approved global roots, MCP clients, metadata behavior, and diagnostics." />
      <StateNotice reasonCode={view.surface.reasonCode} />
      <div className="settings-layout">
        <section className="settings-section"><div className="section-heading"><div><h2>Inventory Adapters</h2><p>Availability does not grant lifecycle permission.</p></div></div><div className="adapter-list">{adapters.map((adapter) => <div className="adapter-row" key={adapter.name}><span className="resource-glyph"><AppIcon name="manager" /></span><div><strong>{adapter.name}</strong><small>{adapter.scope}</small></div><span className={adapter.state === "Available" ? "availability-ready" : "availability-muted"}>{adapter.state}</span></div>)}</div></section>
        <section className="settings-section"><div className="section-heading"><div><h2>Global Skill Roots</h2><p>Logical clients preserve one deduplicated physical target.</p></div><button className="secondary-button" type="button">Review Roots</button></div><div className="root-table"><div className="table-header"><span>Client</span><span>Configured root</span><span>Physical target</span></div>{roots.map((root) => <div className="root-row" key={root.client}><strong>{root.client}</strong><span className="mono-data">{root.path}</span><span className="mono-data">{root.physical}</span></div>)}</div><div className="info-callout"><AppIcon name="info" /><p>Codex and AgentKit resolve to the same physical fixture root. The scanner reads it once and retains both client bindings.</p></div></section>
        <section className="settings-section"><div className="section-heading"><div><h2>Update Behavior</h2><p>Metadata checks run only while the app is active.</p></div></div><label className="setting-toggle"><span><strong>Check metadata while active</strong><small>Never applies updates without plan review and consent.</small></span><input type="checkbox" role="switch" checked={autoCheck} onChange={(event) => setAutoCheck(event.target.checked)} /></label><label className="setting-toggle"><span><strong>Share fixture diagnostics</strong><small>Presentation state only. No machine data leaves the prototype.</small></span><input type="checkbox" role="switch" checked={diagnostics} onChange={(event) => setDiagnostics(event.target.checked)} /></label><div className="field-row"><label htmlFor="catalog-channel">Catalog channel</label><select id="catalog-channel" name="catalog-channel" defaultValue="stable"><option value="stable">Stable verified catalog</option><option value="review">Review fixture catalog</option></select></div></section>
        <section className="settings-section diagnostic-section"><div><span className="detail-kicker">Diagnostics</span><h2>Fixture Contract State</h2><p>Review manifest present. Project-lead approval and locked contract intentionally absent.</p></div><dl><div><dt>Contract</dt><dd className="mono-data">1.0.0-draft</dd></div><div><dt>Status</dt><dd>Review</dd></div><div><dt>Backend</dt><dd>Not connected</dd></div></dl><button className="secondary-button" type="button">Copy Fixture Summary</button></section>
      </div>
    </>
  );
}
