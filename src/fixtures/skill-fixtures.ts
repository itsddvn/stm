import type { SkillViewModel } from "../../contracts/ui/view-model-contract";
import { withSkillPresentationActions } from "./presentation-action-fixtures";

type SkillFixtureSeed = {
  sideBySideSupported: boolean;
  viewModel: Omit<SkillViewModel, "primaryAction" | "resolutionActions">;
};

const skillFixtureSeeds: SkillFixtureSeed[] = [
  {
    sideBySideSupported: false,
    viewModel: {
      id: "frontend-design", name: "Frontend Design", description: "Build high-fidelity product interfaces from references.", source: "agentkit/skills", revision: "v2.0.0 · 7f84c21", availableRevision: "v2.1.0 · c9e9f31", digest: "sha256:3b8f…91ac", state: "managed_update_available",
      purposes: ["Design", "Frontend"], targets: [
        { client: "Codex", path: "$CODEX_HOME/skills/frontend-design", state: "current" },
        { client: "AgentKit", path: "$AGENTKIT_HOME/skills/frontend-design", state: "current" },
      ], riskFlags: ["Contains scripts"], diff: [
        { file: "SKILL.md", change: "modified", summary: "Clarifies accessibility review gate" },
        { file: "references/tokens.md", change: "added", summary: "Adds token contract guidance" },
      ],
    },
  },
  {
    sideBySideSupported: false,
    viewModel: {
      id: "security-scan", name: "Security Scan", description: "Audit code for secrets and vulnerable patterns.", source: "agentkit/skills", revision: "v1.4.2 · 9a12be0", digest: "sha256:074a…c8ee", state: "managed_current", purposes: ["Security", "Code review"],
      targets: [{ client: "Codex", path: "$CODEX_HOME/skills/security-scan", state: "current" }], riskFlags: [], diff: [],
    },
  },
  {
    sideBySideSupported: false,
    viewModel: {
      id: "release-pilot", name: "Release Pilot", description: "Prepare and validate project releases.", source: "trusted-catalog/release-pilot", revision: "v1.3.0 · d24b80c", availableRevision: "v1.4.0 · f91a6bc", digest: "sha256:84f1…006c", state: "modified", purposes: ["DevOps", "Release"],
      targets: [
        { client: "Claude Code", path: "$CLAUDE_HOME/skills/release-pilot", state: "modified" },
        { client: "AgentKit", path: "$AGENTKIT_HOME/skills/release-pilot", state: "current" },
      ], riskFlags: ["Local modification", "Contains scripts"], diff: [
        { file: "SKILL.md", change: "modified", summary: "Local instruction differs from receipt" },
        { file: "scripts/release.sh", change: "modified", summary: "Upstream validation logic changed" },
      ],
    },
  },
  {
    sideBySideSupported: false,
    viewModel: {
      id: "docx", name: "DOCX", description: "Create and edit Word documents.", source: "openai/document-skills", revision: "4fe91a2", digest: "sha256:aa34…f09c", state: "external", purposes: ["Documents"],
      targets: [{ client: "Codex", path: "$CODEX_HOME/skills/docx", state: "current" }], riskFlags: ["No app receipt"], diff: [],
    },
  },
  {
    sideBySideSupported: false,
    viewModel: {
      id: "browser-control", name: "Browser Control", description: "Drive browser workflows through approved controls.", source: "trusted-catalog/browser-control", revision: "v0.8.0 · 128ab6f", availableRevision: "v0.9.0 · 4e65dc1", digest: "sha256:f0a1…8bbb", state: "conflict", purposes: ["Browser", "Integrations"],
      targets: [
        { client: "Codex", path: "$CODEX_HOME/skills/browser-control", state: "failed" },
        { client: "Claude Code", path: "$CLAUDE_HOME/skills/browser-control", state: "current" },
      ], riskFlags: ["Partial target failure", "Tool requirements"], diff: [{ file: "SKILL.md", change: "modified", summary: "Updates browser permission contract" }],
    },
  },
  {
    sideBySideSupported: true,
    viewModel: {
      id: "database-operations", name: "Database Operations", description: "Plan and review safe database maintenance workflows.", source: "trusted-catalog/database-operations", revision: "Not installed", availableRevision: "v1.0.0 · 2e44a9c", digest: "sha256:pending", state: "missing", purposes: ["Data", "DevOps"],
      targets: [{ client: "Codex", path: "$CODEX_HOME/skills/database-operations", state: "missing" }], riskFlags: ["Contains scripts", "Tool requirements"], diff: [
        { file: "SKILL.md", change: "added", summary: "Defines bounded database operations" },
        { file: "scripts/inspect-schema.ts", change: "added", summary: "Read-only schema inspection helper" },
      ],
    },
  },
];

export const skillFixtures: SkillViewModel[] = skillFixtureSeeds.map((seed) =>
  withSkillPresentationActions(seed.viewModel, seed.sideBySideSupported),
);
