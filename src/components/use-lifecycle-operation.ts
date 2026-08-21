import { useEffect, useMemo, useState } from "react";
import type {
  LifecycleExecutionResult,
  LifecycleFollowUpAction,
  LifecyclePlan,
  LifecyclePlanRequest,
} from "../../contracts/ui/lifecycle-contract";
import { runtimeIpcClient } from "../lib/ipc/runtime-ipc-client";

export type LifecycleStage = "loading" | "review" | "progress" | "result";

interface LifecycleOperationState {
  requestKey: string;
  plan: LifecyclePlan | null;
  result: LifecycleExecutionResult | null;
  stage: LifecycleStage;
}

interface ConsentState {
  evidenceKey: string;
  checked: boolean;
}

export function useLifecycleOperation(request: LifecyclePlanRequest | null, open: boolean) {
  const requestKey = useMemo(() => JSON.stringify(request), [request]);
  const [operation, setOperation] = useState<LifecycleOperationState>({
    requestKey: "",
    plan: null,
    result: null,
    stage: "loading",
  });
  const [consent, setConsent] = useState<ConsentState>({ evidenceKey: "", checked: false });
  const current = operation.requestKey === requestKey
    ? operation
    : { requestKey, plan: null, result: null, stage: "loading" as const };
  const { plan, result, stage } = current;
  const consentEvidenceKey = plan ? lifecycleConsentEvidenceKey(plan) : "";
  const consentEligible = plan ? isLifecycleConsentEligible(plan) : false;
  const consented = consentEligible && consent.evidenceKey === consentEvidenceKey && consent.checked;

  useEffect(() => {
    if (!open || !request) return;
    let active = true;
    void runtimeIpcClient.prepareLifecycle(request).then((nextPlan) => {
      if (!active) return;
      setOperation({ requestKey, plan: nextPlan, result: null, stage: "review" });
    });
    return () => { active = false; };
  }, [open, request, requestKey]);

  function setConsented(checked: boolean) {
    setConsent({ evidenceKey: consentEvidenceKey, checked });
  }

  async function start() {
    if (!plan || !consented || !isLifecycleConsentEligible(plan)) return;
    const nextResult = await runtimeIpcClient.startLifecycle(plan.planId, {
      planDigest: plan.digest,
      planExpiresAt: plan.expiresAt,
      grantedAt: new Date().toISOString(),
    });
    setOperation({ requestKey, plan, result: nextResult, stage: lifecycleStageForResult(nextResult) });
    notifyLifecycleSettled(nextResult);
  }

  async function refreshStatus() {
    if (!plan || !result) return;
    const nextResult = await runtimeIpcClient.getLifecycleStatus(result.operationId);
    setOperation({ requestKey, plan, result: nextResult, stage: lifecycleStageForResult(nextResult) });
    notifyLifecycleSettled(nextResult);
  }

  async function cancel() {
    if (!plan || !result) return;
    const nextResult = await runtimeIpcClient.cancelLifecycle(result.operationId);
    setOperation({ requestKey, plan, result: nextResult, stage: lifecycleStageForResult(nextResult) });
    notifyLifecycleSettled(nextResult);
  }

  async function reviewFollowUp(action: LifecycleFollowUpAction) {
    const nextPlan = await runtimeIpcClient.prepareLifecycle(action.planRequest);
    setOperation({ requestKey, plan: nextPlan, result: null, stage: "review" });
  }

  return { plan, result, stage, consented, consentEligible, setConsented, start, refreshStatus, cancel, reviewFollowUp };
}

function notifyLifecycleSettled(result: LifecycleExecutionResult) {
  if (result.status !== "in_progress" && runtimeIpcClient.isDesktop()) {
    window.dispatchEvent(new Event("stm:lifecycle-settled"));
  }
}

export function lifecycleStageForResult(result: LifecycleExecutionResult): Extract<LifecycleStage, "progress" | "result"> {
  return result.status === "in_progress" ? "progress" : "result";
}

export function isLifecycleConsentEligible(plan: LifecyclePlan, now = Date.now()): boolean {
  const expiresAt = Date.parse(plan.expiresAt);
  const checkedAt = Date.parse(plan.revalidation.checkedAt);
  const planIsFresh = Number.isFinite(expiresAt)
    && Number.isFinite(checkedAt)
    && plan.revalidation.state === "fresh"
    && checkedAt <= now
    && expiresAt > now;
  if (!planIsFresh) return false;
  return plan.execution.mode !== "batch" || plan.execution.items.every((item) => isLifecycleConsentEligible(item, now));
}

export function lifecycleConsentEvidenceKey(plan: LifecyclePlan): string {
  const ownEvidence = [
    plan.planId,
    plan.digest,
    plan.expiresAt,
    plan.revalidation.checkedAt,
    plan.revalidation.state,
    JSON.stringify(plan.revalidation.checks),
  ].join("|");
  if (plan.execution.mode !== "batch") return ownEvidence;
  return `${ownEvidence}|${plan.execution.items.map(lifecycleConsentEvidenceKey).join("|")}`;
}
