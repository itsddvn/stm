import type {
  AtomicLifecyclePlan,
  LifecycleExecutionResult,
  LifecyclePlan,
  LifecyclePlanRequest,
  LifecyclePrivilege,
} from "../../contracts/ui/lifecycle-contract";
import type { ExecutionMode } from "../../contracts/ui/view-model-contract";
import { mcpFixtures } from "./mcp-fixtures";
import { operationFixtures } from "./operation-fixtures";
import { skillFixtures } from "./skill-fixtures";
import { resolveSourceAnalysisHandle } from "./source-analysis-fixtures";
import { toolFixtures } from "./tool-fixtures";
import { updateFixtures } from "./update-fixtures";

const fixtureExpiry = "2099-12-31T23:59:59+07:00";

interface ResolvedPlanEvidence {
  canonicalId: string;
  mappingId: string;
  owner: string;
  source: string;
  executionMode: ExecutionMode | "signed_product_update";
  currentVersion: string;
  targetVersion: string;
  privilege: LifecyclePrivilege;
  affectedPaths: string[];
  affectedRecords: string[];
  itemIds: string[];
  confidence: string;
  limitations: string[];
}

export function buildLifecyclePlan<TRequest extends LifecyclePlanRequest>(request: TRequest): LifecyclePlan<TRequest> {
  if (request.itemIds?.length) return buildBatchLifecyclePlan(request);
  const evidence = resolvePlanEvidence(request);
  const base = {
    request,
    canonicalId: evidence.canonicalId,
    mappingId: evidence.mappingId,
    resourceId: request.resourceId,
    owner: evidence.owner,
    source: evidence.source,
    currentVersion: evidence.currentVersion,
    targetVersion: evidence.targetVersion,
    privilege: evidence.privilege,
    affectedPaths: evidence.affectedPaths,
    affectedRecords: evidence.affectedRecords,
    confidence: evidence.confidence,
    limitations: evidence.limitations,
    expiresAt: fixtureExpiry,
    revalidation: {
      state: "fresh" as const,
      checkedAt: "2026-08-21T00:00:00+07:00",
      checks: [
        "Recheck canonical identity and authoritative owner",
        "Recheck current resource digest immediately before execution",
        "Reject changed mapping, arguments, targets, privilege, or expiry",
      ],
    },
  };

  if (evidence.executionMode === "vendor_handoff") {
    return finalizeLifecyclePlan({ ...base, execution: { mode: "vendor_handoff", handoffTarget: evidence.owner } }) as LifecyclePlan<TRequest>;
  }
  if (evidence.executionMode === "detect_only") {
    return finalizeLifecyclePlan({ ...base, execution: { mode: "detect_only", guidance: "Inspect publisher guidance and verify the release channel." } }) as LifecyclePlan<TRequest>;
  }
  return finalizeLifecyclePlan({
    ...base,
    execution: {
      mode: evidence.executionMode,
      executable: executableFor(request, evidence),
      argv: argvFor(request, evidence),
    },
  }) as LifecyclePlan<TRequest>;
}

export function buildLifecycleExecution<TPlan extends LifecyclePlan>(plan: TPlan, status: LifecycleExecutionResult["status"]): LifecycleExecutionResult<TPlan> {
  if (plan.execution.mode === "batch") return buildBatchExecution(plan, status);
  const evidence = resolvePlanEvidence(plan.request);
  const itemIds = evidence.itemIds.length ? evidence.itemIds : [plan.resourceId];
  const terminal = status !== "in_progress";
  const items = itemIds.map((id, index) => {
    const itemStatus = !terminal ? (index === 0 ? "success" : "in_progress")
      : status === "cancelled" ? (index === 0 ? "success" : "cancelled")
        : status === "partial" && index === itemIds.length - 1 ? "failed" : "success";
    return {
      id,
      label: id.replaceAll("-", " "),
      status: itemStatus,
      receipt: itemStatus === "success" ? `fixture-receipt:${id}:${plan.digest.slice(-8)}` : undefined,
      redactedDetail: itemStatus === "failed" ? "Target rejected changed evidence; sensitive values redacted." : "Deterministic simulation detail; sensitive values redacted.",
    } as const;
  });
  const vendorHandoff = plan.execution.mode === "vendor_handoff";
  return {
    operationId: `fixture-operation:${plan.resourceId}:${plan.digest.slice(-8)}`,
    planDigest: plan.digest,
    status,
    completedSteps: status === "in_progress" ? Math.min(2, itemIds.length + 1) : itemIds.length + 2,
    totalSteps: itemIds.length + 2,
    canCancel: status === "in_progress",
    receipt: terminal ? `fixture-receipt:${plan.resourceId}:${plan.digest.slice(-8)}` : undefined,
    redactedDetail: vendorHandoff ? "The reviewed handoff was recorded. Execution remains with the vendor." : "The fixture adapter returned redacted lifecycle evidence.",
    items,
    retryActions: vendorHandoff || status === "success" ? [] : [followUp("retry", "Review retry plan", plan)],
    recoveryActions: vendorHandoff || status === "success" ? [] : [followUp("recover", "Review managed recovery plan", plan)],
  };
}

