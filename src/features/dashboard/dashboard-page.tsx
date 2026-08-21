import type { AppViewModel } from "../../../contracts/ui/view-model-contract";
import { AppIcon } from "../../components/app-icon";
import { EmptyState } from "../../components/empty-state";
import { LoadingTable } from "../../components/loading-table";
import { OwnershipRail } from "../../components/ownership-rail";
import { PageHeader } from "../../components/page-header";
import { StateNotice } from "../../components/state-notice";

export function DashboardPage({ view }: { view: AppViewModel }) {
  const updateCount = view.updates.filter((item) => item.resourceType !== "product").length;
  const modifiedCount = view.skills.filter((skill) => ["modified", "conflict"].includes(skill.state)).length;

  return (
    <>
      <PageHeader title="Operations Overview" description="Local tools, skills, MCP servers, authority, and update readiness from deterministic fixture outputs." actions={
        <button className="secondary-button" type="button"><AppIcon name="refresh" />Refresh Fixture</button>
      } />
      <StateNotice reasonCode={view.surface.reasonCode} />
      {view.surface.loadState === "loading" ? <LoadingTable /> : view.surface.loadState === "empty" ? <EmptyState title="No fixture inventory" detail="Choose Success or Partial from the scenario switcher." /> : (
        <div className="dashboard-layout">
          <section className="metric-strip" aria-label="Inventory summary">
            <Metric label="Known tools" value={view.tools.length} detail="Recommended catalog" />
            <Metric label="MCP servers" value={view.mcpServers.length} detail="Global client bindings" />
            <Metric label="Available updates" value={updateCount} detail="Nothing selected" />
            <Metric label="Skill conflicts" value={modifiedCount} detail="Review required" />
          </section>
          <section className="dashboard-primary">
            <div className="section-heading"><div><h2>Authority Map</h2><p>Every lifecycle path remains attached to its detected owner.</p></div><a href="#/tools">Inspect all tools</a></div>
            <div className="authority-list">
              {view.tools.slice(0, 5).map((tool) => (
                <div className="authority-row" key={tool.id}>
                  <div className="resource-name"><span className="resource-glyph"><AppIcon name={tool.kind === "CLI tool" ? "terminal" : "package"} /></span><span><strong>{tool.name}</strong><small>{tool.state.replaceAll("_", " ")}</small></span></div>
                  <OwnershipRail owner={tool.owner} mode={tool.executionMode} compact />
                  <span className="mono-data">{tool.installedVersion ?? "not installed"}</span>
                </div>
              ))}
            </div>
          </section>
          <aside className="dashboard-secondary">
            <div className="section-heading"><div><h2>Recent Activity</h2><p>Fixture history only</p></div></div>
            <ol className="activity-list">
              {view.operations.slice(0, 4).map((operation) => (
                <li key={operation.id}><span className={`activity-mark mark-${operation.status}`}><AppIcon name={operation.status === "success" ? "success" : operation.status === "failed" ? "failure" : "warning"} size={16} /></span><div><strong>{operation.resource}</strong><p>{operation.detail}</p><small>{formatTime(operation.startedAt)}</small></div></li>
              ))}
            </ol>
            <a className="text-link" href="#/history">Open operation history</a>
          </aside>
        </div>
      )}
    </>
  );
}

function Metric({ label, value, detail }: { label: string; value: number; detail: string }) {
  return <div className="metric"><span>{label}</span><strong>{value}</strong><small>{detail}</small></div>;
}

function formatTime(value: string) {
  return new Intl.DateTimeFormat("en", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }).format(new Date(value));
}
