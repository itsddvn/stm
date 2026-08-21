import { useMemo, useState } from "react";
import type { AppViewModel, McpServerViewModel } from "../../../contracts/ui/view-model-contract";
import { AppIcon } from "../../components/app-icon";
import { EmptyState } from "../../components/empty-state";
import { LoadingTable } from "../../components/loading-table";
import { PageHeader } from "../../components/page-header";
import { SearchFilterBar } from "../../components/search-filter-bar";
import { SourceInstallDialog } from "../../components/source-install-dialog";
import { StateNotice } from "../../components/state-notice";
import { StatusBadge } from "../../components/status-badge";
import { McpDetailPanel } from "./mcp-detail-panel";

type McpDialogState =
  | { mode: "add" }
  | { mode: "configure" | "remove" | "toggle"; server: McpServerViewModel };

const addMcpAction = {
  id: "mcp.review_add",
  label: "Add MCP Server",
  enabled: true,
  presentationOnly: true,
} as const;

export function McpPage({ view }: { view: AppViewModel }) {
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState("all");
  const [selectedId, setSelectedId] = useState(view.mcpServers[0]?.id ?? "");
  const [dialog, setDialog] = useState<McpDialogState | null>(null);
  const filtered = useMemo(() => view.mcpServers.filter((server) => {
    const matchesFilter = filter === "all" || server.transport === filter || server.health === filter;
    const searchText = `${server.name} ${server.description} ${server.clients.map((binding) => binding.client).join(" ")} ${server.capabilities.join(" ")}`;
    return matchesFilter && searchText.toLowerCase().includes(query.toLowerCase());
  }), [filter, query, view.mcpServers]);
  const selected = filtered.find((server) => server.id === selectedId) ?? filtered[0];
  const dialogServer = dialog && dialog.mode !== "add" ? dialog.server : null;
  const initialUrl = dialogServer
    ? dialogServer.source.startsWith("https://") ? dialogServer.source : `https://github.com/${dialogServer.source}`
    : "";
  const dialogTitle = !dialog || dialog.mode === "add" ? undefined
    : dialog.mode === "configure" ? `Review ${dialog.server.name} Configuration`
      : dialog.mode === "toggle" ? `${dialog.server.toggleAction.label}: ${dialog.server.name}`
        : `Review ${dialog.server.name} Removal`;
  const dialogAction = !dialog ? undefined
    : dialog.mode === "add" ? addMcpAction
      : dialog.mode === "configure" ? dialog.server.primaryAction
        : dialog.mode === "toggle" ? dialog.server.toggleAction
          : dialog.server.removeAction;

  return (
    <>
      <PageHeader
        title="MCP Servers"
        description="Canonical servers, transports, global client bindings, capabilities, trust, and connection health."
        actions={<><button className="secondary-button" type="button"><AppIcon name="refresh" />Rescan Fixtures</button><button className="primary-button" type="button" onClick={() => setDialog({ mode: "add" })}><AppIcon name="add" />Add MCP Server</button></>}
      />
      <StateNotice reasonCode={view.surface.reasonCode} />
      <SearchFilterBar label="MCP servers" query={query} onQueryChange={setQuery} filter={filter} onFilterChange={setFilter} options={[{ value: "all", label: "All servers" }, { value: "stdio", label: "stdio" }, { value: "streamable_http", label: "Streamable HTTP" }, { value: "sse", label: "SSE" }, { value: "healthy", label: "Healthy" }, { value: "degraded", label: "Degraded" }]} />
      {view.surface.loadState === "loading" ? <LoadingTable /> : filtered.length === 0 ? <EmptyState title="No MCP servers match" detail="Clear filters, add a reviewed server, or choose another fixture scenario." /> : (
        <div className="master-detail-layout">
          <div className="resource-list" aria-label="MCP servers">
            {filtered.map((server) => (
              <button className={`resource-row ${server.id === selected?.id ? "selected" : ""}`} type="button" onClick={() => setSelectedId(server.id)} key={server.id}>
                <span className="resource-glyph"><AppIcon name="mcp" /></span>
                <span className="resource-summary"><strong>{server.name}</strong><small>{server.transport.replaceAll("_", " ")} · {server.source}</small></span>
                <span className="revision-cell"><small>Clients</small><strong>{server.clients.filter((binding) => binding.state === "enabled").length}/{server.clients.length}</strong></span>
                <StatusBadge state={server.state} />
              </button>
            ))}
          </div>
          {selected ? <McpDetailPanel server={selected} onConfigure={() => setDialog({ mode: "configure", server: selected })} onToggle={() => setDialog({ mode: "toggle", server: selected })} onRemove={() => setDialog({ mode: "remove", server: selected })} /> : null}
        </div>
      )}
      {dialog ? <SourceInstallDialog kind="mcp" open onClose={() => setDialog(null)} initialUrl={initialUrl} title={dialogTitle} mcpAction={dialogAction} mcpServerId={dialogServer?.id} /> : null}
    </>
  );
}
