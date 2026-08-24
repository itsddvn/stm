import type { AppViewModel } from "../../../contracts/ui/view-model-contract";
import { AppIcon } from "../../components/app-icon";
import { EmptyState } from "../../components/empty-state";
import { LoadingTable } from "../../components/loading-table";
import { OwnershipRail } from "../../components/ownership-rail";
import { PageHeader } from "../../components/page-header";
import { StateNotice } from "../../components/state-notice";
import { useI18n, type Translator } from "../../lib/i18n";

export function DashboardPage({ view }: { view: AppViewModel }) {
  const { locale, t } = useI18n();
  const updateCount = view.updates.filter((item) => item.resourceType !== "product").length;
  const modifiedCount = view.skills.filter((skill) => ["modified", "conflict"].includes(skill.state)).length;

  return (
    <>
      <PageHeader title={t("page.dashboard.title")} description={t("page.dashboard.description")} actions={
        <>
          <button className="primary-button" type="button" onClick={() => window.dispatchEvent(new Event("stm:open-quick-setup"))}>{t("common.quickSetup")}</button>
          <button className="secondary-button" type="button" data-runtime-action="refresh"><AppIcon name="refresh" />{t("common.refresh")}</button>
        </>
      } />
      <StateNotice reasonCode={view.surface.reasonCode} />
      {view.surface.loadState === "loading" ? <LoadingTable /> : view.surface.loadState === "empty" ? <EmptyState title="No fixture inventory" detail="Choose Success or Partial from the scenario switcher." /> : (
        <div className="dashboard-layout">
          <section className="metric-strip" aria-label={t("dashboard.knownTools")}>
            <Metric label={t("dashboard.knownTools")} value={view.tools.length} detail={t("dashboard.catalog")} />
            <Metric label={t("dashboard.mcp")} value={view.mcpServers.length} detail={t("dashboard.bindings")} />
            <Metric label={t("dashboard.updates")} value={updateCount} detail={t("dashboard.noneSelected")} />
            <Metric label={t("dashboard.conflicts")} value={modifiedCount} detail={t("dashboard.reviewRequired")} />
          </section>
          <section className="dashboard-primary">
            <div className="section-heading"><div><h2>{t("dashboard.authority")}</h2><p>{t("dashboard.authorityDetail")}</p></div><a href="#/tools">{t("dashboard.inspectTools")}</a></div>
            <div className="authority-list">
              {view.tools.slice(0, 5).map((tool) => (
                <div className="authority-row" key={tool.id}>
                  <div className="resource-name"><span className="resource-glyph"><AppIcon name={tool.kind === "CLI tool" ? "terminal" : "package"} /></span><span><strong>{tool.name}</strong><small>{localizedToolState(tool.state, t)}</small></span></div>
                  <OwnershipRail owner={tool.owner} mode={tool.executionMode} compact />
                  <span className="mono-data">{tool.installedVersion === "Detected" ? t("action.installed") : tool.installedVersion ?? t("dashboard.notInstalled")}</span>
                </div>
              ))}
            </div>
          </section>
          <aside className="dashboard-secondary">
            <div className="section-heading"><div><h2>{t("dashboard.recent")}</h2><p>{t("dashboard.recentDetail")}</p></div></div>
            <ol className="activity-list">
              {view.operations.slice(0, 4).map((operation) => (
                <li key={operation.id}><span className={`activity-mark mark-${operation.status}`}><AppIcon name={operation.status === "success" ? "success" : operation.status === "failed" ? "failure" : "warning"} size={16} /></span><div><strong>{operation.resource}</strong><small>{formatTime(operation.startedAt, locale)}</small></div></li>
              ))}
            </ol>
            <a className="text-link" href="#/history">{t("dashboard.openHistory")}</a>
          </aside>
        </div>
      )}
    </>
  );
}

function Metric({ label, value, detail }: { label: string; value: number; detail: string }) {
  return <div className="metric"><span>{label}</span><strong>{value}</strong><small>{detail}</small></div>;
}

function formatTime(value: string, locale: "vi" | "en") {
  return new Intl.DateTimeFormat(locale === "vi" ? "vi-VN" : "en", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }).format(new Date(value));
}

function localizedToolState(state: string, t: Translator) {
  const keys = {
    managed_current: "state.managed_current",
    managed_update_available: "state.managed_update_available",
    missing: "state.missing",
    external: "state.external",
    blocked: "state.blocked",
    unknown: "state.unknown",
  } as const;
  return state in keys ? t(keys[state as keyof typeof keys]) : state.replaceAll("_", " ");
}
