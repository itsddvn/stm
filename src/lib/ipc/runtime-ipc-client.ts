import type { ScenarioId } from "../../../contracts/ui/state-contract";
import type { AppViewModel, OperationViewModel, SourceAnalysisViewModel, SourceKind } from "../../../contracts/ui/view-model-contract";
import type { LifecycleConsentAuthorization, LifecycleExecutionResult, LifecyclePlan, LifecyclePlanRequest } from "../../../contracts/ui/lifecycle-contract";
import type { InstallProviderPreference, MigrationCandidate, PortableImportResult, QuickSetupView } from "../../../contracts/ui/setup-contract";
import type { ToolViewModel } from "../../../contracts/ui/view-model-contract";
import { buildQuickSetupView, dismissQuickSetup, saveSetupPreference } from "../../fixtures/setup-fixtures";
import { isQuickSetupDismissed } from "../../fixtures/setup-fixtures";
import { validatePortableSetupText } from "../../fixtures/portable-validate";
import { getCurrentLocale } from "../i18n";
import { mockIpcClient, type ToolsManagerIpcClient } from "./mock-ipc-client";

export interface RefreshStatus {
  surface: AppViewModel["surface"];
  lastSnapshotAt: string;
  warningCount: number;
  warnings: string[];
  inProgress: boolean;
  canCancel: boolean;
  operationId?: string;
  currentStep?: string;
  stepsCompleted: number;
  totalSteps: number;
  snapshot?: AppViewModel;
  result?: string;
  errorMessage?: string;
}

interface DiagnosticsManagerReport {
  manager: string;
  status: string;
  packages: Array<{ id: string }>;
}

interface DiagnosticsSkillRoot {
  client: "Codex" | "Claude Code" | "AgentKit";
  declaredRoot: string;
  canonicalRoot?: string;
  accepted: boolean;
}

export interface DiagnosticsReport {
  uiContract: { version: string; locked: boolean };
  storage: { path: string; recoveredFromCorruption: boolean; lastGoodAvailable: boolean };
  catalogVersion: string;
  managers: DiagnosticsManagerReport[];
  skills: { roots: DiagnosticsSkillRoot[] };
  mcp: { servers: Array<{ clients: Array<{ client: string; state: string }> }> };
  warnings: string[];
}

export interface RuntimeIpcClient extends ToolsManagerIpcClient {
  isDesktop(): boolean;
  startRefresh(): Promise<AppViewModel>;
  getRefreshStatus(): Promise<RefreshStatus>;
  cancelRefresh(operationId: string): Promise<boolean>;
  listOperations(): Promise<OperationViewModel[]>;
  runDiagnostics(): Promise<DiagnosticsReport>;
  getQuickSetup(tools: ToolViewModel[]): Promise<QuickSetupView>;
  getSetupPreferences(): Promise<{ quickSetupDismissed: boolean }>;
  validatePortableSetup(text: string): Promise<string[]>;
  importPortableSetup(): Promise<PortableImportResult | null>;
  exportPortableSetup(target: string): Promise<string | null>;
  getMigrationCandidates(): Promise<MigrationCandidate[]>;
  setProviderPreference(preference: InstallProviderPreference): Promise<void>;
  dismissQuickSetup(): Promise<void>;
}

declare global {
  interface Window {
    __TAURI_INTERNALS__?: {
      invoke<T>(cmd: string, args?: unknown, options?: unknown): Promise<T>;
    };
  }
}

function hasTauriInvoke() {
  return typeof window !== "undefined" && typeof window.__TAURI_INTERNALS__?.invoke === "function";
}

export function isFixtureRuntime() {
  return !hasTauriInvoke();
}

class FixtureRuntimeClient implements RuntimeIpcClient {
  isDesktop() {
    return false;
  }

  async getAppView(scenario: ScenarioId) {
    return mockIpcClient.getAppView(scenario);
  }

  async startRefresh() {
    return mockIpcClient.getAppView("loading");
  }

