import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppViewModel } from "../../contracts/ui/view-model-contract";
import type { DiagnosticsReport, RefreshStatus, RuntimeIpcClient } from "../lib/ipc/runtime-ipc-client";

const AUTO_REFRESH_MS = 60_000;
const STATUS_POLL_MS = 120;

interface ControllerOptions {
  client: RuntimeIpcClient;
  onView: (view: AppViewModel) => void;
  doc?: Document;
  win?: Window;
}

interface RuntimeState {
  autoCheck: boolean;
  diagnosticsEnabled: boolean;
  refreshInProgress: boolean;
  currentOperationId?: string;
  latestView?: AppViewModel;
  latestDiagnostics?: DiagnosticsReport;
}

export interface DesktopRuntimeController {
  start: () => Promise<void>;
  refresh: () => Promise<void>;
  dispose: () => void;
}

export function createDesktopRuntimeController({
  client,
  onView,
  doc = document,
  win = window,
}: ControllerOptions): DesktopRuntimeController {
  const runtime: RuntimeState = {
    autoCheck: true,
    diagnosticsEnabled: false,
    refreshInProgress: false,
  };

  let active = true;
  let autoRefreshTimer: number | undefined;
  let statusPollTimer: number | undefined;
  let stopRefreshEvents: UnlistenFn | undefined;

  async function start() {
    bindDomEvents();
    if (client.isDesktop()) {
      stopRefreshEvents = await listen("phase-three-scan", () => {
        void syncRefreshStatus();
      });
      if (!active) {
        stopRefreshEvents();
        stopRefreshEvents = undefined;
        return;
      }
    }
    await triggerRefresh();
  }

  function dispose() {
    active = false;
    stopRefreshEvents?.();
    stopRefreshEvents = undefined;
    if (autoRefreshTimer) win.clearInterval(autoRefreshTimer);
    if (statusPollTimer) win.clearInterval(statusPollTimer);
    doc.removeEventListener("click", handleClick, true);
    doc.removeEventListener("change", handleChange, true);
    doc.removeEventListener("visibilitychange", handleVisibilityChange);
  }

  function bindDomEvents() {
    doc.addEventListener("click", handleClick, true);
    doc.addEventListener("change", handleChange, true);
    doc.addEventListener("visibilitychange", handleVisibilityChange);
    syncAutoRefresh();
  }

  function handleVisibilityChange() {
    syncAutoRefresh();
  }

  async function handleClick(event: Event) {
    const button = (event.target as HTMLElement | null)?.closest("button");
    if (!button) return;

    const label = button.textContent?.replace(/\s+/g, " ").trim() ?? "";
    if (button.dataset.runtimeAction === "refresh") {
      event.preventDefault();
      if (runtime.refreshInProgress && runtime.currentOperationId) {
        await client.cancelRefresh(runtime.currentOperationId);
      } else {
        await triggerRefresh();
      }
      return;
    }

    if (label === "Copy Fixture Summary") {
      event.preventDefault();
      await copyDiagnosticsSummary();
      return;
    }

    if (label === "Review Roots") {
      event.preventDefault();
      await refreshDiagnostics();
      focusFirstRootRow();
    }
  }

  function handleChange(event: Event) {
    const target = event.target as HTMLInputElement | HTMLSelectElement | null;
    if (!target) return;
    const label = target.closest("label")?.textContent?.replace(/\s+/g, " ").trim() ?? "";

    if (target instanceof HTMLInputElement && target.type === "checkbox" && label.includes("Check metadata while active")) {
      runtime.autoCheck = target.checked;
      syncAutoRefresh();
      return;
    }

    if (target instanceof HTMLInputElement && target.type === "checkbox" && label.includes("Share fixture diagnostics")) {
      runtime.diagnosticsEnabled = target.checked;
    }
  }

  async function triggerRefresh() {
    const next = await client.startRefresh();
    runtime.latestView = next;
    runtime.refreshInProgress = true;
    onView(next);
    beginStatusPolling();
  }

  function beginStatusPolling() {
    if (statusPollTimer) return;
    statusPollTimer = win.setInterval(() => {
      void syncRefreshStatus();
    }, STATUS_POLL_MS);
  }

  async function syncRefreshStatus() {
    const status = await client.getRefreshStatus();
    runtime.refreshInProgress = status.inProgress;
    runtime.currentOperationId = status.operationId ?? undefined;

    if (status.snapshot) {
      runtime.latestView = status.snapshot;
      onView(status.snapshot);
    }

    if (!status.inProgress && statusPollTimer) {
      win.clearInterval(statusPollTimer);
      statusPollTimer = undefined;
      await refreshDiagnostics();
      syncAutoRefresh();
    }
  }

  function syncAutoRefresh() {
    if (autoRefreshTimer) {
      win.clearInterval(autoRefreshTimer);
      autoRefreshTimer = undefined;
    }

    if (!runtime.autoCheck || runtime.refreshInProgress || doc.visibilityState === "hidden") {
      return;
    }

    autoRefreshTimer = win.setInterval(() => {
      if (!runtime.refreshInProgress) {
        void triggerRefresh();
      }
    }, AUTO_REFRESH_MS);
  }

  async function refreshDiagnostics() {
    const diagnostics = await client.runDiagnostics();
    runtime.latestDiagnostics = diagnostics;
    patchSettingsPanel(doc, diagnostics, runtime);
  }

  async function copyDiagnosticsSummary() {
    const diagnostics = runtime.latestDiagnostics ?? await client.runDiagnostics();
    runtime.latestDiagnostics = diagnostics;
    patchSettingsPanel(doc, diagnostics, runtime);
    await copyText(win.navigator, summarizeDiagnostics(diagnostics));
  }

  function focusFirstRootRow() {
    const row = doc.querySelector<HTMLElement>(".root-table .root-row");
    row?.focus();
  }

  return { start, refresh: triggerRefresh, dispose };
}

