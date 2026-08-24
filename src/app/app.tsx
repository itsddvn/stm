import { useEffect, useState } from "react";
import type { ScenarioId } from "../../contracts/ui/state-contract";
import type { PortableSetupDocument } from "../../contracts/ui/setup-contract";
import { DashboardPage } from "../features/dashboard/dashboard-page";
import { HistoryPage } from "../features/history/history-page";
import { McpPage } from "../features/mcp/mcp-page";
import { SettingsPage } from "../features/settings/settings-page";
import { SkillsPage } from "../features/skills/skills-page";
import { ToolsPage } from "../features/tools/tools-page";
import { UpdatesPage } from "../features/updates/updates-page";
import { AppShell } from "./app-shell";
import { useFixtureView } from "./use-fixture-view";
import { useHashRoute } from "./use-hash-route";
import { QuickSetupDialog } from "../features/setup/quick-setup-dialog";
import { runtimeIpcClient } from "../lib/ipc/runtime-ipc-client";

export function App() {
  const [scenario, setScenario] = useState<ScenarioId>("success");
  const { routeId } = useHashRoute();
  const view = useFixtureView(scenario);
  const [quickSetupOpen, setQuickSetupOpen] = useState(false);
  const [checkedLaunch, setCheckedLaunch] = useState(false);
  const [quickSetupImportedResources, setQuickSetupImportedResources] = useState<PortableSetupDocument["resources"]>([]);

  useEffect(() => {
    const open = (event: Event) => {
      const detail = event instanceof CustomEvent
        ? event.detail as { importedResources?: PortableSetupDocument["resources"] } | undefined
        : undefined;
      setQuickSetupImportedResources(detail?.importedResources ?? []);
      setQuickSetupOpen(true);
    };
    window.addEventListener("stm:open-quick-setup", open);
    return () => window.removeEventListener("stm:open-quick-setup", open);
  }, []);
  useEffect(() => {
    if (!view || checkedLaunch) return;
    void runtimeIpcClient.getSetupPreferences().then((prefs) => {
      setQuickSetupOpen(!prefs.quickSetupDismissed);
      setCheckedLaunch(true);
    });
  }, [view, checkedLaunch]);
  useEffect(() => {
    document.querySelector<HTMLElement>(".page-header h1")?.focus();
  }, [routeId]);

  if (!view) return <div className="app-boot">Loading runtime boundary…</div>;

  const page = {
    dashboard: <DashboardPage view={view} />,
    tools: <ToolsPage view={view} />,
    skills: <SkillsPage view={view} />,
    mcp: <McpPage view={view} />,
    updates: <UpdatesPage view={view} />,
    history: <HistoryPage view={view} />,
    settings: <SettingsPage view={view} />,
  }[routeId];

  return (
    <AppShell routeId={routeId} scenario={scenario} onScenarioChange={setScenario}>
      {page}
      <QuickSetupDialog view={view} importedResources={quickSetupImportedResources} open={quickSetupOpen} onClose={() => { setQuickSetupImportedResources([]); setQuickSetupOpen(false); }} />
    </AppShell>
  );
}
