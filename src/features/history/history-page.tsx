import { useMemo, useState } from "react";
import type { LifecycleFollowUpAction } from "../../../contracts/ui/lifecycle-contract";
import type { AppViewModel, OperationViewModel, SourceKind } from "../../../contracts/ui/view-model-contract";
import { AppIcon } from "../../components/app-icon";
import { EmptyState } from "../../components/empty-state";
import { FixtureDialog } from "../../components/fixture-dialog";
import { LifecycleExecutionState } from "../../components/lifecycle-execution-state";
import { SourceInstallDialog } from "../../components/source-install-dialog";
import { LifecyclePlanReview } from "../../components/lifecycle-plan-review";
import { LoadingTable } from "../../components/loading-table";
import { PageHeader } from "../../components/page-header";
import { StateNotice } from "../../components/state-notice";
import { useLifecycleOperation } from "../../components/use-lifecycle-operation";
import { useI18n } from "../../lib/i18n";

export function HistoryPage({ view }: { view: AppViewModel }) {
  const { t } = useI18n();
  const [selectedId, setSelectedId] = useState(view.operations[0]?.id ?? "");
  const [dialogOperation, setDialogOperation] = useState<OperationViewModel | null>(null);
  const [sourceReanalysis, setSourceReanalysis] = useState<SourceKind | null>(null);
  const selected = view.operations.find((operation) => operation.id === selectedId) ?? view.operations[0];
  const followUpKind = selected ? operationFollowUpKind(selected) : "inspect";

  return (
    <>
      <PageHeader title={t("page.history.title")} description={t("page.history.description")} />
      <StateNotice reasonCode={view.surface.reasonCode} />
      {view.surface.loadState === "loading" ? <LoadingTable /> : view.operations.length === 0 ? <EmptyState title="No operations recorded" detail="Lifecycle simulations and desktop adapter results will appear here." /> : (
        <div className="history-layout">
          <div className="history-table" aria-label="Operations">
            <div className="table-header history-columns"><span>Status</span><span>Resource</span><span>Action</span><span>Authority</span><span>Started</span></div>
            {view.operations.map((operation) => (
              <button className={`history-row history-columns ${selected?.id === operation.id ? "selected" : ""}`} type="button" onClick={() => setSelectedId(operation.id)} key={operation.id}>
                <span className={`operation-status operation-${operation.status}`}><AppIcon name={operation.status === "success" ? "success" : operation.status === "failed" ? "failure" : "warning"} size={16} />{operation.status}</span>
                <strong>{operation.resource}</strong>
                <span>{operation.action}</span>
                <span>{operation.owner}</span>
                <span className="mono-data">{formatDate(operation.startedAt)}</span>
              </button>
            ))}
          </div>
          {selected ? (
            <aside className="history-detail">
              <span className="detail-kicker">{selected.id}</span>
              <h2>{selected.resource}</h2>
              <p>{selected.detail}</p>
              <dl className="detail-grid single">
                <div><dt>Status</dt><dd>{selected.status}</dd></div>
                <div><dt>Action</dt><dd>{selected.action}</dd></div>
                <div><dt>Authority</dt><dd>{selected.owner}</dd></div>
                <div><dt>Receipt</dt><dd className="mono-data">{selected.receipt}</dd></div>
              </dl>
              <section className="history-receipt-details" aria-label="Per-item receipt details">
                <h3>Per-item receipt details</h3>
                {selected.details.length ? (
                  <ul>{selected.details.map((detail) => <li className="mono-data" key={detail}>{detail}</li>)}</ul>
                ) : <p>No per-item detail was recorded.</p>}
              </section>
              <button
                className={followUpKind === "inspect" ? "secondary-button" : "primary-button"}
                type="button"
                onClick={() => {
                  const kind = sourceReanalysisKind(selected.lifecycleRequest);
                  if (kind) {
                    setSourceReanalysis(kind);
                  } else {
                    setDialogOperation(selected);
                  }
                }}
              >
                <AppIcon name={followUpKind === "recover" ? "rollback" : "search"} />
                {followUpKind === "recover" ? "Review Recovery" : followUpKind === "retry" ? "Review Retry" : "Inspect Receipt"}
              </button>
            </aside>
          ) : null}
        </div>
      )}
      {dialogOperation ? (
        <HistoryLifecycleDialog
          operation={dialogOperation}
          open
          onClose={() => setDialogOperation(null)}
          onReanalyzeSource={(kind) => {
            setDialogOperation(null);
            setSourceReanalysis(kind);
          }}
        />
      ) : null}
      {sourceReanalysis ? (
        <SourceInstallDialog
          kind={sourceReanalysis}
          open
          onClose={() => setSourceReanalysis(null)}
          title={`Re-analyze ${sourceReanalysis === "mcp" ? "MCP" : sourceReanalysis === "skill" ? "Skill" : "Tool"} Source`}
        />
      ) : null}
    </>
  );
}

