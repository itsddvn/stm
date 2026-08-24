import type { LifecycleExecutionResult, LifecycleFollowUpAction, LifecyclePlan } from "../../contracts/ui/lifecycle-contract";
import { useI18n, type MessageKey } from "../lib/i18n";
import { AppIcon } from "./app-icon";

const statusKeys: Record<LifecycleExecutionResult["status"], MessageKey> = {
  in_progress: "result.progress",
  success: "result.success",
  partial: "result.partial",
  failed: "result.failed",
  cancelled: "result.cancelled",
  recoverable: "result.recoverable",
};

export function LifecycleExecutionState({
  plan,
  result,
  onReviewFollowUp,
}: {
  plan: LifecyclePlan;
  result: LifecycleExecutionResult;
  onReviewFollowUp: (action: LifecycleFollowUpAction) => void;
}) {
  const { t } = useI18n();
  const inProgress = result.status === "in_progress";
  const failedItems = result.items.filter((item) => item.status === "failed");
  return (
    <div className={inProgress ? "lifecycle-execution progress-state" : "lifecycle-execution result-state"} aria-live="polite">
      <span className="progress-glyph"><AppIcon name={inProgress ? "run" : result.status === "success" ? "success" : "warning"} size={28} /></span>
      <h3>{t(statusKeys[result.status])}</h3>
      <progress value={result.completedSteps} max={result.totalSteps}>{result.completedSteps} / {result.totalSteps}</progress>
      <p className="progress-copy">{t("result.summary", { done: result.completedSteps, total: result.totalSteps })}</p>
      {failedItems.length ? (
        <div className="warning-callout">
          <AppIcon name="warning" />
          <div>{failedItems.map((item) => <p key={item.id}><strong>{item.label}</strong>: {item.redactedDetail}</p>)}</div>
        </div>
      ) : null}
      {!inProgress && plan.execution.mode !== "vendor_handoff" && (result.retryActions.length || result.recoveryActions.length) ? (
        <div className="lifecycle-recovery-actions">
          {result.retryActions.map((action) => <button className="secondary-button" type="button" onClick={() => onReviewFollowUp(action)} key={action.id}><AppIcon name="refresh" />{action.label}</button>)}
          {result.recoveryActions.map((action) => <button className="secondary-button" type="button" onClick={() => onReviewFollowUp(action)} key={action.id}><AppIcon name="rollback" />{action.label}</button>)}
        </div>
      ) : null}
      <details className="advanced-details">
        <summary>{t("result.advanced")}</summary>
        <div className="lifecycle-item-results">
          {result.items.map((item) => (
            <article key={item.id}>
              <strong>{item.label}</strong>
              <span className={`operation-status operation-${item.status}`}>{t(itemStatusKey(item.status))}</span>
              <p>{item.redactedDetail}</p>
              <code>{item.receipt ?? "—"}</code>
            </article>
          ))}
        </div>
        <dl className="execution-receipt">
          <div><dt>{t("technical.plan")}</dt><dd><code>{result.planDigest}</code></dd></div>
          <div><dt>{t("technical.operation")}</dt><dd><code>{result.operationId}</code></dd></div>
          <div><dt>{t("technical.receipt")}</dt><dd><code>{result.receipt ?? "—"}</code></dd></div>
        </dl>
      </details>
    </div>
  );
}

function itemStatusKey(status: string): MessageKey {
  if (status === "success") return "result.success";
  if (status === "failed") return "result.failed";
  if (status === "cancelled") return "result.cancelled";
  if (status === "skipped") return "result.skipped";
  return "result.progress";
}
