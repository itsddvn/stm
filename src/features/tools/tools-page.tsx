import { useMemo, useState } from "react";
import type { AppViewModel, ToolViewModel } from "../../../contracts/ui/view-model-contract";
import { AppIcon } from "../../components/app-icon";
import { EmptyState } from "../../components/empty-state";
import { LoadingTable } from "../../components/loading-table";
import { OwnershipRail } from "../../components/ownership-rail";
import { PageHeader } from "../../components/page-header";
import { SearchFilterBar } from "../../components/search-filter-bar";
import { SourceInstallDialog } from "../../components/source-install-dialog";
import { StateNotice } from "../../components/state-notice";
import { StatusBadge } from "../../components/status-badge";
import { ToolDetailPanel } from "./tool-detail-panel";
import { ToolOperationDialog } from "./tool-operation-dialog";

export function ToolsPage({ view }: { view: AppViewModel }) {
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState("all");
  const [selectedId, setSelectedId] = useState(view.tools[0]?.id ?? "");
  const [dialogTool, setDialogTool] = useState<ToolViewModel | null>(null);
  const [sourceDialogOpen, setSourceDialogOpen] = useState(false);
  const filtered = useMemo(() => view.tools.filter((tool) => (filter === "all" || tool.executionMode === filter) && `${tool.name} ${tool.groups.join(" ")}`.toLowerCase().includes(query.toLowerCase())), [filter, query, view.tools]);
  const selected = filtered.find((tool) => tool.id === selectedId) ?? filtered[0];

  return (
    <>
      <PageHeader title="Tools Inventory" description="Canonical tools, detected ownership, and mapping-level lifecycle readiness." actions={<><button className="secondary-button" type="button"><AppIcon name="refresh" />Refresh Fixture</button><button className="primary-button" type="button" onClick={() => setSourceDialogOpen(true)}><AppIcon name="link" />Install from Link</button></>} />
      <StateNotice reasonCode={view.surface.reasonCode} />
      <SearchFilterBar label="tools" query={query} onQueryChange={setQuery} filter={filter} onFilterChange={setFilter} options={[{ value: "all", label: "All execution modes" }, { value: "managed_execute", label: "Managed execute" }, { value: "vendor_handoff", label: "Vendor handoff" }, { value: "detect_only", label: "Detect only" }]} />
      {view.surface.loadState === "loading" ? <LoadingTable /> : filtered.length === 0 ? <EmptyState title="No tools match" detail="Clear filters or choose another fixture scenario." /> : (
        <div className="master-detail-layout">
          <div className="resource-list" aria-label="Tools">
            {filtered.map((tool) => <button className={`resource-row ${tool.id === selected?.id ? "selected" : ""}`} type="button" onClick={() => setSelectedId(tool.id)} key={tool.id}><span className="resource-glyph"><AppIcon name={tool.kind === "CLI tool" ? "terminal" : "package"} /></span><span className="resource-summary"><strong>{tool.name}</strong><small>{tool.kind} · {tool.groups[0]}</small></span><OwnershipRail owner={tool.owner} mode={tool.executionMode} compact /><StatusBadge state={tool.state} /></button>)}
          </div>
          {selected ? <ToolDetailPanel tool={selected} onPreview={() => setDialogTool(selected)} /> : null}
        </div>
      )}
      {dialogTool ? <ToolOperationDialog tool={dialogTool} open onClose={() => setDialogTool(null)} /> : null}
      {sourceDialogOpen ? <SourceInstallDialog kind="tool" open onClose={() => setSourceDialogOpen(false)} /> : null}
    </>
  );
}
