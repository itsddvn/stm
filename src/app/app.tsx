import { useEffect, useState } from "react";
import type { ScenarioId } from "../../contracts/ui/state-contract";
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

export function App() {
  const [scenario, setScenario] = useState<ScenarioId>("success");
  const { routeId } = useHashRoute();
  const view = useFixtureView(scenario);

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

  return <AppShell routeId={routeId} scenario={scenario} onScenarioChange={setScenario}>{page}</AppShell>;
}
