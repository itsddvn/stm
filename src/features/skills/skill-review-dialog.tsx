import { useMemo, useState } from "react";
import type { SkillViewModel } from "../../../contracts/ui/view-model-contract";
import { ActionDisabledReason } from "../../components/action-disabled-reason";
import { AppIcon } from "../../components/app-icon";
import { FixtureDialog } from "../../components/fixture-dialog";
import { LifecycleExecutionState } from "../../components/lifecycle-execution-state";
import { LifecyclePlanReview } from "../../components/lifecycle-plan-review";
import { useLifecycleOperation } from "../../components/use-lifecycle-operation";

export function SkillReviewDialog({ skill, open, onClose }: { skill: SkillViewModel; open: boolean; onClose: () => void }) {
  const [choice, setChoice] = useState("");
  const selectedResolutionAction = skill.resolutionActions.find((action) => action.id === choice);
  const actionId = selectedResolutionAction?.id ?? skill.primaryAction.id;
  const request = useMemo(() => ({
    resourceKind: "skill" as const,
    action: actionId.includes("partial") ? "resolve-partial" : actionId.split(".").at(-1) ?? "review",
    resourceId: skill.id,
  }), [actionId, skill]);
  const lifecycle = useLifecycleOperation(request, open);
  const needsChoice = skill.resolutionActions.length > 0;
  const canStart = lifecycle.consented && lifecycle.consentEligible && (!needsChoice || Boolean(selectedResolutionAction?.enabled));
  const modified = skill.primaryAction.id === "skill.resolve_local_modification";
  const partial = skill.primaryAction.id === "skill.review_partial_failure";

  return (
    <FixtureDialog
      open={open}
      onClose={onClose}
      title={modified ? "Resolve Local Modification" : partial ? "Resolve Partial Failure" : `Review ${skill.name} Lifecycle`}
      description="Review provenance, content evidence, every target, and the digest-bound action before execution."
      footer={(
        <>
          <button className="secondary-button" type="button" onClick={onClose}>{lifecycle.stage === "result" ? "Close" : "Cancel"}</button>
          {lifecycle.stage === "review" ? <button className="primary-button" type="button" disabled={!canStart} onClick={() => void lifecycle.start()}><AppIcon name="run" />Authorize &amp; Start</button> : null}
          {lifecycle.stage === "progress" && lifecycle.result?.canCancel ? <button className="secondary-button" type="button" onClick={() => void lifecycle.cancel()}>Cancel Operation</button> : null}
          {lifecycle.stage === "progress" ? <button className="primary-button" type="button" onClick={() => void lifecycle.refreshStatus()}>Refresh Status</button> : null}
        </>
      )}
    >
      {lifecycle.stage === "loading" ? <p className="dialog-loading">Preparing skill lifecycle evidence…</p> : null}
      {lifecycle.stage === "review" && lifecycle.plan ? (
        <div className="skill-review">
          {modified ? <div className="warning-callout"><AppIcon name="warning" /><div><strong>Receipt digest differs from local content</strong><p>Choose a conflict action. The changed action produces a new plan digest and clears consent.</p></div></div> : null}
          {partial ? <div className="warning-callout"><AppIcon name="failure" /><div><strong>Target outcomes differ</strong><p>Every target stays visible with its own receipt and recovery boundary.</p></div></div> : null}
          {skill.diff.map((entry) => <div className="diff-row expanded" key={entry.file}><span className={`diff-kind diff-${entry.change}`}>{entry.change}</span><span className="mono-data">{entry.file}</span><p>{entry.summary}</p></div>)}
          {needsChoice ? <fieldset className="choice-list"><legend>{modified ? "Conflict action" : "Recovery action"}</legend>{skill.resolutionActions.map((action) => <Choice action={action} selected={choice} onChange={setChoice} key={action.id} />)}</fieldset> : null}
          <LifecyclePlanReview plan={lifecycle.plan} consented={lifecycle.consented} onConsentChange={lifecycle.setConsented} />
        </div>
      ) : null}
      {(lifecycle.stage === "progress" || lifecycle.stage === "result") && lifecycle.plan && lifecycle.result ? <LifecycleExecutionState plan={lifecycle.plan} result={lifecycle.result} onReviewFollowUp={(action) => void lifecycle.reviewFollowUp(action)} /> : null}
    </FixtureDialog>
  );
}

function Choice({ action, selected, onChange }: { action: SkillViewModel["resolutionActions"][number]; selected: string; onChange: (value: string) => void }) {
  return <label className={`choice-control ${!action.enabled ? "choice-control-disabled" : ""}`}><input type="radio" name="resolution" value={action.id} checked={selected === action.id} disabled={!action.enabled} onChange={() => onChange(action.id)} /><span><strong>{action.label}</strong><small>{resolutionDetail(action.id)}</small></span><ActionDisabledReason compact reasonCode={action.disabledReasonCode} /></label>;
}

function resolutionDetail(id: SkillViewModel["resolutionActions"][number]["id"]) {
  if (id === "skill.keep_local") return "Retain local content and leave upstream replacement blocked.";
  if (id === "skill.export_diff") return "Export a redacted patch without replacing content.";
  if (id === "skill.restore_managed") return "Restore the previous receipt-backed revision.";
  if (id === "skill.install_side_by_side") return "Create a separate target only where the client supports it.";
  if (id === "skill.retry_failed_target") return "Retry only the failed target with fresh revalidation.";
  if (id === "skill.rollback_completed_target") return "Restore the previous managed revision for completed targets.";
  return "Preserve all per-target results and receipts.";
}
