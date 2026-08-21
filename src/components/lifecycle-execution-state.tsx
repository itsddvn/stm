import type { LifecycleExecutionResult, LifecycleFollowUpAction, LifecyclePlan } from "../../contracts/ui/lifecycle-contract";
import { AppIcon } from "./app-icon";
import { isFixtureRuntime } from "../lib/ipc/runtime-ipc-client";

export function LifecycleExecutionState({
  plan,
  result,
  onReviewFollowUp,
}: {
  plan: LifecyclePlan;
  result: LifecycleExecutionResult;
  onReviewFollowUp: (action: LifecycleFollowUpAction) => void;
}) {
  const inProgress = result.status === "in_progress";
  const simulation = isFixtureRuntime();
  return (
    <div className={inProgress ? "lifecycle-execution progress-state" : "lifecycle-execution result-state"} aria-live="polite">
      <span className="progress-glyph"><AppIcon name={inProgress ? "run" : result.status === "success" ? "success" : "warning"} size={28} /></span>
      <p className="simulation-chip">{simulation ? "Simulation mode" : "Desktop operation"}</p>
      <h3>{inProgress ? `Lifecycle ${simulation ? "simulation" : "operation"} in progress` : `${simulation ? "Simulation" : "Operation"} ${result.status.replaceAll("_", " ")}`}</h3>
      <p>{result.redactedDetail}</p>
      <progress value={result.completedSteps} max={result.totalSteps}>{result.completedSteps} of {result.totalSteps}</progress>
      <p className="progress-copy">{result.completedSteps} of {result.totalSteps} steps · {result.canCancel ? "Cancellation available" : "Cancellation closed"}</p>
      <div className="lifecycle-item-results" aria-label="Per-item results">
        <h4>Every item result</h4>
        {result.items.map((item) => (
          <article key={item.id}>
            <span className={`operation-status operation-${item.status}`}><AppIcon name={item.status === "success" ? "success" : item.status === "failed" ? "failure" : "warning"} size={16} />{item.status.replaceAll("_", " ")}</span>
            <strong>{item.label}</strong>
            <p>{item.redactedDetail}</p>
            <code>{item.receipt ?? "Receipt pending"}</code>
          </article>
        ))}
      </div>
      <dl className="execution-receipt">
        <div><dt>Plan digest</dt><dd><code>{result.planDigest}</code></dd></div>
        <div><dt>Operation</dt><dd><code>{result.operationId}</code></dd></div>
        <div><dt>Receipt</dt><dd><code>{result.receipt ?? "Pending"}</code></dd></div>
      </dl>
      {!inProgress && plan.execution.mode !== "vendor_handoff" && (result.retryActions.length || result.recoveryActions.length) ? (
        <div className="lifecycle-recovery-actions">
          <p>Retry and recovery first create a fresh plan with a new digest, expiry, revalidation, and consent review.</p>
          {result.retryActions.map((action) => <button className="secondary-button" type="button" onClick={() => onReviewFollowUp(action)} key={action.id}><AppIcon name="refresh" />{action.label}</button>)}
          {result.recoveryActions.map((action) => <button className="secondary-button" type="button" onClick={() => onReviewFollowUp(action)} key={action.id}><AppIcon name="rollback" />{action.label}</button>)}
        </div>
      ) : null}
    </div>
  );
}