  async getRefreshStatus(): Promise<RefreshStatus> {
    const snapshot = await mockIpcClient.getAppView("success");
    return {
      surface: snapshot.surface,
      lastSnapshotAt: "2026-08-20T09:00:00+07:00",
      warningCount: 0,
      warnings: [],
      inProgress: false,
      canCancel: false,
      stepsCompleted: 7,
      totalSteps: 7,
      snapshot,
      result: "success",
    };
  }

  async listOperations(): Promise<OperationViewModel[]> {
    return (await mockIpcClient.getAppView("success")).operations;
  }

  async cancelRefresh() {
    return false;
  }

  async runDiagnostics(): Promise<DiagnosticsReport> {
    return {
      uiContract: { version: "1.1.0", locked: false },
      storage: {
        path: "/Users/<user>/Library/Application Support/stm/snapshots.sqlite",
        recoveredFromCorruption: false,
        lastGoodAvailable: true,
      },
      catalogVersion: "2026.08.20",
      managers: [
        { manager: "homebrew", status: "success", packages: [{ id: "git" }] },
        { manager: "winget", status: "manager_unavailable", packages: [] },
        { manager: "apt", status: "empty", packages: [] },
      ],
      skills: {
        roots: [
          {
            client: "Codex",
            declaredRoot: "/Users/<user>/.codex/skills",
            canonicalRoot: "/Users/<user>/.codex/skills",
            accepted: true,
          },
          {
            client: "Claude Code",
            declaredRoot: "/Users/<user>/.claude/skills",
            canonicalRoot: "/Users/<user>/.claude/skills",
            accepted: true,
          },
          {
            client: "AgentKit",
            declaredRoot: "/Users/<user>/.agents/skills",
            canonicalRoot: "/Users/<user>/.codex/skills",
            accepted: true,
          },
        ],
      },
      mcp: { servers: [{ clients: [{ client: "Codex", state: "enabled" }] }] },
      warnings: [],
    };
  }

  async analyzeSource(kind: SourceKind, url: string): Promise<SourceAnalysisViewModel> {
    return mockIpcClient.analyzeSource(kind, url);
  }

  async prepareLifecycle<TRequest extends LifecyclePlanRequest>(request: TRequest) {
    return mockIpcClient.prepareLifecycle(request);
  }

  async startLifecycle(planId: string, authorization: LifecycleConsentAuthorization) {
    return mockIpcClient.startLifecycle(planId, authorization);
  }

  async getLifecycleStatus(operationId: string) {
    return mockIpcClient.getLifecycleStatus(operationId);
  }

  async cancelLifecycle(operationId: string) {
    return mockIpcClient.cancelLifecycle(operationId);
  }

  async getQuickSetup(tools: ToolViewModel[]) {
    return buildQuickSetupView(tools);
  }

  async setProviderPreference(preference: InstallProviderPreference) {
    saveSetupPreference(preference);
  }

  async dismissQuickSetup() {
    dismissQuickSetup();
  }

  async getSetupPreferences() {
    return { quickSetupDismissed: isQuickSetupDismissed() };
  }

  async validatePortableSetup(text: string) {
    return validatePortableSetupText(text, "macos_arm64").warnings;
  }

  async importPortableSetup() {
    return null;
  }

  async exportPortableSetup(_target: string) {
    return null;
  }


  async getMigrationCandidates(): Promise<MigrationCandidate[]> {
    return [{
      recipe: {
        id: "codex-npm-to-homebrew",
        resourceId: "codex-cli",
        sourceMappingId: "npm:@openai/codex",
        targetMappingId: "homebrew:codex",
        targetExecutablePaths: ["/opt/homebrew/bin/codex", "/usr/local/bin/codex"],
        sharedConfigIds: ["codex-home"],
        cleanupOldOwnerDefault: true,
      },
      sourceOwner: "npm",
      targetOwner: "Homebrew",
      cleanupOldOwner: true,
    }];
  }
}

class TauriRuntimeClient implements RuntimeIpcClient {
  isDesktop() {
    return true;
  }