function resolvePlanEvidence(request: LifecyclePlanRequest): ResolvedPlanEvidence {
  if (request.sourceAnalysisHandle) return sourceEvidence(request);
  if (request.resourceKind === "tool") {
    const tool = toolFixtures.find((item) => item.id === request.resourceId || item.packageId === request.resourceId);
    if (tool) return {
      canonicalId: `tool:${tool.id}`, mappingId: `${tool.manager.toLowerCase().replaceAll(" ", "-")}:${tool.packageId}`, owner: tool.owner, source: tool.manager,
      executionMode: tool.executionMode, currentVersion: tool.installedVersion ?? "Not installed", targetVersion: tool.availableVersion ?? "No version change",
      privilege: tool.executionMode === "vendor_handoff" ? "vendor_controlled" : tool.privilege === "required" ? "elevation_required" : "none",
      affectedPaths: tool.executionMode === "vendor_handoff" ? [] : [`manager-package:${tool.packageId}`], affectedRecords: [`inventory:tool:${tool.id}`, `receipt:tool:${tool.id}`], itemIds: [tool.id],
      confidence: tool.lifecycleConfidence, limitations: tool.executionMode === "vendor_handoff" ? ["Vendor execution and recovery remain outside STM claims."] : ["Desktop state must match this reviewed manager mapping."],
    };
  }
  if (request.resourceKind === "skill") {
    const skill = skillFixtures.find((item) => item.id === request.resourceId);
    if (skill) return {
      canonicalId: `skill:${skill.id}`, mappingId: `trusted-skill:${skill.source}:${skill.id}`, owner: "Trusted skill catalog", source: skill.source,
      executionMode: "managed_execute", currentVersion: skill.revision, targetVersion: skill.availableRevision ?? skill.revision, privilege: "user_confirmation",
      affectedPaths: skill.targets.map((target) => target.path), affectedRecords: skill.targets.map((target) => `skill-receipt:${skill.id}:${target.client}`),
      itemIds: skill.targets.map((target) => `${target.client}-${target.state}`.toLowerCase().replaceAll(" ", "-")), confidence: "Catalog provenance and fixture digest matched", limitations: skill.riskFlags.length ? skill.riskFlags : ["No additional content risks reported."],
    };
  }
  if (request.resourceKind === "mcp") {
    const server = mcpFixtures.find((item) => item.id === request.resourceId);
    if (server) return mcpEvidence(server.id, server.source, server.clients.filter((client) => client.state !== "unsupported").map((client) => client.client), server.commandOrUrl);
  }
  if (request.resourceKind === "product") return productEvidence(request);
  if (request.resourceKind === "operation") return operationEvidence(request);
  return fallbackEvidence(request);
}

