import { useEffect, useState } from "react";
import type { InstallProviderPreference, MigrationCandidate } from "../../../contracts/ui/setup-contract";
import { loadSetupPreference } from "../../fixtures/setup-fixtures";
import { runtimeIpcClient } from "../../lib/ipc/runtime-ipc-client";
import type { AppViewModel } from "../../../contracts/ui/view-model-contract";
import { AppIcon } from "../../components/app-icon";
import { PageHeader } from "../../components/page-header";
import { StateNotice } from "../../components/state-notice";
import { MigrationReviewDialog } from "./migration-review-dialog";
import { useI18n } from "../../lib/i18n";

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
  const { t } = useI18n();
  const [autoCheck, setAutoCheck] = useState(true);
  const [diagnostics, setDiagnostics] = useState(false);
  const [preference, setPreference] = useState<InstallProviderPreference>(loadSetupPreference);
  const [portableTarget, setPortableTarget] = useState("");
  const [portableStatus, setPortableStatus] = useState<string>();
  const [migrationCandidates, setMigrationCandidates] = useState<MigrationCandidate[]>([]);
  const [migrationCandidate, setMigrationCandidate] = useState<MigrationCandidate>();
  useEffect(() => {
    void Promise.all([
      runtimeIpcClient.getQuickSetup(view.tools),
      runtimeIpcClient.getMigrationCandidates(),
    ]).then(([setup, candidates]) => {
      setPreference(setup.preference);
      setPortableTarget((current) => current || setup.target);
      setMigrationCandidates(candidates);
    });
  }, [view.tools]);

  async function importPortable() {
    try {
      const result = await runtimeIpcClient.importPortableSetup();
      if (!result) {
        setPortableStatus(runtimeIpcClient.isDesktop() ? undefined : "Native import is available in the desktop app.");
        return;
      }
      setPortableStatus(result.reviewRequiredIds.length
        ? `Imported ${result.document.resources.length} resources. Review required: ${result.reviewRequiredIds.join(", ")}.`
        : `Imported ${result.document.resources.length} resources. Open Quick Setup to review the additive selection.`);
      window.dispatchEvent(new CustomEvent("stm:open-quick-setup", {
        detail: { importedResources: result.document.resources },
      }));
    } catch (error) {
      setPortableStatus(error instanceof Error ? error.message : "Import failed");
    }
  }

  async function exportPortable() {
    try {
      const fileName = await runtimeIpcClient.exportPortableSetup(portableTarget);
      setPortableStatus(fileName
        ? `Exported ${fileName}. The target resolves latest compatible versions.`
        : runtimeIpcClient.isDesktop()
          ? undefined
          : "Native export is available in the desktop app.");
    } catch (error) {
      setPortableStatus(error instanceof Error ? error.message : "Export failed");
    }
  }
  return (
    <>
      <PageHeader title={t("page.settings.title")} description={t("page.settings.description")} />
      <StateNotice reasonCode={view.surface.reasonCode} />
      <div className="settings-layout">
        <section className="settings-section">
          <div className="section-heading"><div><h2>{t("setup.provider")}</h2><p>{t("setup.automaticDetail")}</p></div><button className="primary-button" type="button" onClick={() => window.dispatchEvent(new Event("stm:open-quick-setup"))}>{t("common.quickSetup")}</button></div>
          <label className="setting-toggle"><span><strong>{t("setup.automatic")}</strong><small>{t("setup.automaticDetail")}</small></span><input type="radio" name="provider" checked={preference === "automatic"} onChange={() => { setPreference("automatic"); void runtimeIpcClient.setProviderPreference("automatic"); }} /></label>
          <label className="setting-toggle"><span><strong>{t("setup.homebrew")}</strong><small>{t("setup.homebrewDetail")}</small></span><input type="radio" name="provider" checked={preference === "prefer_homebrew"} onChange={() => { setPreference("prefer_homebrew"); void runtimeIpcClient.setProviderPreference("prefer_homebrew"); }} /></label>
          <label className="setting-toggle"><span><strong>{t("setup.bun")}</strong><small>{t("setup.bunDetail")}</small></span><input type="radio" name="provider" checked={preference === "prefer_bun"} onChange={() => { setPreference("prefer_bun"); void runtimeIpcClient.setProviderPreference("prefer_bun"); }} /></label>
        </section>
        <section className="settings-section">
          <div className="section-heading"><div><h2>{t("settings.migration")}</h2><p>{t("settings.migrationDetail")}</p></div></div>
          {migrationCandidates.length ? migrationCandidates.map((candidate) => <div className="adapter-row" key={candidate.recipe.id}><div><strong>{candidate.recipe.resourceId}</strong><small>{candidate.sourceOwner} → {candidate.targetOwner}</small></div><button className="secondary-button" type="button" onClick={() => setMigrationCandidate(candidate)}>{t("settings.reviewMigration")}</button></div>) : <p>{t("settings.noMigration")}</p>}
        </section>
        <section className="settings-section">
          <div className="section-heading"><div><h2>{t("settings.portable")}</h2><p>{t("settings.portableDetail")}</p></div></div>
          <label className="setting-toggle"><span><strong>Target</strong></span><select value={portableTarget} onChange={(event) => setPortableTarget(event.target.value)}><option value="" disabled>—</option><option value="macos_arm64">macOS arm64</option><option value="macos_x64">macOS x64</option><option value="windows_x64">Windows x64</option><option value="linux_x64">Linux x64</option></select></label>
          <div className="detail-actions"><button className="secondary-button" type="button" onClick={() => void importPortable()}>{t("settings.import")}</button><button className="primary-button" type="button" disabled={!portableTarget} onClick={() => void exportPortable()}>{t("settings.export")}</button></div>
          {portableStatus ? <p>{portableStatus}</p> : null}
        </section>
        <details className="advanced-details settings-advanced">
          <summary>{t("settings.advanced")}</summary>
        <section className="settings-section"><div className="section-heading"><div><h2>Inventory Adapters</h2><p>Availability does not grant lifecycle permission.</p></div></div><div className="adapter-list">{adapters.map((adapter) => <div className="adapter-row" key={adapter.name}><span className="resource-glyph"><AppIcon name="manager" /></span><div><strong>{adapter.name}</strong><small>{adapter.scope}</small></div><span className={adapter.state === "Available" ? "availability-ready" : "availability-muted"}>{adapter.state}</span></div>)}</div></section>
        <section className="settings-section"><div className="section-heading"><div><h2>Global Skill Roots</h2><p>Logical clients preserve one deduplicated physical target.</p></div><button className="secondary-button" type="button">Review Roots</button></div><div className="root-table"><div className="table-header"><span>Client</span><span>Configured root</span><span>Physical target</span></div>{roots.map((root) => <div className="root-row" key={root.client}><strong>{root.client}</strong><span className="mono-data">{root.path}</span><span className="mono-data">{root.physical}</span></div>)}</div><div className="info-callout"><AppIcon name="info" /><p>Codex and AgentKit resolve to the same physical fixture root. The scanner reads it once and retains both client bindings.</p></div></section>
        <section className="settings-section"><div className="section-heading"><div><h2>Update Behavior</h2><p>Metadata checks run only while the app is active.</p></div></div><label className="setting-toggle"><span><strong>Check metadata while active</strong><small>Never applies updates without plan review and consent.</small></span><input type="checkbox" role="switch" checked={autoCheck} onChange={(event) => setAutoCheck(event.target.checked)} /></label><label className="setting-toggle"><span><strong>Share fixture diagnostics</strong><small>Presentation state only. No machine data leaves the prototype.</small></span><input type="checkbox" role="switch" checked={diagnostics} onChange={(event) => setDiagnostics(event.target.checked)} /></label><div className="field-row"><label htmlFor="catalog-channel">Catalog channel</label><select id="catalog-channel" name="catalog-channel" defaultValue="stable"><option value="stable">Stable verified catalog</option><option value="review">Review fixture catalog</option></select></div></section>
        <section className="settings-section diagnostic-section"><div><span className="detail-kicker">Diagnostics</span><h2>Fixture Contract State</h2><p>Review manifest present. Project-lead approval and locked contract intentionally absent.</p></div><dl><div><dt>Contract</dt><dd className="mono-data">1.0.0-draft</dd></div><div><dt>Status</dt><dd>Review</dd></div><div><dt>Backend</dt><dd>Not connected</dd></div></dl><button className="secondary-button" type="button">Copy Fixture Summary</button></section>
        </details>
      </div>
      {migrationCandidate ? <MigrationReviewDialog candidate={migrationCandidate} open onClose={() => setMigrationCandidate(undefined)} /> : null}
    </>
  );
}
