import { readFileSync, existsSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const errors = [];

const requiredSchemas = [
  "catalog/schemas/application-update.schema.json",
  "catalog/schemas/auth-reference.schema.json",
  "catalog/schemas/inventory-contracts.schema.json",
  "catalog/schemas/mcp-client-binding.schema.json",
  "catalog/schemas/mcp-discovery.schema.json",
  "catalog/schemas/mcp-server.schema.json",
  "catalog/schemas/operation-plan.schema.json",
  "catalog/schemas/operation-receipt.schema.json",
  "catalog/schemas/skill-scan-report.schema.json",
  "catalog/schemas/source-analysis.schema.json",
  "catalog/schemas/tool-record.schema.json"
];

const requiredManagerFixtures = [
  "tests/fixtures/feasibility/managers/winget/success.txt",
  "tests/fixtures/feasibility/managers/winget/empty.txt",
  "tests/fixtures/feasibility/managers/winget/malformed.txt",
  "tests/fixtures/feasibility/managers/winget/manager-unavailable.txt",
  "tests/fixtures/feasibility/managers/winget/timed-out.txt",
  "tests/fixtures/feasibility/managers/winget/version-variant.txt",
  "tests/fixtures/feasibility/managers/homebrew/success.txt",
  "tests/fixtures/feasibility/managers/homebrew/empty.txt",
  "tests/fixtures/feasibility/managers/homebrew/malformed.txt",
  "tests/fixtures/feasibility/managers/homebrew/manager-unavailable.txt",
  "tests/fixtures/feasibility/managers/homebrew/timed-out.txt",
  "tests/fixtures/feasibility/managers/homebrew/version-variant.txt",
  "tests/fixtures/feasibility/managers/apt/success.txt",
  "tests/fixtures/feasibility/managers/apt/empty.txt",
  "tests/fixtures/feasibility/managers/apt/malformed.txt",
  "tests/fixtures/feasibility/managers/apt/manager-unavailable.txt",
  "tests/fixtures/feasibility/managers/apt/timed-out.txt",
  "tests/fixtures/feasibility/managers/apt/version-variant.txt"
];

const requiredSkillFixtures = [
  "tests/fixtures/feasibility/skills/home/.codex/skills/frontend-design/SKILL.md",
  "tests/fixtures/feasibility/skills/home/.claude/skills/release-ops/SKILL.md",
  "tests/fixtures/feasibility/skills/home/.agents/skills/mcp-builder/SKILL.md"
];

const requiredMcpFixtures = [
  "tests/fixtures/feasibility/mcp/codex/config.toml",
  "tests/fixtures/feasibility/mcp/claude-code/config.json",
  "tests/fixtures/feasibility/mcp/cursor/mcp.json",
  "tests/fixtures/feasibility/mcp/cursor/unsupported-config.json"
];

const requiredDocs = [
  "README.md",
  "docs/code-standards.md",
  "docs/system-architecture.md",
  "docs/phase-02-feasibility-report.md",
  ".github/workflows/quality.yml",
  "src-tauri/tauri.conf.json"
];

for (const path of [...requiredSchemas, ...requiredManagerFixtures, ...requiredSkillFixtures, ...requiredMcpFixtures, ...requiredDocs]) {
  if (!existsSync(resolve(root, path))) {
    errors.push(`missing required Phase 2 artifact: ${path}`);
  }
}

const seenIds = new Set();
for (const schemaPath of requiredSchemas) {
  const schema = readJson(schemaPath);
  if (typeof schema !== "object" || schema === null || Array.isArray(schema)) {
    errors.push(`schema must be a JSON object: ${schemaPath}`);
    continue;
  }

  if (typeof schema.$id !== "string" || schema.$id.length === 0) {
    errors.push(`schema missing $id: ${schemaPath}`);
  } else if (seenIds.has(schema.$id)) {
    errors.push(`duplicate schema $id: ${schema.$id}`);
  } else {
    seenIds.add(schema.$id);
  }

  if (schemaPath !== "catalog/schemas/inventory-contracts.schema.json" && schema.type !== "object") {
    errors.push(`schema must declare top-level object type: ${schemaPath}`);
  }

  if (schema.required && !schema.properties) {
    errors.push(`schema with required keys must declare properties: ${schemaPath}`);
  }
}

for (const manager of ["winget", "homebrew", "apt"]) {
  for (const statusFile of ["manager-unavailable.txt", "timed-out.txt"]) {
    const content = readText(`tests/fixtures/feasibility/managers/${manager}/${statusFile}`).trim();
    if (!content.startsWith("# status:")) {
      errors.push(`manager status fixture must start with '# status:': ${manager}/${statusFile}`);
    }
  }
}

const codexFixture = readText("tests/fixtures/feasibility/mcp/codex/config.toml");
if (!codexFixture.includes("github_mirror") || !codexFixture.includes("tokenAlias")) {
  errors.push("codex MCP fixture must cover duplicate bindings and token alias references");
}

const claudeFixture = readJson("tests/fixtures/feasibility/mcp/claude-code/config.json");
if (!claudeFixture?.mcpServers?.Docs?.headers?.Authorization) {
  errors.push("Claude Code MCP fixture must include header reference redaction coverage");
}

const cursorFixture = readJson("tests/fixtures/feasibility/mcp/cursor/mcp.json");
if (cursorFixture?.mcpServers?.Postgres?.disabled !== true) {
  errors.push("Cursor MCP fixture must include disabled binding coverage");
}

if (errors.length > 0) {
  console.error("Phase 2 foundation verification failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log(
  `Phase 2 foundation verification passed (${requiredSchemas.length} schemas, ${requiredManagerFixtures.length + requiredSkillFixtures.length + requiredMcpFixtures.length} fixtures).`
);

function readJson(path) {
  return JSON.parse(readText(path));
}

function readText(path) {
  return readFileSync(resolve(root, path), "utf8");
}