function sourceEvidence(request: LifecyclePlanRequest): ResolvedPlanEvidence {
  const analyzed = request.sourceAnalysisHandle ? resolveSourceAnalysisHandle(request.sourceAnalysisHandle) : undefined;
  if (request.resourceKind === "tool") {
    const tool = toolFixtures.find((item) => item.id === request.resourceId || slug(item.name) === request.resourceId);
    if (tool) return { ...resolvePlanEvidence({ resourceKind: "tool", action: request.action, resourceId: tool.id }), source: analyzed?.normalizedUrl ?? tool.manager };
  }
  if (request.resourceKind === "skill") {
    const skill = skillFixtures.find((item) => item.id === request.resourceId || slug(item.name) === request.resourceId || `${slug(item.name)}-skill` === request.resourceId);
    if (skill) return { ...resolvePlanEvidence({ resourceKind: "skill", action: request.action, resourceId: skill.id }), source: analyzed?.normalizedUrl ?? skill.source };
  }
  if (request.resourceKind === "mcp") {
    const server = mcpFixtures.find((item) => item.id === request.resourceId);
    if (server) return mcpEvidence(server.id, server.source, server.clients.filter((client) => client.state !== "unsupported").map((client) => client.client), server.commandOrUrl);
  }
  const targets = request.resourceKind === "skill" ? ["Codex", "AgentKit"] : request.resourceKind === "mcp" ? ["Codex", "Claude Code"] : [request.resourceId];
  const source = analyzed?.normalizedUrl ?? `analysis:${request.sourceAnalysisHandle}`;
  if (request.resourceKind === "mcp") return mcpEvidence(request.resourceId, source, targets, "Reviewed HTTPS endpoint");
  return {
    canonicalId: `${request.resourceKind}:${request.resourceId}`, mappingId: `source-review:${request.resourceKind}:${request.resourceId}`, owner: "Source mapping unresolved",
    source, executionMode: "detect_only", currentVersion: "Not installed", targetVersion: "No authorized target", privilege: "none",
    affectedPaths: [], affectedRecords: [], itemIds: targets.map(slug),
    confidence: "Source analysis only", limitations: ["No authoritative managed mapping matched this source; execution remains unavailable."],
  };
}

function mcpEvidence(id: string, source: string, targets: string[], commandOrUrl: string): ResolvedPlanEvidence {
  return {
    canonicalId: `mcp:${id}`, mappingId: `mcp-global:${id}`, owner: "Supported MCP client adapters", source, executionMode: "managed_execute", currentVersion: "Current client configuration", targetVersion: "Reviewed configuration",
    privilege: "user_confirmation", affectedPaths: targets.map((target) => `global-client-config:${target}`), affectedRecords: targets.map((target) => `mcp-binding:${id}:${target}`), itemIds: targets.map(slug),
    confidence: "Supported client schema fixture", limitations: [`Transport evidence: ${commandOrUrl}`, "Credential values are excluded; reference names only."],
  };
}

function productEvidence(request: LifecyclePlanRequest): ResolvedPlanEvidence {
  const update = updateFixtures.find((item) => item.id === request.resourceId || item.resourceType === "product");
  return { canonicalId: "product:stm", mappingId: "signed-product-channel:stm", owner: "Signed product channel", source: "Authenticated STM release endpoint", executionMode: "signed_product_update", currentVersion: update?.current ?? "0.1.0", targetVersion: update?.target ?? "0.2.0", privilege: "user_confirmation", affectedPaths: ["application-bundle:STM"], affectedRecords: ["product-update-receipt:stm"], itemIds: ["signed-package", "restart"], confidence: "Signed package fixture", limitations: ["Restart can fail independently and requires fresh recovery consent."] };
}

function operationEvidence(request: LifecyclePlanRequest): ResolvedPlanEvidence {
  const operation = operationFixtures.find((item) => item.id === request.resourceId);
  const inspectionOnly = request.action === "inspect-receipt" || operation?.action === "Vendor handoff";
  return { canonicalId: `operation:${request.resourceId}`, mappingId: `${inspectionOnly ? "receipt" : "recovery"}:${operation?.receipt ?? request.resourceId}`, owner: operation?.owner ?? "Lifecycle recovery", source: operation?.receipt ?? "redacted receipt", executionMode: inspectionOnly ? "detect_only" : "managed_execute", currentVersion: "Recorded operation state", targetVersion: inspectionOnly ? "No mutation" : "Recovered managed state", privilege: inspectionOnly ? "none" : "user_confirmation", affectedPaths: [], affectedRecords: [operation?.receipt ?? request.resourceId], itemIds: [request.resourceId], confidence: operation ? "Receipt-backed fixture evidence" : "Recovery handle fixture", limitations: operation?.action === "Vendor handoff" ? ["Vendor execution remains outside STM; no rollback capability is claimed."] : [operation?.detail ?? "Original operation detail unavailable."] };
}

