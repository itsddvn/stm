import type { ReactNode } from "react";
import { routes, type RouteId } from "../../contracts/ui/route-contract";
import { scenarioIds, type ScenarioId } from "../../contracts/ui/state-contract";
import { AppIcon } from "../components/app-icon";
import { navCopy, scenarioLabels } from "../lib/copy";
import { isFixtureRuntime } from "../lib/ipc/runtime-ipc-client";

interface AppShellProps {
  routeId: RouteId;
  scenario: ScenarioId;
  onScenarioChange: (scenario: ScenarioId) => void;
  children: ReactNode;
}

export function activateSkipLink(
  event: { preventDefault: () => void },
  doc: { getElementById: (id: string) => Pick<HTMLElement, "focus"> | null } = document,
) {
  event.preventDefault();
  doc.getElementById("main-content")?.focus();
}

export function AppShell({ routeId, scenario, onScenarioChange, children }: AppShellProps) {
  return (
    <div className="app-shell">
      <a className="skip-link" href="#main-content" onClick={activateSkipLink}>Skip to main content</a>
      <aside className="sidebar" aria-label="Primary navigation">
        <div className="brand-lockup">
          <span className="brand-mark"><AppIcon name="terminal" size={22} weight="bold" /></span>
          <span><strong>STM</strong><small>Smart Tools Management</small></span>
        </div>
        <nav>
          {routes.map((route) => (
            <a className={route.id === routeId ? "active" : ""} href={`#${route.path}`} aria-current={route.id === routeId ? "page" : undefined} key={route.id}>
              <AppIcon name={route.id} size={19} weight={route.id === routeId ? "fill" : "regular"} />
              <span>{navCopy[route.id]}</span>
            </a>
          ))}
        </nav>
        <div className="sidebar-status">
          <span className="status-indicator" aria-hidden="true" />
          <div><strong>{isFixtureRuntime() ? "Deterministic simulation" : "Desktop runtime"}</strong><small>{isFixtureRuntime() ? "Desktop adapter-ready" : "Typed lifecycle adapter"}</small></div>
        </div>
      </aside>
      <div className="workspace">
        <div className="utility-bar">
          <span className="machine-context">Review machine <strong>macOS · arm64</strong></span>
          <label className="scenario-switcher">
            <span>Scenario</span>
            <select value={scenario} onChange={(event) => onScenarioChange(event.target.value as ScenarioId)}>
              {scenarioIds.map((id) => <option value={id} key={id}>{scenarioLabels[id]}</option>)}
            </select>
          </label>
        </div>
        <main id="main-content" className="main-content" tabIndex={-1}>{children}</main>
      </div>
    </div>
  );
}
