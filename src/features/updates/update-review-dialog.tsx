import { useMemo } from "react";
import type { UpdateViewModel } from "../../../contracts/ui/view-model-contract";
import { AppIcon } from "../../components/app-icon";
import { FixtureDialog } from "../../components/fixture-dialog";
import { LifecycleExecutionState } from "../../components/lifecycle-execution-state";
import { LifecyclePlanReview } from "../../components/lifecycle-plan-review";
import { useLifecycleOperation } from "../../components/use-lifecycle-operation";

export function UpdateReviewDialog({ items, open, onClose }: { items: UpdateViewModel[]; open: boolean; onClose: () => void }) {
  const productOnly = items.length === 1 && items[0].resourceType === "product";
  const request = useMemo(() => items.length ? {
    resourceKind: productOnly ? "product" as const : "operation" as const,
    action: productOnly ? "product-update" : "update-queue",
    resourceId: productOnly ? items[0].id : "selected-update-queue",
    itemIds: productOnly ? undefined : items.map((item) => item.id),
  } : null, [items, productOnly]);
  const lifecycle = useLifecycleOperation(request, open);

  return (
    <FixtureDialog
      open={open}
      onClose={onClose}
      title={productOnly ? "Review Product Update" : "Review Update Queue"}
      description={productOnly ? "Review the signed product plan and independent recovery boundary." : "Review one digest-bound queue plan with every item preserved in results."}
      footer={(
        <>
          <button className="secondary-button" type="button" onClick={onClose}>{lifecycle.stage === "result" ? "Close" : "Cancel"}</button>
          {lifecycle.stage === "review" ? <button className="primary-button" type="button" disabled={!lifecycle.consented || !lifecycle.consentEligible} onClick={() => void lifecycle.start()}><AppIcon name="run" />Authorize &amp; Start</button> : null}
          {lifecycle.stage === "progress" && lifecycle.result?.canCancel ? <button className="secondary-button" type="button" onClick={() => void lifecycle.cancel()}>Cancel Operation</button> : null}
          {lifecycle.stage === "progress" ? <button className="primary-button" type="button" onClick={() => void lifecycle.refreshStatus()}>Refresh Status</button> : null}
        </>
      )}
    >
      {lifecycle.stage === "loading" ? <p className="dialog-loading">Preparing queue lifecycle evidence…</p> : null}
      {lifecycle.stage === "review" && lifecycle.plan ? (
        <div className="update-review-list">
          {productOnly ? <div className="handoff-boundary"><AppIcon name="privilege" /><div><strong>Independent signed product channel</strong><p>Package verification, restart, receipt, and recovery remain separate from catalog tool adapters.</p></div></div> : null}
          {items.map((item) => <article className="update-review-row" key={item.id}><span className="resource-glyph"><AppIcon name={item.resourceType === "skill" ? "skills" : item.resourceType === "product" ? "settings" : "tools"} /></span><div><strong>{item.name}</strong><small>{item.resourceType} · {item.executionMode.replaceAll("_", " ")}</small></div><span className="mono-data">{item.current}</span><span aria-hidden="true">→</span><span className="mono-data">{item.target}</span><p>{item.risk}</p></article>)}
          <LifecyclePlanReview plan={lifecycle.plan} consented={lifecycle.consented} onConsentChange={lifecycle.setConsented} />
        </div>
      ) : null}
      {(lifecycle.stage === "progress" || lifecycle.stage === "result") && lifecycle.plan && lifecycle.result ? <LifecycleExecutionState plan={lifecycle.plan} result={lifecycle.result} onReviewFollowUp={(action) => void lifecycle.reviewFollowUp(action)} /> : null}
    </FixtureDialog>
  );
}
