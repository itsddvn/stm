import { useEffect, useMemo, useState } from "react";
import type { AppViewModel } from "../../../contracts/ui/view-model-contract";
import type { InstallProviderPreference, PortableSetupDocument, QuickSetupView, SetupRowAction, SetupRowView } from "../../../contracts/ui/setup-contract";
import { FixtureDialog } from "../../components/fixture-dialog";
import { LifecycleExecutionState } from "../../components/lifecycle-execution-state";
import { LifecyclePlanReview } from "../../components/lifecycle-plan-review";
import { useLifecycleOperation } from "../../components/use-lifecycle-operation";
import { useI18n, type MessageKey, type Translator } from "../../lib/i18n";
import { runtimeIpcClient } from "../../lib/ipc/runtime-ipc-client";


const actionKeys: Record<SetupRowAction, MessageKey> = {
  install: "action.install",
  update: "action.update",
  installed: "action.installed",
  handoff: "action.handoff",
  guidance: "action.guidance",
  blocked: "action.blocked",
};
const EMPTY_IMPORTED_RESOURCES: PortableSetupDocument["resources"] = [];

type ImportedResourceSelection = PortableSetupDocument["resources"][number] & { selected: boolean };

function desiredAction(action: SetupRowAction) {
  if (action === "update" || action === "handoff") return "update";
  if (action === "guidance") return "review";
  return "install";
}