export function summarizeDiagnostics(diagnostics: DiagnosticsReport) {
  const lines = [
    "STM diagnostics",
    `UI contract: ${diagnostics.uiContract.version} (${diagnostics.uiContract.locked ? "locked" : "review"})`,
    `Catalog: ${diagnostics.catalogVersion}`,
    `Storage: last good ${diagnostics.storage.lastGoodAvailable ? "available" : "missing"}, recovered ${diagnostics.storage.recoveredFromCorruption ? "yes" : "no"}, path ${redactSensitiveText(diagnostics.storage.path)}`,
    "Managers:",
    ...diagnostics.managers.map((manager) => `- ${manager.manager}: ${manager.status} (${manager.packages.length} packages)`),
    "Skill roots:",
    ...diagnostics.skills.roots.map((root) => {
      const declared = redactSensitiveText(root.declaredRoot);
      const canonical = root.canonicalRoot ? redactSensitiveText(root.canonicalRoot) : "n/a";
      return `- ${root.client}: ${root.accepted ? "accepted" : "rejected"} | declared ${declared} | canonical ${canonical}`;
    }),
    "Warnings:",
    ...(diagnostics.warnings.length > 0 ? diagnostics.warnings.map((warning) => `- ${redactSensitiveText(warning)}`) : ["- none"]),
  ];
  return lines.join("\n");
}

export function redactSensitiveText(value: string) {
  return value
    .replace(/\/Users\/[^/]+/g, "/Users/<user>")
    .replace(/\/home\/[^/]+/g, "/home/<user>")
    .replace(/[A-Z]:\\Users\\[^\\]+/g, "C:\\Users\\<user>");
}

export function patchSettingsPanel(doc: Document, diagnostics: DiagnosticsReport, runtime: RuntimeState) {
  const diagnosticSection = doc.querySelector(".diagnostic-section");
  if (diagnosticSection) {
    const entries = diagnosticSection.querySelectorAll("dd");
    if (entries[0]) entries[0].textContent = diagnostics.uiContract.version;
    if (entries[1]) entries[1].textContent = diagnostics.uiContract.locked ? "Locked" : "Review";
    if (entries[2]) entries[2].textContent = runtime.refreshInProgress ? "Refreshing" : "Connected";
  }

  const rootRows = Array.from(doc.querySelectorAll(".root-table .root-row"));
  for (const row of rootRows) {
    const client = row.querySelector("strong")?.textContent?.trim();
    const root = diagnostics.skills.roots.find((entry) => entry.client === client);
    const cells = row.querySelectorAll("span.mono-data");
    if (!root || cells.length < 2) continue;
    cells[0].textContent = redactSensitiveText(root.declaredRoot);
    cells[1].textContent = root.canonicalRoot ? redactSensitiveText(root.canonicalRoot) : "Rejected";
  }

  const adapterStates = buildAdapterStates(diagnostics, runtime.latestView);
  const adapterRows = Array.from(doc.querySelectorAll(".adapter-list .adapter-row"));
  for (const row of adapterRows) {
    const name = row.querySelector("strong")?.textContent?.trim();
    const state = row.querySelector<HTMLElement>(".availability-ready, .availability-muted");
    if (!name || !state || !adapterStates[name]) continue;
    state.textContent = adapterStates[name];
    state.className = adapterStates[name] === "Available" ? "availability-ready" : "availability-muted";
  }
}

function buildAdapterStates(diagnostics: DiagnosticsReport, view?: AppViewModel) {
  const managers = new Map(diagnostics.managers.map((entry) => [entry.manager, entry.status]));
  return {
    Homebrew: formatManagerState(managers.get("homebrew")),
    npm: view?.tools.some((tool) => tool.manager.toLowerCase() === "npm") ? "Available" : "Unavailable",
    "Vendor updaters": view?.tools.some((tool) => tool.executionMode === "vendor_handoff") ? "Available" : "Unavailable",
    "MCP client configs": diagnostics.mcp.servers.length > 0 ? "Available" : "Unavailable",
    WinGet: formatManagerState(managers.get("winget")),
    "Linux native manager": ["apt", "dnf", "pacman"].some((name) => managers.get(name) === "success") ? "Available" : "Unavailable",
  } as Record<string, string>;
}

function formatManagerState(status?: string) {
  return status === "success" || status === "empty" ? "Available" : "Unavailable";
}

async function copyText(navigatorRef: Navigator, value: string) {
  if (navigatorRef.clipboard?.writeText) {
    await navigatorRef.clipboard.writeText(value);
    return;
  }

  const textarea = document.createElement("textarea");
  textarea.value = value;
  textarea.setAttribute("readonly", "true");
  textarea.style.position = "fixed";
  textarea.style.opacity = "0";
  document.body.append(textarea);
  textarea.select();
  document.execCommand("copy");
  textarea.remove();
}

export function mergeRefreshStatus(base: AppViewModel | undefined, status: RefreshStatus) {
  return status.snapshot ?? base ?? null;
}