function fallbackEvidence(request: LifecyclePlanRequest): ResolvedPlanEvidence {
  return { canonicalId: `${request.resourceKind}:${request.resourceId}`, mappingId: `fixture-mapping:${request.resourceId}`, owner: "Unknown lifecycle owner", source: "Unresolved fixture evidence", executionMode: "detect_only", currentVersion: "Recorded state", targetVersion: "No authorized target", privilege: "none", affectedPaths: [], affectedRecords: [], itemIds: [request.resourceId], confidence: "Unresolved", limitations: ["No authoritative lifecycle mapping matched this semantic request."] };
}

function buildBatchLifecyclePlan<TRequest extends LifecyclePlanRequest>(request: TRequest): LifecyclePlan<TRequest> {
  const items = (request.itemIds ?? []).map((id) => {
    const update = updateFixtures.find((item) => item.id === id);
    if (!update) return buildLifecyclePlan({ resourceKind: "operation", action: "review", resourceId: id });
    if (update.resourceType === "product") return buildLifecyclePlan({ resourceKind: "product", action: "product-update", resourceId: update.id });
    if (update.resourceType === "skill") {
      const skill = skillFixtures.find((item) => item.name === update.name);
      return buildLifecyclePlan({ resourceKind: "skill", action: "update", resourceId: skill?.id ?? slug(update.name) });
    }
    const tool = toolFixtures.find((item) => item.name === update.name);
    return buildLifecyclePlan({ resourceKind: "tool", action: "update", resourceId: tool?.id ?? slug(update.name) });
  }).flatMap((plan) => plan.execution.mode === "batch" ? [] : [plan]) as AtomicLifecyclePlan[];
  return finalizeLifecyclePlan({
    request,
    canonicalId: `batch:${request.resourceId}`,
    mappingId: "batch:independent-child-plans",
    resourceId: request.resourceId,
    owner: "Multiple authoritative owners",
    source: "Independent per-item lifecycle evidence",
    currentVersion: "See each child plan",
    targetVersion: "See each child plan",
    privilege: "user_confirmation",
    affectedPaths: items.flatMap((item) => item.affectedPaths),
    affectedRecords: items.flatMap((item) => item.affectedRecords),
    confidence: "Each child plan resolved independently",
    limitations: ["Execution mode, privilege, command, and recovery stay scoped to each child plan."],
    expiresAt: fixtureExpiry,
    revalidation: { state: "fresh", checkedAt: "2026-08-21T00:00:00+07:00", checks: ["Revalidate every child digest", "Reject the batch if any child evidence changes or expires"] },
    execution: { mode: "batch", items },
  }) as LifecyclePlan<TRequest>;
}

function buildBatchExecution<TPlan extends LifecyclePlan>(plan: TPlan, status: LifecycleExecutionResult["status"]): LifecycleExecutionResult<TPlan> {
  if (plan.execution.mode !== "batch") throw new Error("Batch execution requires a batch plan");
  const batch = plan.execution;
  const terminal = status !== "in_progress";
  const items = batch.items.map((child, index) => {
    const childStatus = !terminal ? (index === 0 ? "success" : "in_progress")
      : status === "cancelled" ? (index === 0 ? "success" : "cancelled")
        : status === "partial" && index === batch.items.length - 1 ? "failed" : "success";
    return {
      id: child.resourceId,
      label: child.canonicalId,
      status: childStatus,
      receipt: childStatus === "success" ? `fixture-receipt:${child.resourceId}:${child.digest.slice(-8)}` : undefined,
      redactedDetail: child.execution.mode === "vendor_handoff"
        ? `Vendor handoff to ${child.execution.handoffTarget}; no rollback claim.`
        : childStatus === "failed"
          ? "Managed child stopped after revalidation; sensitive values redacted."
          : "Managed child returned redacted fixture evidence.",
    } as const;
  });
  const failedManagedItemIds = items.flatMap((item, index) => (
    item.status === "failed" && batch.items[index]?.execution.mode !== "vendor_handoff"
      ? [plan.request.itemIds?.[index] ?? batch.items[index].resourceId]
      : []
  ));
  const canRecover = failedManagedItemIds.length > 0;
  return {
    operationId: `fixture-operation:${plan.resourceId}:${plan.digest.slice(-8)}`,
    planDigest: plan.digest,
    status,
    completedSteps: status === "in_progress" ? 2 : items.length + 2,
    totalSteps: items.length + 2,
    canCancel: status === "in_progress",
    receipt: terminal ? `fixture-receipt:${plan.resourceId}:${plan.digest.slice(-8)}` : undefined,
    redactedDetail: "Every child retained its independently reviewed execution boundary.",
    items,
    retryActions: canRecover ? [batchFollowUp("retry", "Review retry plan for failed managed items", plan, failedManagedItemIds)] : [],
    recoveryActions: canRecover ? [batchFollowUp("recover", "Review managed recovery plan", plan, failedManagedItemIds)] : [],
  };
}

