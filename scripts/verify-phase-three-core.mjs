import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const errors = [];

const requiredArtifacts = [
  "README.md",
  "catalog/schemas/tool-catalog.schema.json",
  "catalog/schemas/skill-catalog.schema.json",
  "catalog/tools/recommended.json",
  "catalog/tools/candidates.json",
  "crates/stm-runtime/migrations/0001_initial.sql",
  "crates/stm-runtime/migrations/0002_read_only_snapshot.sql",
  "crates/stm-runtime/migrations/0003_lifecycle_receipts.sql",
  "docs/code-standards.md",
  "docs/system-architecture.md",
  "src-tauri/capabilities/default.json",
  "src-tauri/permissions/application-commands.toml",
  "package.json",
  ".github/workflows/quality.yml",
  "tests/fixtures/catalog/product-update.json",
  "tests/fixtures/catalog/operations.json",
  "tests/fixtures/roots/skill-roots.json",
  "tests/fixtures/skills/receipts.json",
  "tests/fixtures/skills/state-overrides.json",
  "tests/fixtures/skills/update-metadata.json",
  "tests/fixtures/tools/app-receipts.json",
  "tests/fixtures/tools/os-apps.json",
  "tests/fixtures/tools/probes.json",
  "tests/fixtures/tools/update-metadata.json",
  "tests/fixtures/mcp/health.json",
  "tests/fixtures/mcp/codex/config.toml",
  "tests/fixtures/mcp/claude-code/config.json",
  "tests/fixtures/mcp/cursor/mcp.json",
  "tests/fixtures/mcp/cursor/unsupported-config.json"
];

for (const manager of ["winget", "homebrew", "apt", "dnf", "pacman"]) {
  for (const file of [
    "success.txt",
    "empty.txt",
    "malformed.txt",
    "manager-unavailable.txt",
    "timed-out.txt",
    "version-variant.txt"
  ]) {
    requiredArtifacts.push(`tests/fixtures/managers/${manager}/${file}`);
  }
}

for (const path of requiredArtifacts) {
  if (!existsSync(resolve(root, path))) {
    errors.push(`missing required Phase 3 artifact: ${path}`);
  }
}

const recommended = readJson("catalog/tools/recommended.json");
const candidates = readJson("catalog/tools/candidates.json");
const packageJson = readJson("package.json");
const capability = readJson("src-tauri/capabilities/default.json");
const permissionsToml = readFile("src-tauri/permissions/application-commands.toml");
const qualityWorkflow = readFile(".github/workflows/quality.yml");
const readme = readFile("README.md");
const architectureDoc = readFile("docs/system-architecture.md");

if (!recommended.tools?.length) {
  errors.push("recommended catalog must contain at least one tool");
}

if (!candidates.tools?.length) {
  errors.push("candidate catalog must not be empty");
}

const recommendedIds = new Set();
const requiredProfileIds = ["git", "agentkit-cli", "codex-cli", "cloudflared"];
for (const tool of recommended.tools ?? []) {
  if (recommendedIds.has(tool.id)) {
    errors.push(`duplicate recommended tool id: ${tool.id}`);
  }
  recommendedIds.add(tool.id);
  if (tool.recommended !== true) {
    errors.push(`recommended tool must stay recommended: ${tool.id}`);
  }
}
for (const id of requiredProfileIds) {
  if (!recommendedIds.has(id)) {
    errors.push(`recommended catalog missing profile default: ${id}`);
  }
}

const candidateIds = new Set((candidates.tools ?? []).map((tool) => tool.id));
for (const id of recommendedIds) {
  if (candidateIds.has(id)) {
    errors.push(`recommended tool must not also appear in candidates: ${id}`);
  }
}

const skillReceipts = readJson("tests/fixtures/skills/receipts.json");
if (!skillReceipts.some((receipt) => receipt.id === "database-operations")) {
  errors.push("skill receipts must include a missing-target coverage entry");
}

const mcpClaude = readJson("tests/fixtures/mcp/claude-code/config.json");
if (!mcpClaude?.mcpServers?.Sentry?.authRequired) {
  errors.push("Claude Code MCP fixture must include missing auth-reference coverage");
}

const operations = readJson("tests/fixtures/catalog/operations.json");
if (!operations.some((entry) => entry.receipt?.status === "partial")) {
  errors.push("operations fixture must include partial receipt coverage");
}

if (packageJson.scripts?.["verify:phase-three-core"] !== "node scripts/verify-phase-three-core.mjs") {
  errors.push("package.json must expose verify:phase-three-core");
}

for (const permission of ["phase-three-read", "phase-five-tool-lifecycle"]) {
  if (!capability.permissions?.includes(permission)) {
    errors.push(`capability allowlist must include ${permission}`);
  }
}

for (const command of [
  "refresh_snapshot",
  "refresh_status",
  "headless_scan",
  "list_tools",
  "get_tool_detail",
  "list_skills",
  "get_skill_detail",
  "list_mcp_servers",
  "get_mcp_detail",
  "list_updates",
  "list_operations",
  "analyze_source",
  "run_diagnostics",
  "prepare_lifecycle_plan",
  "start_lifecycle_operation",
  "lifecycle_operation_status",
  "cancel_lifecycle_operation",
  "cancel_operation"
]) {
  if (!permissionsToml.includes(`"${command}"`)) {
    errors.push(`application command allowlist missing ${command}`);
  }
}

if (!qualityWorkflow.includes("pnpm verify:phase-three-core")) {
  errors.push("quality workflow must run pnpm verify:phase-three-core");
}

if (!readme.includes("verify:phase-three-core")) {
  errors.push("README must document verify:phase-three-core");
}

if (!architectureDoc.includes("verify-phase-three-core.mjs")) {
  errors.push("system architecture doc must mention the Phase 3 verification script");
}

if (errors.length > 0) {
  console.error("Phase 3 core verification failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log(
  `Phase 3 core verification passed (${requiredArtifacts.length} artifacts, ${recommended.tools.length} recommended tools, ${candidates.tools.length} candidates).`
);

function readJson(path) {
  return JSON.parse(readFileSync(resolve(root, path), "utf8"));
}

function readFile(path) {
  return readFileSync(resolve(root, path), "utf8");
}