export function QuickSetupDialog({ view, importedResources = EMPTY_IMPORTED_RESOURCES, open, onClose }: { view: AppViewModel; importedResources?: PortableSetupDocument["resources"]; open: boolean; onClose: () => void }) {
  const { t } = useI18n();
  const [setup, setSetup] = useState<QuickSetupView>();
  const [step, setStep] = useState<"source" | "provider" | "select" | "review">("source");
  const [preference, setPreference] = useState<InstallProviderPreference>("automatic");
  const [rows, setRows] = useState<SetupRowView[]>([]);
  const [importedReviewRows, setImportedReviewRows] = useState<ImportedResourceSelection[]>([]);
  const [importError, setImportError] = useState<string>();
  const selected = useMemo(
    () => rows.filter((row) => row.selected && row.action !== "installed" && row.action !== "blocked"),
    [rows],
  );
  const selectedImported = useMemo(
    () => importedReviewRows.filter((resource) => resource.selected),
    [importedReviewRows],
  );
  const selectedCount = selected.length + selectedImported.length;
  const request = useMemo(() => ({
    resourceKind: "operation" as const,
    action: "setup-queue",
    resourceId: "quick-setup",
    itemIds: selected.map((row) => row.id),
    children: [
      ...selected.map((row) => ({
        resourceKind: "tool" as const,
        resourceId: row.id,
        desiredAction: desiredAction(row.action),
        mappingId: row.mappingId,
      })),
      ...selectedImported.map((resource) => ({
        resourceKind: resource.kind as "tool" | "skill" | "mcp",
        resourceId: resource.id,
        desiredAction: "review",
      })),
    ],
  }), [selected, selectedImported]);
  const lifecycle = useLifecycleOperation(step === "review" ? request : null, open && step === "review");

  useEffect(() => {
    if (!open) return;
    void runtimeIpcClient.getQuickSetup(view.tools).then((next) => {
      const nextRows = [...next.tools, ...next.optional];
      const knownToolIds = new Set(nextRows.map((row) => row.id));
      const importedToolIds = new Set(
        importedResources.filter((resource) => resource.kind === "tool").map((resource) => resource.id),
      );
      setSetup(next);
      setPreference(next.preference);
      setRows(nextRows.map((row) => ({
        ...row,
        selected: row.selected || importedToolIds.has(row.id),
      })));
      setImportedReviewRows(importedResources
        .filter((resource) => resource.kind !== "tool" || !knownToolIds.has(resource.id))
        .map((resource) => ({ ...resource, selected: true })));
      setStep(next.dismissed ? "select" : "source");
      setImportError(undefined);
    }).catch((error: unknown) => {
      setImportError(error instanceof Error ? error.message : String(error));
    });
  }, [open, view.tools, importedResources]);
  async function saveProviderAndContinue() {
    try {
      await runtimeIpcClient.setProviderPreference(preference);
      const next = await runtimeIpcClient.getQuickSetup(view.tools);
      setSetup(next);
      setPreference(next.preference);
      setRows([...next.tools, ...next.optional]);
      setImportError(undefined);
      setStep("select");
    } catch (error) {
      setImportError(error instanceof Error ? error.message : String(error));
    }
  }
  function close() {
    void runtimeIpcClient.dismissQuickSetup();
    onClose();
  }

  function toggle(id: string) {
    setRows((current) => current.map((row) => row.id === id ? { ...row, selected: !row.selected } : row));
  }

  function toggleImported(kind: string, id: string) {
    setImportedReviewRows((current) => current.map((resource) =>
      resource.kind === kind && resource.id === id
        ? { ...resource, selected: !resource.selected }
        : resource));
  }

  async function importSetup() {
    try {
      const imported = await runtimeIpcClient.importPortableSetup();
      if (!imported) {
        setImportError(runtimeIpcClient.isDesktop() ? undefined : t("setup.nativeOnly"));
        return;
      }
      const knownToolIds = new Set(rows.map((row) => row.id));
      const importedIds = new Set(imported.document.resources
        .filter((resource) => resource.kind === "tool")
        .map((resource) => resource.id));
      setRows((current) => current.map((row) => ({
        ...row,
        selected: row.selected || importedIds.has(row.id),
      })));
      setImportedReviewRows(imported.document.resources
        .filter((resource) => resource.kind !== "tool" || !knownToolIds.has(resource.id))
        .map((resource) => ({ ...resource, selected: true })));
      const messages = [
        ...imported.warnings,
        imported.reviewRequiredIds.length
          ? t("setup.reviewRequired", { items: imported.reviewRequiredIds.join(", ") })
          : "",
      ].filter(Boolean);
      setImportError(messages[0]);
      setStep("select");
    } catch (error) {
      setImportError(error instanceof Error ? error.message : t("setup.invalidFile"));
    }
  }

  async function exportSetup() {
    try {
      const fileName = await runtimeIpcClient.exportPortableSetup(setup?.target ?? "macos_arm64");
      setImportError(fileName
        ? t("setup.exported", { file: fileName })
        : runtimeIpcClient.isDesktop()
          ? undefined
          : t("setup.nativeOnly"));
    } catch (error) {
      setImportError(error instanceof Error ? error.message : t("setup.exportBlocked"));
    }
  }

  return (
    <FixtureDialog
      open={open}
      onClose={close}
      title={t("setup.title")}
      description={t("setup.description")}
      footer={(
        <>
          <button className="secondary-button" type="button" onClick={close}>{t("setup.skip")}</button>
          {step === "select" || (step === "review" && lifecycle.stage === "result") ? <button className="secondary-button" type="button" onClick={() => void exportSetup()}>{lifecycle.stage === "result" ? t("setup.exportInstalled") : t("setup.exportCurrent")}</button> : null}
          {step === "select" ? <button className="primary-button" type="button" disabled={selectedCount === 0} onClick={() => setStep("review")}>{t("setup.review", { count: selectedCount })}</button> : null}
          {step === "review" && lifecycle.stage === "review" && lifecycle.plan ? <button className="primary-button" type="button" disabled={!lifecycle.consented || !lifecycle.consentEligible} onClick={() => void lifecycle.start()}>{t("setup.install")}</button> : null}
        </>
      )}
    >
      {step === "source" ? (
        <div className="choice-list">
          <button className="choice-control" type="button" onClick={() => setStep(setup?.dismissed ? "select" : "provider")}><span><strong>{t("setup.useRecommendations")}</strong><small>{t("setup.useRecommendationsDetail")}</small></span></button>
          <button className="choice-control" type="button" onClick={() => void importSetup()}><span><strong>{t("setup.import")}</strong><small>{t("setup.importDetail")}</small></span></button>
          {importError ? <p className="dialog-loading">{importError}</p> : null}
        </div>
      ) : null}
      {step === "provider" ? (
        <fieldset className="choice-list">
          <legend>{t("setup.provider")}</legend>
          <p>{t("setup.detected")}: {setup?.providers.homebrew ? "Homebrew ✓" : t("setup.noHomebrew")} · {setup?.providers.npm ? "npm ✓" : t("setup.noNpm")}</p>
          <ProviderChoice value="automatic" current={preference} label={t("setup.automatic")} detail={t("setup.automaticDetail")} onChange={setPreference} />
          <ProviderChoice value="prefer_homebrew" current={preference} label={t("setup.homebrew")} detail={t("setup.homebrewDetail")} onChange={setPreference} />
          <ProviderChoice value="prefer_bun" current={preference} label={t("setup.bun")} detail={t("setup.bunDetail")} onChange={setPreference} />
          <button className="primary-button" type="button" onClick={() => void saveProviderAndContinue()}>{t("setup.continue")}</button>
        </fieldset>
      ) : null}
      {step === "select" ? (
        <div>
          <p>{t("setup.detected")}: {setup?.providers.homebrew ? "Homebrew ✓" : t("setup.noHomebrew")} · {setup?.providers.npm ? "npm ✓" : t("setup.noNpm")}</p>
          <div className="detail-actions">
            <button className="secondary-button" type="button" onClick={() => { setRows((current) => current.map((row) => ({ ...row, selected: row.action !== "blocked" }))); setImportedReviewRows((current) => current.map((resource) => ({ ...resource, selected: true }))); }}>{t("setup.selectAll")}</button>
            <button className="secondary-button" type="button" onClick={() => { setRows((current) => current.map((row) => ({ ...row, selected: false }))); setImportedReviewRows((current) => current.map((resource) => ({ ...resource, selected: false }))); }}>{t("setup.clearAll")}</button>
          </div>
          <SetupList rows={rows.filter((row) => !row.optional)} onToggle={toggle} />
          <h3>{t("setup.optional")}</h3>
          <SetupList rows={rows.filter((row) => row.optional)} onToggle={toggle} />
          {importedReviewRows.length ? <><h3>{t("setup.imported")}</h3><ImportedSetupList rows={importedReviewRows} onToggle={toggleImported} /></> : null}
        </div>
      ) : null}
      {step === "review" ? (
        <div>
          {lifecycle.prepareError ? <div className="warning-callout"><strong>{t("setup.prepareFailed")}</strong><p>{lifecycle.prepareError}</p><button className="secondary-button" type="button" onClick={lifecycle.retryPrepare}>{t("setup.retry")}</button></div> : null}
          {lifecycle.executionError ? <div className="warning-callout"><strong>{t("error.operation", { message: lifecycle.executionError })}</strong></div> : null}
          {!lifecycle.prepareError && (lifecycle.stage === "loading" || !lifecycle.plan) ? <p className="dialog-loading">{t("setup.preparing")}</p> : null}
          {lifecycle.stage === "review" && lifecycle.plan ? <LifecyclePlanReview plan={lifecycle.plan} consented={lifecycle.consented} onConsentChange={lifecycle.setConsented} /> : null}
          {(lifecycle.stage === "progress" || lifecycle.stage === "result") && lifecycle.plan && lifecycle.result ? <LifecycleExecutionState plan={lifecycle.plan} result={lifecycle.result} onReviewFollowUp={(action) => void lifecycle.reviewFollowUp(action)} /> : null}
        </div>
      ) : null}
    </FixtureDialog>
  );
}

