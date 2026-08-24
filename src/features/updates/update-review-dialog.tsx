import { useMemo } from "react";
import type { UpdateViewModel } from "../../../contracts/ui/view-model-contract";
import { AppIcon } from "../../components/app-icon";
import { FixtureDialog } from "../../components/fixture-dialog";
import { LifecycleExecutionState } from "../../components/lifecycle-execution-state";
import { LifecyclePlanReview } from "../../components/lifecycle-plan-review";
import { useLifecycleOperation } from "../../components/use-lifecycle-operation";
import { useI18n } from "../../lib/i18n";

export function UpdateReviewDialog({ items, open, onClose }: { items: UpdateViewModel[]; open: boolean; onClose: () => void }) {
  const { t } = useI18n();
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
      title={t("page.updates.title")}
      description={t("page.updates.description")}
      footer={(
        <>
          <button className="secondary-button" type="button" onClick={onClose}>{t("common.close")}</button>
          {lifecycle.stage === "review" ? <button className="primary-button" type="button" disabled={!lifecycle.consented || !lifecycle.consentEligible} onClick={() => void lifecycle.start()}><AppIcon name="run" />{t("common.start")}</button> : null}
          {lifecycle.stage === "progress" && lifecycle.result?.canCancel ? <button className="secondary-button" type="button" onClick={() => void lifecycle.cancel()}>{t("common.cancelOperation")}</button> : null}
          {lifecycle.stage === "progress" ? <button className="primary-button" type="button" onClick={() => void lifecycle.refreshStatus()}>{t("common.refreshStatus")}</button> : null}
        </>
      )}
    >
      {lifecycle.stage === "loading" ? <p className="dialog-loading">{t("setup.preparing")}</p> : null}
      {lifecycle.executionError ? <div className="warning-callout"><strong>{t("error.operation", { message: lifecycle.executionError })}</strong></div> : null}
      {lifecycle.stage === "review" && lifecycle.plan ? (
        <LifecyclePlanReview plan={lifecycle.plan} consented={lifecycle.consented} onConsentChange={lifecycle.setConsented} />
      ) : null}
      {(lifecycle.stage === "progress" || lifecycle.stage === "result") && lifecycle.plan && lifecycle.result ? <LifecycleExecutionState plan={lifecycle.plan} result={lifecycle.result} onReviewFollowUp={(action) => void lifecycle.reviewFollowUp(action)} /> : null}
    </FixtureDialog>
  );
}
