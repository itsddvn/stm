import type { ReactNode } from "react";
import { routes, type RouteId } from "../../contracts/ui/route-contract";
import { scenarioIds, type ScenarioId } from "../../contracts/ui/state-contract";
import { AppIcon } from "../components/app-icon";
import { scenarioLabels } from "../lib/copy";
import { useI18n, type MessageKey } from "../lib/i18n";
import { isFixtureRuntime } from "../lib/ipc/runtime-ipc-client";

interface AppShellProps {
  routeId: RouteId;
  scenario: ScenarioId;
  onScenarioChange: (scenario: ScenarioId) => void;
  children: ReactNode;
}

const navKeys: Record<RouteId, MessageKey> = {
  dashboard: "nav.dashboard",
  tools: "nav.tools",
  skills: "nav.skills",
  mcp: "nav.mcp",
  updates: "nav.updates",
  history: "nav.history",
  settings: "nav.settings",
};

export function activateSkipLink(
  event: { preventDefault: () => void },
  doc: { getElementById: (id: string) => Pick<HTMLElement, "focus"> | null } = document,
) {
  event.preventDefault();
  doc.getElementById("main-content")?.focus();
}

export function AppShell({ routeId, scenario, onScenarioChange, children }: AppShellProps) {
  const { locale, setLocale, t } = useI18n();
  return (
    <div className="app-shell">
      <a className="skip-link" href="#main-content" onClick={activateSkipLink}>{t("nav.skip")}</a>
      <aside className="sidebar" aria-label={t("nav.primary")}>
        <div className="brand-lockup">
          <span className="brand-mark"><AppIcon name="terminal" size={22} weight="bold" /></span>
          <span><strong>STM</strong><small>Smart Tools Management</small></span>
        </div>
        <nav>
          {routes.map((route) => (
            <a className={route.id === routeId ? "active" : ""} href={`#${route.path}`} aria-current={route.id === routeId ? "page" : undefined} key={route.id}>
              <AppIcon name={route.id} size={19} weight={route.id === routeId ? "fill" : "regular"} />
              <span>{t(navKeys[route.id])}</span>
            </a>
          ))}
        </nav>
        <div className="sidebar-status">
          <span className="status-indicator" aria-hidden="true" />
          <div><strong>{isFixtureRuntime() ? t("runtime.fixture") : t("runtime.desktop")}</strong><small>{t("runtime.ready")}</small></div>
        </div>
      </aside>
      <div className="workspace">
        <div className="utility-bar">
          <span className="machine-context">{t("runtime.machine")} <strong>macOS · arm64</strong></span>
          {isFixtureRuntime() ? (
            <label className="scenario-switcher">
              <span>{t("runtime.scenario")}</span>
              <select value={scenario} onChange={(event) => onScenarioChange(event.target.value as ScenarioId)}>
                {scenarioIds.map((id) => <option value={id} key={id}>{scenarioLabels[id]}</option>)}
              </select>
            </label>
          ) : null}
          <label className="scenario-switcher">
            <span>{t("language.label")}</span>
            <select value={locale} onChange={(event) => setLocale(event.target.value as "vi" | "en")}>
              <option value="vi">{t("language.vi")}</option>
              <option value="en">{t("language.en")}</option>
            </select>
          </label>
        </div>
        <main id="main-content" className="main-content" tabIndex={-1}>{children}</main>
      </div>
    </div>
  );
}
