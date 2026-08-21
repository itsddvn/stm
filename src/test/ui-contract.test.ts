import { describe, expect, expectTypeOf, it, vi } from "vitest";
import { routes } from "../../contracts/ui/route-contract";
import { scenarioIds } from "../../contracts/ui/state-contract";
import type { AppViewModel } from "../../contracts/ui/view-model-contract";
import matrix from "../../tests/fixtures/ui-contract/scenario-matrix.json";
import { activateSkipLink } from "../app/app-shell";
import { isLifecycleConsentEligible, lifecycleConsentEvidenceKey, lifecycleStageForResult } from "../components/use-lifecycle-operation";
import { buildLifecycleExecution, buildLifecyclePlan } from "../fixtures/lifecycle-fixtures";
import { buildScenarioFixture } from "../fixtures/scenario-fixtures";
import { sourceReanalysisKind } from "../features/history/history-page";
import {
  buildSkillResolutionActions,
  buildToolPrimaryAction,
} from "../fixtures/presentation-action-fixtures";
import { mockIpcClient } from "../lib/ipc/mock-ipc-client";

function authorize(plan: { digest: string; expiresAt: string }) {
  return {
    planDigest: plan.digest,
    planExpiresAt: plan.expiresAt,
    grantedAt: "2026-08-21T10:00:00+07:00",
  };
}

