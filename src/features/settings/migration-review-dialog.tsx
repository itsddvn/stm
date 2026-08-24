import { useMemo, useState } from "react";
import type { MigrationCandidate } from "../../../contracts/ui/setup-contract";
import { FixtureDialog } from "../../components/fixture-dialog";
import { LifecycleExecutionState } from "../../components/lifecycle-execution-state";
import { LifecyclePlanReview } from "../../components/lifecycle-plan-review";
import { useLifecycleOperation } from "../../components/use-lifecycle-operation";

export function MigrationReviewDialog({ candidate, open, onClose }: { candidate: MigrationCandidate; open: boolean; onClose: () => void }) {
  const [cleanupOldOwner, setCleanupOldOwner] = useState(candidate.cleanupOldOwner);
  const request = useMemo(() => ({
    resourceKind: "operation" as const,
    action: cleanupOldOwner ? "migrate-with-cleanup" : "migrate-keep-source",
    resourceId: candidate.recipe.resourceId,
  }), [candidate.recipe.resourceId, cleanupOldOwner]);
  const lifecycle = useLifecycleOperation(request, open);

  return (
    <FixtureDialog
      open={open}
      onClose={onClose}
      title="Review provider migration"
      description="Install and verify the Homebrew-owned Codex executable before optional npm cleanup. Shared Codex configuration remains untouched."
      footer={(
        <>
          <button className="secondary-button" type="button" onClick={onClose}>{lifecycle.stage === "result" ? "Close" : "Cancel"}</button>
          {lifecycle.stage === "review" && lifecycle.plan ? <button className="primary-button" type="button" disabled={!lifecycle.consented || !lifecycle.consentEligible} onClick={() => void lifecycle.start()}>Start migration</button> : null}
        </>
      )}
    >
      <div className="info-callout">
        <p><strong>{candidate.recipe.id}</strong></p>
        <p>{candidate.recipe.sourceMappingId} → {candidate.recipe.targetMappingId}</p>
        <p>Target: {candidate.recipe.targetExecutablePaths.join(" or ")}</p>
      </div>
      <label className="setting-toggle">
        <span><strong>Remove old npm installation after verification</strong><small>Preselected. Cleanup cannot start if target installation or explicit target executable verification fails.</small></span>
        <input type="checkbox" checked={cleanupOldOwner} disabled={lifecycle.stage !== "loading" && lifecycle.stage !== "review"} onChange={(event) => setCleanupOldOwner(event.target.checked)} />
      </label>
      {lifecycle.prepareError ? <div className="warning-callout"><strong>Migration plan unavailable</strong><p>{lifecycle.prepareError}</p><button className="secondary-button" type="button" onClick={lifecycle.retryPrepare}>Retry</button></div> : null}
      {!lifecycle.prepareError && (lifecycle.stage === "loading" || !lifecycle.plan) ? <p className="dialog-loading">Preparing migration plan…</p> : null}
      {lifecycle.stage === "review" && lifecycle.plan ? <LifecyclePlanReview plan={lifecycle.plan} consented={lifecycle.consented} onConsentChange={lifecycle.setConsented} /> : null}
      {(lifecycle.stage === "progress" || lifecycle.stage === "result") && lifecycle.plan && lifecycle.result ? <LifecycleExecutionState plan={lifecycle.plan} result={lifecycle.result} onReviewFollowUp={(action) => void lifecycle.reviewFollowUp(action)} /> : null}
    </FixtureDialog>
  );
}
