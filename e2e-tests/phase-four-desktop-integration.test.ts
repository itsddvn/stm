import { beforeEach, describe, expect, it, vi } from "vitest";

beforeEach(() => {
  vi.resetModules();
  Reflect.deleteProperty(globalThis, "window");
});

describe("phase four desktop IPC integration", () => {
  it("keeps the browser fixture adapter deterministic when Tauri is unavailable", async () => {
    const { createRuntimeIpcClient } = await import("../src/lib/ipc/runtime-ipc-client");
    const client = createRuntimeIpcClient();

    expect(client.isDesktop()).toBe(false);
    await expect(client.getAppView("success")).resolves.toMatchObject({
      surface: { loadState: "ready", freshness: "fresh" },
    });
    await expect(client.analyzeSource("tool", "https://github.com/openai/codex")).resolves.toMatchObject({
      status: "review_ready",
    });
  });

  it("invokes the typed desktop commands with stable payloads", async () => {
    const invoke = vi.fn(async (cmd: string, args: unknown) => {
      switch (cmd) {
        case "refresh_snapshot":
          return { surface: { loadState: "loading", freshness: "unknown" }, tools: [], skills: [], mcpServers: [], updates: [], operations: [] };
        case "refresh_status":
          return {
            surface: { loadState: "ready", freshness: "fresh" },
            lastSnapshotAt: "2026-08-20T09:00:00+07:00",
            warningCount: 0,
            warnings: [],
            inProgress: false,
            canCancel: false,
            stepsCompleted: 7,
            totalSteps: 7,
            result: "success",
          };
        case "cancel_operation":
          return true;
        case "run_diagnostics":
          return {
            uiContract: { version: "1.0.0", locked: true },
            storage: { path: "/Users/test/Library/stm.sqlite", recoveredFromCorruption: false, lastGoodAvailable: true },
            catalogVersion: "2026.08.20",
            managers: [],
            skills: { roots: [] },
            mcp: { servers: [] },
            warnings: [],
          };
        case "analyze_source":
          return {
            kind: "tool",
            submittedUrl: "https://github.com/openai/codex",
            status: "review_ready",
            detectedName: "Codex CLI",
            sourceHost: "github.com",
            sourceType: "release",
            publisher: "OpenAI",
            target: "tool",
            trust: "catalog_match",
            riskFlags: [],
            notes: [],
          };
        default:
          throw new Error(`unexpected command: ${cmd} ${JSON.stringify(args)}`);
      }
    });

    Object.assign(globalThis, {
      window: {
        __TAURI_INTERNALS__: { invoke },
      },
    });

    const { createRuntimeIpcClient } = await import("../src/lib/ipc/runtime-ipc-client");
    const client = createRuntimeIpcClient();

    expect(client.isDesktop()).toBe(true);
    await client.startRefresh();
    await client.getRefreshStatus();
    await client.cancelRefresh("inventory-refresh-1");
    await client.runDiagnostics();
    await client.analyzeSource("tool", "https://github.com/openai/codex");

    expect(invoke).toHaveBeenNthCalledWith(1, "refresh_snapshot", {});
    expect(invoke).toHaveBeenNthCalledWith(2, "refresh_status", {});
    expect(invoke).toHaveBeenNthCalledWith(3, "cancel_operation", { operationId: "inventory-refresh-1" });
    expect(invoke).toHaveBeenNthCalledWith(4, "run_diagnostics", {});
    expect(invoke).toHaveBeenNthCalledWith(5, "analyze_source", {
      kind: "tool",
      url: "https://github.com/openai/codex",
    });
  });

  it("redacts user paths from diagnostics exports", async () => {
    const { redactSensitiveText, summarizeDiagnostics } = await import("../src/app/desktop-runtime-controller");

    expect(redactSensitiveText("/Users/alice/.codex/skills")).toBe("/Users/<user>/.codex/skills");
    expect(redactSensitiveText("/home/bob/.agents/skills")).toBe("/home/<user>/.agents/skills");
    expect(redactSensitiveText("C:\\Users\\carol\\stm\\db.sqlite")).toBe("C:\\Users\\<user>\\stm\\db.sqlite");

    const summary = summarizeDiagnostics({
      uiContract: { version: "1.0.0", locked: true },
      storage: { path: "/Users/alice/Library/Application Support/stm/snapshots.sqlite", recoveredFromCorruption: false, lastGoodAvailable: true },
      catalogVersion: "2026.08.20",
      managers: [{ manager: "homebrew", status: "success", packages: [{ id: "git" }] }],
      skills: {
        roots: [{
          client: "Codex",
          declaredRoot: "/Users/alice/.codex/skills",
          canonicalRoot: "/Users/alice/.codex/skills",
          accepted: true,
        }],
      },
      mcp: { servers: [] },
      warnings: ["root:/Users/alice/.codex/skills"],
    });

    expect(summary).toContain("/Users/<user>/.codex/skills");
    expect(summary).not.toContain("/Users/alice");
  });
});
