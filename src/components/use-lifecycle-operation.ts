import { useEffect, useState } from "react";
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
  prepareError: string | null;
}

interface ConsentState {
  evidenceKey: string;
  checked: boolean;
}

export function useLifecycleOperation(request: LifecyclePlanRequest | null, open: boolean) {
  const requestKey = JSON.stringify(request);
  const [operation, setOperation] = useState<LifecycleOperationState>({
    requestKey: "",
    plan: null,
    result: null,
    stage: "loading",
    prepareError: null,
  });
  const [retryNonce, setRetryNonce] = useState(0);
  const [consent, setConsent] = useState<ConsentState>({ evidenceKey: "", checked: false });
  const [executionError, setExecutionError] = useState<string | null>(null);
  const current = operation.requestKey === requestKey
    ? operation
    : { requestKey, plan: null, result: null, stage: "loading" as const, prepareError: null };
  const { plan, result, stage, prepareError } = current;
  const consentEvidenceKey = plan ? lifecycleConsentEvidenceKey(plan) : "";
  const consentEligible = plan ? isLifecycleConsentEligible(plan) : false;
  const consented = consentEligible && consent.evidenceKey === consentEvidenceKey && consent.checked;

  useEffect(() => {
    if (!open || !requestKey || requestKey === "null") return;
    let active = true;
    const nextRequest = JSON.parse(requestKey) as LifecyclePlanRequest;
    void runtimeIpcClient.prepareLifecycle(nextRequest).then((nextPlan) => {
      if (!active) return;
      setExecutionError(null);
      setOperation({ requestKey, plan: nextPlan, result: null, stage: "review", prepareError: null });
    }).catch((error: unknown) => {
      if (!active) return;
      setExecutionError(null);
      const message = error instanceof Error ? error.message : String(error);
      setOperation({ requestKey, plan: null, result: null, stage: "review", prepareError: message });
    });
    return () => { active = false; };
  }, [open, requestKey, retryNonce]);

  function setConsented(checked: boolean) {
    setConsent({ evidenceKey: consentEvidenceKey, checked });
  }

  async function start() {
    if (!plan || !consented || !isLifecycleConsentEligible(plan)) return;
    setExecutionError(null);
    try {
      const nextResult = await runtimeIpcClient.startLifecycle(plan.planId, {
        planDigest: plan.digest,
        planExpiresAt: plan.expiresAt,
        grantedAt: new Date().toISOString(),
      });
      setOperation({ requestKey, plan, result: nextResult, stage: lifecycleStageForResult(nextResult), prepareError: null });
      notifyLifecycleSettled(nextResult);
    } catch (error) {
      setExecutionError(error instanceof Error ? error.message : String(error));
    }
  }

  useEffect(() => {
    const operationId = result?.operationId;
    if (!open || stage !== "progress" || !operationId || !plan) return;
    const timer = window.setInterval(() => {
      void runtimeIpcClient.getLifecycleStatus(operationId).then((nextResult) => {
        setExecutionError(null);
        setOperation({ requestKey, plan, result: nextResult, stage: lifecycleStageForResult(nextResult), prepareError: null });
        notifyLifecycleSettled(nextResult);
      }).catch((error: unknown) => {
        setExecutionError(error instanceof Error ? error.message : String(error));
      });
    }, 400);
    return () => window.clearInterval(timer);
  }, [open, stage, result?.operationId, plan, requestKey]);

  async function refreshStatus() {
    if (!plan || !result) return;
    try {
      const nextResult = await runtimeIpcClient.getLifecycleStatus(result.operationId);
      setExecutionError(null);
      setOperation({ requestKey, plan, result: nextResult, stage: lifecycleStageForResult(nextResult), prepareError: null });
      notifyLifecycleSettled(nextResult);
    } catch (error) {
      setExecutionError(error instanceof Error ? error.message : String(error));
    }
  }

  async function cancel() {
    if (!plan || !result) return;
    try {
      const nextResult = await runtimeIpcClient.cancelLifecycle(result.operationId);
      setExecutionError(null);
      setOperation({ requestKey, plan, result: nextResult, stage: lifecycleStageForResult(nextResult), prepareError: null });
      notifyLifecycleSettled(nextResult);
    } catch (error) {
      setExecutionError(error instanceof Error ? error.message : String(error));
    }
  }

  async function reviewFollowUp(action: LifecycleFollowUpAction) {
    try {
      const nextPlan = await runtimeIpcClient.prepareLifecycle(action.planRequest);
      setExecutionError(null);
      setOperation({ requestKey, plan: nextPlan, result: null, stage: "review", prepareError: null });
    } catch (error) {
      setExecutionError(error instanceof Error ? error.message : String(error));
    }
  }

  return { plan, result, stage, prepareError, executionError, consented, consentEligible, setConsented, start, refreshStatus, cancel, reviewFollowUp, retryPrepare: () => setRetryNonce((value) => value + 1) };
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
