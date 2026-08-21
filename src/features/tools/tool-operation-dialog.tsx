import { useMemo } from "react";
import type { ToolViewModel } from "../../../contracts/ui/view-model-contract";
import { ActionDisabledReason } from "../../components/action-disabled-reason";
import { AppIcon } from "../../components/app-icon";
import { FixtureDialog } from "../../components/fixture-dialog";
import { LifecycleExecutionState } from "../../components/lifecycle-execution-state";
import { LifecyclePlanReview } from "../../components/lifecycle-plan-review";
import { useLifecycleOperation } from "../../components/use-lifecycle-operation";

export function ToolOperationDialog({ tool, open, onClose }: { tool: ToolViewModel; open: boolean; onClose: () => void }) {
  const request = useMemo(() => ({
    resourceKind: "tool" as const,
    action: tool.state === "missing" ? "install" : "update",
    resourceId: tool.id,
  }), [tool]);
  const lifecycle = useLifecycleOperation(request, open);
  const actionBlocked = !tool.primaryAction.enabled;
  const guidanceOnly = tool.executionMode === "detect_only";

  return (
    <FixtureDialog
      open={open}
      onClose={onClose}
      title={guidanceOnly ? `${tool.name} Guidance` : `${tool.name} Lifecycle Review`}
      description="Review the complete typed plan before the operation crosses an execution or vendor boundary."
      footer={(
        <>
          <button className="secondary-button" type="button" onClick={onClose}>{lifecycle.stage === "result" ? "Close" : "Cancel"}</button>
          {lifecycle.stage === "review" && lifecycle.plan && !guidanceOnly && !actionBlocked ? <button className="primary-button" type="button" disabled={!lifecycle.consented || !lifecycle.consentEligible} onClick={() => void lifecycle.start()}><AppIcon name="run" />Authorize &amp; Start</button> : null}
          {lifecycle.stage === "progress" && lifecycle.result?.canCancel ? <button className="secondary-button" type="button" onClick={() => void lifecycle.cancel()}>Cancel Operation</button> : null}
          {lifecycle.stage === "progress" ? <button className="primary-button" type="button" onClick={() => void lifecycle.refreshStatus()}>Refresh Status</button> : null}
        </>
      )}
    >
      {lifecycle.stage === "loading" ? <p className="dialog-loading">Preparing deterministic lifecycle evidence…</p> : null}
      {lifecycle.stage === "review" && lifecycle.plan ? (
        <>
          {actionBlocked ? <ActionDisabledReason reasonCode={tool.primaryAction.disabledReasonCode} /> : null}
          <LifecyclePlanReview plan={lifecycle.plan} consented={lifecycle.consented} onConsentChange={lifecycle.setConsented} />
        </>
      ) : null}
      {(lifecycle.stage === "progress" || lifecycle.stage === "result") && lifecycle.plan && lifecycle.result ? <LifecycleExecutionState plan={lifecycle.plan} result={lifecycle.result} onReviewFollowUp={(action) => void lifecycle.reviewFollowUp(action)} /> : null}
    </FixtureDialog>
  );
}