function HistoryLifecycleDialog({
  operation,
  open,
  onClose,
  onReanalyzeSource,
}: {
  operation: OperationViewModel;
  open: boolean;
  onClose: () => void;
  onReanalyzeSource: (kind: SourceKind) => void;
}) {
  const followUpKind = operationFollowUpKind(operation);
  const request = useMemo(() => operation.lifecycleRequest, [operation.lifecycleRequest]);
  const lifecycle = useLifecycleOperation(request, open);
  const titleVerb = followUpKind === "recover" ? "Recover" : followUpKind === "retry" ? "Retry" : "Inspect";
  return (
    <FixtureDialog
      open={open}
      onClose={onClose}
      title={`${titleVerb} ${operation.resource}`}
      description="Review receipt-bound evidence before retry or recovery."
      footer={(
        <>
          <button className="secondary-button" type="button" onClick={onClose}>
            {lifecycle.stage === "result" ? "Close" : followUpKind === "inspect" ? "Close" : "Cancel"}
          </button>
          {lifecycle.stage === "review" && lifecycle.plan?.execution.mode !== "detect_only" ? (
            <button className="primary-button" type="button" disabled={lifecycle.starting || !lifecycle.consented || !lifecycle.consentEligible} onClick={() => void lifecycle.start()}>
              <AppIcon name={followUpKind === "recover" ? "rollback" : "run"} />
              Authorize &amp; Start
            </button>
          ) : null}
          {lifecycle.stage === "progress" && lifecycle.result?.canCancel ? <button className="secondary-button" type="button" onClick={() => void lifecycle.cancel()}>Cancel Operation</button> : null}
          {lifecycle.stage === "progress" ? <button className="primary-button" type="button" onClick={() => void lifecycle.refreshStatus()}>Refresh Status</button> : null}
        </>
      )}
    >
      {lifecycle.stage === "loading" ? <p className="dialog-loading">Preparing receipt evidence…</p> : null}
      {lifecycle.stage === "review" && lifecycle.plan ? <LifecyclePlanReview plan={lifecycle.plan} consented={lifecycle.consented} onConsentChange={lifecycle.setConsented} /> : null}
      {(lifecycle.stage === "progress" || lifecycle.stage === "result") && lifecycle.plan && lifecycle.result ? (
        <LifecycleExecutionState
          plan={lifecycle.plan}
          result={lifecycle.result}
          onReviewFollowUp={(action) => reviewHistoryFollowUp(action, lifecycle.reviewFollowUp, onReanalyzeSource)}
        />
      ) : null}
    </FixtureDialog>
  );
}

function reviewHistoryFollowUp(
  action: LifecycleFollowUpAction,
  reviewFollowUp: (action: LifecycleFollowUpAction) => Promise<void>,
  onReanalyzeSource: (kind: SourceKind) => void,
) {
  const kind = sourceReanalysisKind(action.planRequest);
  if (kind) {
    onReanalyzeSource(kind);
    return;
  }
  void reviewFollowUp(action);
}

export function sourceReanalysisKind(request: LifecycleFollowUpAction["planRequest"]): SourceKind | null {
  const kind = request.resourceKind;
  return request.action === "reanalyze-source" && (kind === "tool" || kind === "skill" || kind === "mcp")
    ? kind
    : null;
}

function operationFollowUpKind(operation: OperationViewModel): "inspect" | "retry" | "recover" {
  if (operation.lifecycleRequest.action === "inspect-receipt") return "inspect";
  if (operation.lifecycleRequest.action === "recover" || operation.status === "recoverable") return "recover";
  if (operation.status === "failed" || operation.status === "partial" || operation.status === "cancelled") return "retry";
  return "inspect";
}

function formatDate(value: string) {
  return new Intl.DateTimeFormat("en", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }).format(new Date(value));
}
