import type { AppViewModel } from "../../contracts/ui/view-model-contract";
import type { ReasonCode, ScenarioId } from "../../contracts/ui/state-contract";
import { operationFixtures } from "./operation-fixtures";
import { mcpFixtures } from "./mcp-fixtures";
import { withToolPresentationAction } from "./presentation-action-fixtures";
import { skillFixtures } from "./skill-fixtures";
import { toolFixtures } from "./tool-fixtures";
import { updateFixtures } from "./update-fixtures";

const reasonByScenario: Partial<Record<ScenarioId, ReasonCode>> = {
  empty: "inventory.empty",
  loading: "inventory.loading",
  partial: "inventory.partial",
  stale: "inventory.stale",
  unsupported: "mapping.unsupported",
  blocked: "mapping.blocked",
  manager_unavailable: "manager.unavailable",
  offline: "network.offline",
  cancelled: "operation.cancelled",
  failure: "operation.failed",
  recovery: "operation.recovery_available",
};

export function buildScenarioFixture(scenario: ScenarioId): AppViewModel {
  const reasonCode = reasonByScenario[scenario];
  const isLoading = scenario === "loading";
  const isEmpty = scenario === "empty";
  const isPartial = scenario === "partial";
  const tools: AppViewModel["tools"] = isEmpty || isLoading ? [] : isPartial ? toolFixtures.slice(0, 5) : toolFixtures;
  const skills: AppViewModel["skills"] = isEmpty || isLoading ? [] : isPartial ? skillFixtures.slice(0, 3) : skillFixtures;
  const mcpServers: AppViewModel["mcpServers"] = isEmpty || isLoading ? [] : isPartial ? mcpFixtures.slice(0, 2) : mcpFixtures;
  const updates: AppViewModel["updates"] = isEmpty || isLoading ? [] : isPartial ? updateFixtures.slice(0, 3) : updateFixtures;
  const operations = isEmpty || isLoading ? [] : operationFixtures.map((operation, index) => {
    if (index > 0) return operation;
    if (scenario === "cancelled") return { ...operation, status: "cancelled" as const, detail: "Cancelled before the next fixture step", lifecycleRequest: { resourceKind: "operation" as const, action: "recover", resourceId: operation.id } };
    if (scenario === "failure") return { ...operation, status: "failed" as const, detail: "Fixture manager returned a non-zero result", lifecycleRequest: { resourceKind: "operation" as const, action: "recover", resourceId: operation.id } };
    if (scenario === "recovery") return { ...operation, status: "recoverable" as const, detail: "Previous managed revision is ready", lifecycleRequest: { resourceKind: "operation" as const, action: "recover", resourceId: operation.id } };
    return operation;
  });

  return {
    surface: {
      loadState: isLoading ? "loading" : isEmpty ? "empty" : isPartial ? "partial" : scenario === "failure" ? "error" : "ready",
      reasonCode,
      freshness: scenario === "stale" || scenario === "offline" ? "stale" : "fresh",
    },
    tools: decorateTools(tools, scenario),
    skills,
    mcpServers,
    updates,
    operations,
  };
}

function decorateTools(tools: AppViewModel["tools"], scenario: ScenarioId) {
  if (!["unsupported", "blocked", "manager_unavailable"].includes(scenario)) return tools;
  return tools.map((tool, index) => {
    if (index > 0) return tool;
    const decoratedTool = {
      ...tool,
      state: scenario === "manager_unavailable" ? "manager_unavailable" : scenario === "unsupported" ? "unsupported" : "blocked",
      reasonCode: reasonByScenario[scenario],
      lifecycleConfidence: scenario === "blocked" ? "Policy blocked" : scenario === "unsupported" ? "No supported mapping" : "Manager unavailable",
    } as const;
    return withToolPresentationAction(decoratedTool);
  });
}