function ProviderChoice({ value, current, label, detail, onChange }: { value: InstallProviderPreference; current: InstallProviderPreference; label: string; detail: string; onChange: (value: InstallProviderPreference) => void }) {
  return <label className="choice-control"><input type="radio" name="provider" checked={current === value} onChange={() => onChange(value)} /><span><strong>{label}</strong><small>{detail}</small></span></label>;
}

function SetupList({ rows, onToggle }: { rows: SetupRowView[]; onToggle: (id: string) => void }) {
  const { t } = useI18n();
  return (
    <div className="resource-list" aria-label={t("setup.title")}>
      {rows.map((row) => (
        <label className="update-row" key={row.id}>
          <span className="checkbox-cell"><input type="checkbox" checked={row.selected} disabled={row.action === "blocked"} onChange={() => onToggle(row.id)} /></span>
          <span className="update-name"><strong>{row.name}</strong></span>
          <strong>{t(actionKeys[row.action])}</strong>
          <span>{localizedOwner(row.owner, t)}</span>
        </label>
      ))}
    </div>
  );
}

function ImportedSetupList({ rows, onToggle }: { rows: ImportedResourceSelection[]; onToggle: (kind: string, id: string) => void }) {
  const { t } = useI18n();
  return (
    <div className="resource-list" aria-label={t("setup.imported")}>
      {rows.map((row) => (
        <label className="update-row" key={`${row.kind}:${row.id}`}>
          <span className="checkbox-cell"><input type="checkbox" checked={row.selected} onChange={() => onToggle(row.kind, row.id)} /></span>
          <span className="update-name"><strong>{row.id}</strong><small>{t("setup.imported")}</small></span>
          <span>{t("review.other", { name: row.id })}</span>
        </label>
      ))}
    </div>
  );
}

function localizedOwner(owner: string, t: Translator) {
  if (owner === "External") return t("owner.external");
  if (owner.toLowerCase().includes("updater")) return t("owner.vendor");
  return owner;
}