  async getAppView(_: ScenarioId): Promise<AppViewModel> {
    const status = await this.getRefreshStatus();
    return status.snapshot ?? this.startRefresh();
  }

  async startRefresh(): Promise<AppViewModel> {
    return this.invoke<AppViewModel>("refresh_snapshot");
  }

  async getRefreshStatus(): Promise<RefreshStatus> {
    return this.invoke<RefreshStatus>("refresh_status");
  }

  async cancelRefresh(operationId: string): Promise<boolean> {
    return this.invoke<boolean>("cancel_operation", { operationId });
  }

  async listOperations(): Promise<OperationViewModel[]> {
    return this.invoke<OperationViewModel[]>("list_operations");
  }

  async runDiagnostics(): Promise<DiagnosticsReport> {
    return this.invoke<DiagnosticsReport>("run_diagnostics");
  }

  async analyzeSource(kind: SourceKind, url: string): Promise<SourceAnalysisViewModel> {
    return this.invoke<SourceAnalysisViewModel>("analyze_source", { kind, url });
  }

  async prepareLifecycle<TRequest extends LifecyclePlanRequest>(request: TRequest) {
    return this.invoke<LifecyclePlan<TRequest>>("prepare_lifecycle_plan", { request });
  }

  async startLifecycle(planId: string, authorization: LifecycleConsentAuthorization): Promise<LifecycleExecutionResult> {
    return this.invoke<LifecycleExecutionResult>("start_lifecycle_operation", { planId, authorization, locale: getCurrentLocale() });
  }

  async getLifecycleStatus(operationId: string): Promise<LifecycleExecutionResult> {
    return this.invoke<LifecycleExecutionResult>("lifecycle_operation_status", { operationId });
  }

  async cancelLifecycle(operationId: string): Promise<LifecycleExecutionResult> {
    return this.invoke<LifecycleExecutionResult>("cancel_lifecycle_operation", { operationId });
  }

  async getQuickSetup(_tools: ToolViewModel[]): Promise<QuickSetupView> {
    const view = await this.invoke<{
      target: string;
      preference: InstallProviderPreference;
      dismissed: boolean;
      providers: { homebrew?: { path: string }; bun?: { path: string }; npm?: { path: string } };
      tools: QuickSetupView["tools"];
      optionalMcp: QuickSetupView["optional"];
    }>("get_quick_setup");
    return {
      target: view.target,
      preference: view.preference,
      dismissed: view.dismissed,
      providers: {
        homebrew: view.providers.homebrew?.path,
        bun: view.providers.bun?.path,
        npm: view.providers.npm?.path,
      },
      tools: view.tools,
      optional: view.optionalMcp,
    };
  }

  async setProviderPreference(preference: InstallProviderPreference) {
    await this.invoke("set_provider_preference", { preference });
  }

  async dismissQuickSetup() {
    await this.invoke("dismiss_quick_setup");
  }

  async getSetupPreferences() {
    return this.invoke<{ quickSetupDismissed: boolean }>("get_setup_preferences");
  }

  async validatePortableSetup(text: string) {
    return this.invoke<string[]>("validate_portable_setup", { bytes: text });
  }

  async importPortableSetup() {
    return this.invoke<PortableImportResult | null>("import_portable_setup");
  }

  async exportPortableSetup(target: string) {
    return this.invoke<string | null>("export_portable_setup", { target });
  }

  async getMigrationCandidates() {
    return this.invoke<MigrationCandidate[]>("get_migration_candidates");
  }


  private invoke<T>(cmd: string, args: Record<string, unknown> = {}) {
    const invoke = window.__TAURI_INTERNALS__?.invoke;
    if (!invoke) {
      return Promise.reject(new Error(`Tauri IPC unavailable for command: ${cmd}`));
    }
    return invoke<T>(cmd, args);
  }
}

export function createRuntimeIpcClient(): RuntimeIpcClient {
  return hasTauriInvoke() ? new TauriRuntimeClient() : new FixtureRuntimeClient();
}

export const runtimeIpcClient = createRuntimeIpcClient();
