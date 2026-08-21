import type { ScenarioId } from "../../../contracts/ui/state-contract";
import type { AppViewModel, SourceAnalysisViewModel, SourceKind } from "../../../contracts/ui/view-model-contract";
import type {
  LifecycleConsentAuthorization,
  LifecycleExecutionResult,
  LifecycleIpcClient,
  LifecyclePlan,
  LifecyclePlanRequest,
} from "../../../contracts/ui/lifecycle-contract";
import { buildLifecycleExecution, buildLifecyclePlan } from "../../fixtures/lifecycle-fixtures";
import { buildScenarioFixture } from "../../fixtures/scenario-fixtures";
import { analyzeSourceFixture } from "../../fixtures/source-analysis-fixtures";

export interface ToolsManagerIpcClient extends LifecycleIpcClient {
  getAppView(scenario: ScenarioId): Promise<AppViewModel>;
  analyzeSource(kind: SourceKind, url: string): Promise<SourceAnalysisViewModel>;
}

class FixtureIpcClient implements ToolsManagerIpcClient {
  private readonly plans = new Map<string, LifecyclePlan>();
  private readonly operations = new Map<string, LifecyclePlan>();

  async getAppView(scenario: ScenarioId): Promise<AppViewModel> {
    return Promise.resolve(buildScenarioFixture(scenario));
  }

  async analyzeSource(kind: SourceKind, url: string): Promise<SourceAnalysisViewModel> {
    return Promise.resolve(analyzeSourceFixture(kind, url));
  }

  async prepareLifecycle<TRequest extends LifecyclePlanRequest>(request: TRequest) {
    const plan = buildLifecyclePlan(request);
    this.plans.set(plan.planId, structuredClone(plan));
    return Promise.resolve(plan);
  }

  async startLifecycle(planId: string, authorization: LifecycleConsentAuthorization) {
    const plan = this.requirePlan(planId);
    validateFixtureAuthorization(plan, authorization);
    const result = buildLifecycleExecution(plan, "in_progress");
    this.operations.set(result.operationId, plan);
    return Promise.resolve(result);
  }

  async getLifecycleStatus(operationId: string): Promise<LifecycleExecutionResult> {
    const plan = this.requireOperation(operationId);
    const status = plan.request.resourceKind === "product" ? "recoverable"
      : plan.request.action.includes("partial") ? "partial"
        : "success";
    return Promise.resolve(buildLifecycleExecution(plan, status));
  }

  async cancelLifecycle(operationId: string): Promise<LifecycleExecutionResult> {
    const plan = this.requireOperation(operationId);
    return Promise.resolve(buildLifecycleExecution(plan, "cancelled"));
  }

  private requirePlan(planId: string) {
    const plan = this.plans.get(planId);
    if (!plan) throw new Error(`Unknown or expired fixture lifecycle plan: ${planId}`);
    return plan;
  }

  private requireOperation(operationId: string) {
    const plan = this.operations.get(operationId);
    if (!plan) throw new Error(`Unknown fixture lifecycle operation: ${operationId}`);
    return plan;
  }
}

function validateFixtureAuthorization(plan: LifecyclePlan, authorization: LifecycleConsentAuthorization) {
  if (authorization.planDigest !== plan.digest || authorization.planExpiresAt !== plan.expiresAt) {
    throw new Error("Lifecycle consent authorization does not match the reviewed plan evidence");
  }
  if (!planEvidenceIsFresh(plan, Date.parse(authorization.grantedAt))) {
    throw new Error("Lifecycle consent authorization is stale or expired");
  }
}

function planEvidenceIsFresh(plan: LifecyclePlan, grantedAt: number): boolean {
  const expiresAt = Date.parse(plan.expiresAt);
  const checkedAt = Date.parse(plan.revalidation.checkedAt);
  if (!Number.isFinite(grantedAt) || !Number.isFinite(expiresAt) || !Number.isFinite(checkedAt)) return false;
  if (plan.revalidation.state !== "fresh" || expiresAt <= grantedAt || checkedAt > grantedAt) return false;
  return plan.execution.mode !== "batch" || plan.execution.items.every((item) => planEvidenceIsFresh(item, grantedAt));
}

export const mockIpcClient: ToolsManagerIpcClient = new FixtureIpcClient();
