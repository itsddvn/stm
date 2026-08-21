import type { ToolViewModel } from "../../contracts/ui/view-model-contract";
import { withToolPresentationAction } from "./presentation-action-fixtures";

const toolFixtureSeeds: Array<Omit<ToolViewModel, "primaryAction">> = [
  {
    id: "git", name: "Git", summary: "Distributed source control", kind: "CLI tool", groups: ["Source control"], recommended: true,
    state: "managed_current", owner: "Homebrew", ownershipKind: "manager_owned", executionMode: "managed_execute",
    installedVersion: "2.51.0", availableVersion: "2.51.0", manager: "Homebrew", packageId: "git", platform: "macOS arm64", privilege: "none", lifecycleConfidence: "Verified mapping",
  },
  {
    id: "orca-ade", name: "Orca", summary: "Agentic development environment", kind: "Desktop app", groups: ["Source control", "Editors & IDEs", "AI coding agents"], recommended: true,
    state: "managed_update_available", owner: "Vendor updater", ownershipKind: "vendor_owned", executionMode: "vendor_handoff",
    installedVersion: "0.9.4", availableVersion: "0.10.1", manager: "Orca updater", packageId: "com.orca.ade", platform: "macOS arm64", privilege: "none", lifecycleConfidence: "Verified handoff",
  },
  {
    id: "cmux-desktop", name: "cmux desktop", summary: "Terminal workspace for agent workflows", kind: "Desktop app", groups: ["Terminal & shell", "AI coding agents"], recommended: true,
    state: "managed_current", owner: "Homebrew", ownershipKind: "manager_owned", executionMode: "managed_execute",
    installedVersion: "1.8.2", availableVersion: "1.8.2", manager: "Homebrew", packageId: "cmux", platform: "macOS 14+", privilege: "none", lifecycleConfidence: "Verified mapping",
  },
  {
    id: "docker-desktop", name: "Docker Desktop", summary: "Container development environment", kind: "Desktop app", groups: ["Containers", "Cloud & DevOps"], recommended: true,
    state: "managed_update_available", owner: "Docker updater", ownershipKind: "vendor_owned", executionMode: "vendor_handoff",
    installedVersion: "4.44.2", availableVersion: "4.45.0", manager: "Docker Desktop", packageId: "docker-desktop", platform: "macOS arm64", privilege: "required", lifecycleConfidence: "Verified handoff",
  },
  {
    id: "orbstack", name: "OrbStack", summary: "Fast local containers and Linux machines", kind: "Desktop app", groups: ["Containers", "Cloud & DevOps"], recommended: true,
    state: "missing", owner: "Homebrew", ownershipKind: "manager_owned", executionMode: "managed_execute",
    availableVersion: "2.2.1", manager: "Homebrew", packageId: "orbstack", platform: "macOS arm64", privilege: "none", lifecycleConfidence: "Verified mapping",
  },
  {
    id: "agentkit-cli", name: "AgentKit CLI", summary: "Agent workflow toolkit", kind: "CLI tool", groups: ["AI coding agents", "Package management"], recommended: true,
    state: "managed_current", owner: "Homebrew", ownershipKind: "manager_owned", executionMode: "managed_execute",
    installedVersion: "0.18.3", availableVersion: "0.18.3", manager: "Homebrew", packageId: "agentkit", platform: "macOS arm64", privilege: "none", lifecycleConfidence: "Detection verified",
  },
  {
    id: "oh-my-pi", name: "Oh My Pi", summary: "Terminal-first coding agent", kind: "CLI tool", groups: ["AI coding agents"], recommended: true,
    state: "external", owner: "Unknown", ownershipKind: "external", executionMode: "detect_only",
    installedVersion: "0.7.8", manager: "No manager receipt", packageId: "omp", platform: "macOS arm64", privilege: "unknown", lifecycleConfidence: "Detection only",
  },
  {
    id: "codex-cli", name: "Codex CLI", summary: "OpenAI coding agent", kind: "CLI tool", groups: ["AI coding agents"], recommended: true,
    state: "managed_update_available", owner: "npm", ownershipKind: "manager_owned", executionMode: "managed_execute",
    installedVersion: "0.31.0", availableVersion: "0.32.1", manager: "npm", packageId: "@openai/codex", platform: "macOS arm64", privilege: "none", lifecycleConfidence: "Verified mapping",
  },
  {
    id: "grok-build", name: "Grok Build", summary: "AI coding CLI", kind: "CLI tool", groups: ["AI coding agents"], recommended: true,
    state: "managed_update_available", owner: "Vendor release", ownershipKind: "vendor_owned", executionMode: "detect_only",
    installedVersion: "0.4.2", availableVersion: "0.5.0", manager: "Vendor channel", packageId: "grok", platform: "macOS arm64", privilege: "unknown", lifecycleConfidence: "Update detection only",
  },
  {
    id: "cloudflared", name: "cloudflared", summary: "Cloudflare Tunnel connector", kind: "Service daemon", groups: ["API & networking", "Cloud & DevOps", "Security"], recommended: true,
    state: "manager_unavailable", owner: "Homebrew", ownershipKind: "manager_owned", executionMode: "managed_execute",
    installedVersion: "2026.7.0", availableVersion: "2026.8.1", manager: "Homebrew", packageId: "cloudflared", platform: "macOS arm64", privilege: "none", lifecycleConfidence: "Manager required",
  },
];

export const toolFixtures: ToolViewModel[] = toolFixtureSeeds.map(withToolPresentationAction);
