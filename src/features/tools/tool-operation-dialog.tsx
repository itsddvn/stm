import { useMemo } from "react";
import type { ToolViewModel } from "../../../contracts/ui/view-model-contract";
import { ActionDisabledReason } from "../../components/action-disabled-reason";
import { AppIcon } from "../../components/app-icon";
import { FixtureDialog } from "../../components/fixture-dialog";
import { LifecycleExecutionState } from "../../components/lifecycle-execution-state";
import { LifecyclePlanReview } from "../../components/lifecycle-plan-review";
import { useLifecycleOperation } from "../../components/use-lifecycle-operation";
import { useI18n } from "../../lib/i18n";

export function ToolOperationDialog({ tool, open, onClose }: { tool: ToolViewModel; open: boolean; onClose: () => void }) {
  const { t } = useI18n();
  const request = useMemo(() => ({
    resourceKind: "tool" as const,
    action: tool.state === "missing" ? "install" : tool.state === "managed_current" ? "inspect" : "update",
    resourceId: tool.id,
  }), [tool]);
  const lifecycle = useLifecycleOperation(request, open);
  const actionBlocked = !tool.primaryAction.enabled;
  const guidanceOnly = tool.executionMode === "detect_only";

  return (
    <FixtureDialog
      open={open}
      onClose={onClose}
      title={tool.name}
      description={t("setup.description")}
      footer={(
        <>
          <button className="secondary-button" type="button" onClick={onClose}>{t("common.close")}</button>
          {lifecycle.stage === "review" && lifecycle.plan && !guidanceOnly && !actionBlocked ? <button className="primary-button" type="button" disabled={!lifecycle.consented || !lifecycle.consentEligible} onClick={() => void lifecycle.start()}><AppIcon name="run" />{t("common.start")}</button> : null}
          {lifecycle.stage === "progress" && lifecycle.result?.canCancel ? <button className="secondary-button" type="button" onClick={() => void lifecycle.cancel()}>{t("common.cancelOperation")}</button> : null}
          {lifecycle.stage === "progress" ? <button className="primary-button" type="button" onClick={() => void lifecycle.refreshStatus()}>{t("common.refreshStatus")}</button> : null}
        </>
      )}
    >
      {lifecycle.stage === "loading" ? <p className="dialog-loading">{t("setup.preparing")}</p> : null}
      {lifecycle.executionError ? <div className="warning-callout"><strong>{t("error.operation", { message: lifecycle.executionError })}</strong></div> : null}
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