function batchFollowUp(id: string, label: string, plan: LifecyclePlan, itemIds: string[]) {
  return {
    id,
    label,
    planRequest: {
      resourceKind: "operation" as const,
      action: `${id}-batch`,
      resourceId: `follow-up:${plan.resourceId}:${plan.digest.slice(-8)}`,
      itemIds,
    },
  };
}

function followUp(id: string, label: string, plan: LifecyclePlan) {
  return { id, label, planRequest: { resourceKind: "operation" as const, action: id, resourceId: `follow-up:${plan.resourceId}:${plan.digest.slice(-8)}` } };
}

function executableFor(request: LifecyclePlanRequest, evidence: ResolvedPlanEvidence) {
  if (evidence.executionMode === "signed_product_update") return "/Applications/STM.app/Contents/MacOS/stm-updater";
  if (request.resourceKind === "skill") return "/Applications/STM.app/Contents/MacOS/stm-skill-adapter";
  if (request.resourceKind === "mcp") return "/Applications/STM.app/Contents/MacOS/stm-mcp-adapter";
  if (request.resourceKind === "operation") return "/Applications/STM.app/Contents/MacOS/stm-lifecycle-adapter";
  if (evidence.owner.toLowerCase().includes("npm")) return "/usr/local/bin/node";
  return "/opt/homebrew/bin/brew";
}

function argvFor(request: LifecyclePlanRequest, evidence: ResolvedPlanEvidence) {
  if (evidence.executionMode === "signed_product_update") return ["apply", "--release", evidence.targetVersion];
  if (request.resourceKind === "skill") return [request.action, "--skill", request.resourceId, "--target", evidence.targetVersion];
  if (request.resourceKind === "mcp") return [request.action, "--server", request.resourceId, "--scope", "global"];
  if (request.resourceKind === "operation") return [request.action, "--operation", request.resourceId];
  if (evidence.owner.toLowerCase().includes("npm")) return ["/usr/local/lib/node_modules/npm/bin/npm-cli.js", "install", "--global", `${evidence.mappingId.slice("npm:".length)}@${evidence.targetVersion}`];
  return [request.action === "install" ? "install" : "upgrade", request.resourceId];
}

type LifecyclePlanWithoutIdentity = LifecyclePlan extends infer TPlan
  ? TPlan extends LifecyclePlan
    ? Omit<TPlan, "planId" | "digest">
    : never
  : never;

function finalizeLifecyclePlan<TPlan extends LifecyclePlanWithoutIdentity>(plan: TPlan): TPlan & Pick<LifecyclePlan, "planId" | "digest"> {
  const digestPrefix = plan.execution.mode === "batch" ? "sha256:fixture-batch-" : "sha256:fixture-";
  const digest = `${digestPrefix}${stableToken(JSON.stringify(plan))}`;
  const planId = `fixture-plan:${stableToken(JSON.stringify({ digest, request: plan.request, canonicalId: plan.canonicalId }))}`;
  return { ...plan, planId, digest };
}

function stableToken(value: string) { let hash = 2166136261; for (let index = 0; index < value.length; index += 1) { hash ^= value.charCodeAt(index); hash = Math.imul(hash, 16777619); } return (hash >>> 0).toString(16).padStart(8, "0"); }
function slug(value: string) { return value.toLowerCase().replaceAll(" ", "-"); }
