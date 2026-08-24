import type { ToolViewModel } from "../../contracts/ui/view-model-contract";
import type { InstallProviderPreference, QuickSetupView, SetupRowAction, SetupRowView } from "../../contracts/ui/setup-contract";

const MACOS_DEFAULTS = ["git", "agentkit-cli", "codex-cli", "cloudflared", "orbstack", "orca-ade", "cmux-desktop"];
const OPTIONAL = ["oh-my-pi", "grok-build", "docker-desktop"];

const PREFERENCE_KEY = "stm.providerPreference";
const DISMISS_KEY = "stm.quickSetupDismissed";

export function loadSetupPreference(): InstallProviderPreference {
  if (typeof window === "undefined") return "automatic";
  const value = window.localStorage.getItem(PREFERENCE_KEY);
  if (value === "prefer_homebrew" || value === "prefer_bun" || value === "automatic") return value;
  return "automatic";
}

export function saveSetupPreference(preference: InstallProviderPreference) {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(PREFERENCE_KEY, preference);
}

export function isQuickSetupDismissed() {
  return typeof window !== "undefined" && window.localStorage.getItem(DISMISS_KEY) === "1";
}

export function dismissQuickSetup() {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(DISMISS_KEY, "1");
}

export function buildQuickSetupView(tools: ToolViewModel[]): QuickSetupView {
  const preference = loadSetupPreference();
  const rows = tools.map((tool) => {
    const optional = OPTIONAL.includes(tool.id) && !MACOS_DEFAULTS.includes(tool.id);
    const action = rowAction(tool);
    return {
      id: tool.id,
      name: tool.name,
      summary: tool.summary,
      selected: MACOS_DEFAULTS.includes(tool.id) && action !== "installed" && action !== "blocked",
      optional,
      action,
      reason: tool.reasonCode,
      owner: tool.owner,
    } satisfies SetupRowView;
  });
  return {
    target: "macos_arm64",
    preference,
    dismissed: isQuickSetupDismissed(),
    providers: { homebrew: "Homebrew", bun: undefined, npm: tools.some((tool) => tool.manager === "npm") ? "npm" : undefined },
    tools: rows.filter((row) => !row.optional),
    optional: rows.filter((row) => row.optional),
  };
}

function rowAction(tool: ToolViewModel): SetupRowAction {
  if (tool.state === "managed_current") return "installed";
  if (tool.state === "managed_update_available") return tool.executionMode === "vendor_handoff" ? "handoff" : "update";
  if (tool.state === "missing") return "install";
  if (tool.executionMode === "detect_only") return "guidance";
  if (tool.executionMode === "vendor_handoff") return "handoff";
  return "blocked";
}