describe("fixture-backed UI contract", () => {
  it("has one deterministic fixture for every scenario", () => {
    expect(matrix.contractVersion).toBe("1.1.0-review");
    expect(matrix.scenarios.map((scenario) => scenario.id)).toEqual(scenarioIds);
    for (const scenario of scenarioIds) {
      expect(buildScenarioFixture(scenario)).toEqual(buildScenarioFixture(scenario));
    }
  });

  it("exposes MCP as a first-class route with typed fixture actions", () => {
    expect(routes.map((route) => route.id)).toEqual([
      "dashboard",
      "tools",
      "skills",
      "mcp",
      "updates",
      "history",
      "settings",
    ]);
    expect(routes.map((route) => route.id)).toContain("mcp");
    const view = buildScenarioFixture("success");
    expect(view.mcpServers.length).toBeGreaterThan(0);
    expect(view.mcpServers.every((server) =>
      server.primaryAction.presentationOnly &&
      server.toggleAction.presentationOnly &&
      server.removeAction.presentationOnly
    )).toBe(true);
    expect(view.mcpServers.find((server) => server.id === "github")?.toggleAction).toMatchObject({
      id: "mcp.review_disable",
      enabled: true,
    });
    expect(view.mcpServers.find((server) => server.id === "postgres")?.primaryAction).toMatchObject({
      enabled: false,
      disabledReasonCode: "action.mcp.auth_reference_missing",
    });
    expect(view.mcpServers.find((server) => server.id === "postgres")?.toggleAction).toMatchObject({
      id: "mcp.review_enable",
      enabled: false,
      disabledReasonCode: "action.mcp.auth_reference_missing",
    });
  });

  it("analyzes source URLs only through the typed fixture IPC boundary", async () => {
    const accepted = await mockIpcClient.analyzeSource("skill", "https://github.com/agentkit/skills/tree/main/frontend-design");
    expect(accepted).toMatchObject({
      status: "review_ready",
      trust: "catalog_match",
      detectedName: "Frontend Design skill",
    });
    const blocked = await mockIpcClient.analyzeSource("tool", "http://user:secret@example.com/tool");
    expect(blocked).toMatchObject({
      status: "blocked",
      trust: "blocked",
      riskFlags: ["Source validation failed"],
    });
    expect(JSON.stringify(blocked)).not.toContain("secret");
    const queryCredential = await mockIpcClient.analyzeSource(
      "mcp",
      "https://mcp.example.com/mcp?access_token=super-secret#secret-fragment",
    );
    expect(queryCredential).toMatchObject({
      status: "blocked",
      submittedUrl: "https://mcp.example.com/mcp",
    });
    expect(JSON.stringify(queryCredential)).not.toContain("super-secret");
  });

  it("keeps every queued update unselected", () => {
    expect(buildScenarioFixture("success").updates.every((update) => update.selected === false)).toBe(true);
  });

  it("gates denied and non-managed tool flows through typed presentation actions", () => {
    expect(buildScenarioFixture("unsupported").tools[0].primaryAction).toMatchObject({
      enabled: false,
      disabledReasonCode: "action.mapping.unsupported",
      presentationOnly: true,
    });
    expect(buildScenarioFixture("blocked").tools[0].primaryAction).toMatchObject({
      enabled: false,
      disabledReasonCode: "action.mapping.blocked",
    });
    expect(buildScenarioFixture("manager_unavailable").tools[0].primaryAction).toMatchObject({
      enabled: false,
      disabledReasonCode: "action.manager.unavailable",
    });
    expect(buildToolPrimaryAction({
      state: "unknown",
      executionMode: "managed_execute",
      ownershipKind: "unknown",
      installedVersion: "1.0.0",
      availableVersion: "1.1.0",
    })).toMatchObject({
      enabled: false,
      disabledReasonCode: "action.execution.unknown",
    });
    expect(buildToolPrimaryAction({
      state: "managed_current",
      executionMode: "managed_execute",
      ownershipKind: "system_owned",
      installedVersion: "1.0.0",
      availableVersion: "1.0.0",
    })).toMatchObject({
      enabled: false,
      disabledReasonCode: "action.execution.system_owned",
    });
    const handoffAction = buildScenarioFixture("success").tools.find((tool) => tool.id === "orca-ade")?.primaryAction;
    expect(handoffAction).toMatchObject({
      id: "tool.review_vendor_handoff",
      enabled: true,
      presentationOnly: true,
    });
    const guidanceAction = buildScenarioFixture("success").tools.find((tool) => tool.id === "oh-my-pi")?.primaryAction;
    expect(guidanceAction).toMatchObject({
      id: "tool.review_guidance",
      enabled: true,
      presentationOnly: true,
    });
  });

  it("blocks conflicted skill updates from the generic queue until conflict actions are chosen", () => {
    const view = buildScenarioFixture("success");
    const blockedUpdate = view.updates.find((update) => update.id === "update-release-pilot");
    expect(blockedUpdate?.selectionAction).toMatchObject({
      enabled: false,
      disabledReasonCode: "action.update.conflict_resolution_required",
      presentationOnly: true,
    });
    const modifiedSkill = view.skills.find((skill) => skill.id === "release-pilot");
    expect(modifiedSkill?.primaryAction.id).toBe("skill.resolve_local_modification");
    expect(modifiedSkill?.resolutionActions.map((action) => action.id)).toEqual([
      "skill.keep_local",
      "skill.export_diff",
      "skill.restore_managed",
      "skill.install_side_by_side",
    ]);
    expect(modifiedSkill?.resolutionActions.at(-1)).toMatchObject({
      enabled: false,
      disabledReasonCode: "action.skill.side_by_side_unsupported",
    });
    expect(
      buildSkillResolutionActions(
        {
          state: "modified",
          availableRevision: "v2",
          targets: [{ client: "Codex", path: "skill", state: "modified" }],
        },
        true,
      ).at(-1),
    ).toMatchObject({
      id: "skill.install_side_by_side",
      enabled: true,
    });
  });

  it("keeps the mock IPC contract typed with presentation actions on every view model", async () => {
    const view = await mockIpcClient.getAppView("success");
    expectTypeOf(view).toMatchTypeOf<AppViewModel>();
    expect(view.tools.every((tool) => tool.primaryAction.presentationOnly)).toBe(true);
    expect(view.skills.every((skill) => skill.primaryAction.presentationOnly && Array.isArray(skill.resolutionActions))).toBe(true);
    expect(
      view.updates.every((update) =>
        update.resourceType === "product"
          ? update.reviewAction?.presentationOnly === true
          : update.selectionAction?.presentationOnly === true,
      ),
    ).toBe(true);
  });

  it("keeps the skip link out of the hash router and moves focus to main content", () => {
    const preventDefault = vi.fn();
    const focus = vi.fn();

    activateSkipLink(
      { preventDefault },
      { getElementById: (id) => (id === "main-content" ? { focus } : null) },
    );

    expect(preventDefault).toHaveBeenCalledTimes(1);
    expect(focus).toHaveBeenCalledTimes(1);
  });

  it("resolves exact lifecycle evidence behind the typed IPC boundary", async () => {
    const plan = await mockIpcClient.prepareLifecycle({ resourceKind: "tool", action: "update", resourceId: "codex-cli" });
    expect(plan).toMatchObject({
      canonicalId: "tool:codex-cli",
      mappingId: "npm:@openai/codex",
      owner: "npm",
      source: "npm",
      currentVersion: "0.31.0",
      targetVersion: "0.32.1",
      privilege: "none",
      revalidation: { state: "fresh" },
      execution: { mode: "managed_execute", executable: "/usr/local/bin/node", argv: ["/usr/local/lib/node_modules/npm/bin/npm-cli.js", "install", "--global", "@openai/codex@0.32.1"] },
    });
    expect(plan.digest).toMatch(/^sha256:fixture-/);
    expect(plan.planId).toMatch(/^fixture-plan:/);
    expect(plan.affectedRecords.length).toBeGreaterThan(0);
    expect(plan.limitations.length).toBeGreaterThan(0);
    expect(isLifecycleConsentEligible(plan, Date.parse("2026-08-21T10:00:00+07:00"))).toBe(true);
    expect(isLifecycleConsentEligible({ ...plan, expiresAt: "2026-08-21T09:30:00+07:00" }, Date.parse("2026-08-21T10:00:00+07:00"))).toBe(false);
    expect(isLifecycleConsentEligible({ ...plan, revalidation: { ...plan.revalidation, state: "evidence_changed" } })).toBe(false);
    const changedChecks = { ...plan, revalidation: { ...plan.revalidation, checks: [...plan.revalidation.checks, "Recheck package signature"] } };
    expect(lifecycleConsentEvidenceKey(changedChecks)).not.toBe(lifecycleConsentEvidenceKey(plan));
    expect(lifecycleConsentEvidenceKey({ ...plan, digest: "sha256:changed" })).not.toBe(lifecycleConsentEvidenceKey(plan));
    expect(lifecycleConsentEvidenceKey({
      ...plan,
      revalidation: { ...plan.revalidation, checkedAt: "2026-08-21T00:01:00+07:00" },
    })).not.toBe(lifecycleConsentEvidenceKey(plan));
    await expect(mockIpcClient.startLifecycle(plan.planId, { ...authorize(plan), planDigest: "sha256:changed" })).rejects.toThrow(/does not match/);
    await expect(mockIpcClient.startLifecycle(plan.planId, { ...authorize(plan), grantedAt: "not-a-date" })).rejects.toThrow(/stale or expired/);
    await expect(mockIpcClient.startLifecycle("fixture-plan:unknown", authorize(plan))).rejects.toThrow(/unknown or expired/i);
  });

  it("keeps lifecycle controls available until status becomes terminal", async () => {
    const plan = await mockIpcClient.prepareLifecycle({ resourceKind: "tool", action: "update", resourceId: "codex-cli" });
    expect(lifecycleStageForResult(buildLifecycleExecution(plan, "in_progress"))).toBe("progress");
    expect(lifecycleStageForResult(buildLifecycleExecution(plan, "success"))).toBe("result");
  });

  it("keeps vendor handoff plans free of rollback and managed-command claims", async () => {
    const plan = await mockIpcClient.prepareLifecycle({ resourceKind: "tool", action: "update", resourceId: "orca-ade" });
    expect(plan.execution).toEqual({ mode: "vendor_handoff", handoffTarget: "Vendor updater" });
    expect(plan.execution).not.toHaveProperty("executable");
    expect(plan.execution).not.toHaveProperty("argv");
    const progress = await mockIpcClient.startLifecycle(plan.planId, authorize(plan));
    const result = await mockIpcClient.cancelLifecycle(progress.operationId);
    expect(result.retryActions).toEqual([]);
    expect(result.recoveryActions).toEqual([]);
    expect(result.redactedDetail).not.toMatch(/rollback/i);
    const vendorHistory = buildScenarioFixture("success").operations.find((operation) => operation.id === "op-2472");
    expect(vendorHistory?.lifecycleRequest).toEqual({ resourceKind: "operation", action: "inspect-receipt", resourceId: "op-2472" });
    const historyPlan = await mockIpcClient.prepareLifecycle(vendorHistory!.lifecycleRequest);
    expect(historyPlan.execution.mode).toBe("detect_only");
    expect(historyPlan.execution).not.toHaveProperty("executable");
  });

  it("routes persisted source recovery to source analysis before planning", () => {
    expect(sourceReanalysisKind({
      resourceKind: "tool",
      action: "reanalyze-source",
      resourceId: "codex-cli",
    })).toBe("tool");
    expect(sourceReanalysisKind({
      resourceKind: "operation",
      action: "inspect-receipt",
      resourceId: "op-2472",
    })).toBeNull();
  });

  it("keeps redacted per-item receipts available for history review", () => {
    const history = buildScenarioFixture("success").operations;
    const partial = history.find((operation) => operation.id === "op-2479");
    expect(partial?.details).toHaveLength(2);
    expect(partial?.details[0]).toContain("success");
    expect(partial?.details[1]).toContain("failed");
    expect(partial?.details.join("\n")).not.toContain("/Users/");
  });

  it("keeps heterogeneous bulk updates as independent child plans", async () => {
    const plan = await mockIpcClient.prepareLifecycle({
      resourceKind: "operation",
      action: "update-queue",
      resourceId: "selected-update-queue",
      itemIds: ["update-orca", "update-codex", "update-frontend-design"],
    });
    expect(plan.execution.mode).toBe("batch");
    if (plan.execution.mode !== "batch") throw new Error("Expected batch plan");
    expect(plan.execution.items.map((item) => item.execution.mode)).toEqual(["vendor_handoff", "managed_execute", "managed_execute"]);
    expect(plan.execution.items[0].execution).not.toHaveProperty("executable");
    expect(plan.execution.items[1].execution).toMatchObject({ executable: "/usr/local/bin/node" });
    expect(plan.execution.items[2].execution).toMatchObject({ executable: "/Applications/STM.app/Contents/MacOS/stm-skill-adapter" });
    const changedChild = { ...plan.execution.items[1], revalidation: { ...plan.execution.items[1].revalidation, state: "evidence_changed" as const } };
    const invalidBatch = { ...plan, execution: { ...plan.execution, items: [plan.execution.items[0], changedChild, plan.execution.items[2]] } };
    expect(isLifecycleConsentEligible(invalidBatch)).toBe(false);
    expect(lifecycleConsentEvidenceKey(invalidBatch)).not.toBe(lifecycleConsentEvidenceKey(plan));
    const progress = await mockIpcClient.startLifecycle(plan.planId, authorize(plan));
    expect(progress.items).toHaveLength(3);
    const partial = buildLifecycleExecution(plan, "partial");
    expect(partial.recoveryActions[0].planRequest.itemIds).toEqual(["update-frontend-design"]);
    const recoveryPlan = await mockIpcClient.prepareLifecycle(partial.recoveryActions[0].planRequest);
    expect(recoveryPlan.execution.mode).toBe("batch");
    if (recoveryPlan.execution.mode !== "batch") throw new Error("Expected filtered recovery batch");
    expect(recoveryPlan.execution.items.map((item) => item.canonicalId)).toEqual(["skill:frontend-design"]);
  });

  it("returns follow-up guidance that prepares a fresh consent-bound plan", async () => {
    const original = await mockIpcClient.prepareLifecycle({ resourceKind: "product", action: "product-update", resourceId: "update-product" });
    const progress = await mockIpcClient.startLifecycle(original.planId, authorize(original));
    const result = await mockIpcClient.getLifecycleStatus(progress.operationId);
    expect(result.status).toBe("recoverable");
    const recovery = result.recoveryActions[0];
    expect(recovery.label).toMatch(/review/i);
    const followUp = await mockIpcClient.prepareLifecycle(recovery.planRequest);
    expect(followUp.digest).not.toBe(original.digest);
    expect(followUp.revalidation.state).toBe("fresh");
    expect(followUp.request.action).toBe("recover");
  });

  it("keeps source analysis lifecycle requests semantic and policy-free", async () => {
    const analysis = await mockIpcClient.analyzeSource("tool", "https://github.com/openai/codex");
    expect(analysis.lifecycleRequest).toMatchObject({
      resourceKind: "tool",
      action: "install",
      resourceId: "codex-cli",
    });
    expect(analysis.lifecycleRequest.sourceAnalysisHandle).toMatch(/^fixture-source:tool:codex-cli:[a-f0-9]{8}$/);
    expect(analysis.lifecycleRequest).not.toHaveProperty("executionMode");
    expect(analysis.lifecycleRequest).not.toHaveProperty("privilege");
    expect(analysis.lifecycleRequest).not.toHaveProperty("affectedPaths");
    const plan = buildLifecyclePlan(analysis.lifecycleRequest);
    expect(plan.source).toBe("https://github.com/openai/codex");
    expect(plan.execution).toMatchObject({
      mode: "managed_execute",
      executable: "/usr/local/bin/node",
      argv: ["/usr/local/lib/node_modules/npm/bin/npm-cli.js", "install", "--global", "@openai/codex@0.32.1"],
    });
  });
});
