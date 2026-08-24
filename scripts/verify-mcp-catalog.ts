import { lstat, readFile } from "node:fs/promises";
import { resolve } from "node:path";

const catalogPath = resolve("catalog/mcp/approved.json");
const schemaPath = resolve("catalog/schemas/mcp-catalog.schema.json");
const allowedTransports = new Set(["stdio", "streamable_http", "sse"]);
const allowedClients = new Set(["codex", "claude_code", "cursor"]);
const allowedCapabilities = new Set([
  "resources",
  "tools",
  "prompts",
  "logging",
  "completions",
  "roots",
  "sampling",
  "elicitation",
]);

interface AuthReference {
  kind: "env_var" | "token_alias" | "header_name";
  reference: string;
}

interface Mapping {
  id: string;
  name: string;
  aliases: string[];
  transport: string;
  commandOrUrl: string;
  argsPrefix: string[];
  allowAbsoluteTrailingArgs: boolean;
  capabilities: string[];
  clients: string[];
  authRequired: boolean;
  authReferences: AuthReference[];
}

interface Catalog {
  schemaVersion: number;
  mappings: Mapping[];
}

async function readRegularJson(path: string, maximumBytes: number): Promise<unknown> {
  const metadata = await lstat(path);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size > maximumBytes) {
    throw new Error(`${path} must be a bounded regular file`);
  }
  return JSON.parse(await readFile(path, "utf8")) as unknown;
}

function expectStringArray(value: unknown, field: string): asserts value is string[] {
  if (!Array.isArray(value) || value.some((item) => typeof item !== "string")) {
    throw new Error(`${field} must be a string array`);
  }
}

function validateMapping(mapping: Mapping, identities: Set<string>): void {
  if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(mapping.id) || !mapping.name) {
    throw new Error("MCP mappings require canonical id and name fields");
  }
  expectStringArray(mapping.aliases, `${mapping.id}.aliases`);
  expectStringArray(mapping.argsPrefix, `${mapping.id}.argsPrefix`);
  expectStringArray(mapping.capabilities, `${mapping.id}.capabilities`);
  expectStringArray(mapping.clients, `${mapping.id}.clients`);
  if (typeof mapping.authRequired !== "boolean" || !Array.isArray(mapping.authReferences)) {
    throw new Error(`${mapping.id}.authReferences must be an array with an authRequired flag`);
  }
  const authReferences = new Set<string>();
  for (const reference of mapping.authReferences) {
    if (typeof reference !== "object" || reference === null
      || !["env_var", "token_alias", "header_name"].includes(reference.kind)
      || typeof reference.reference !== "string"
      || !/^[A-Za-z][A-Za-z0-9_.:-]{0,127}$/.test(reference.reference)) {
      throw new Error(`${mapping.id} has an invalid credential reference`);
    }
    const identity = `${reference.kind}:${reference.reference}`;
    if (authReferences.has(identity)) throw new Error(`${mapping.id} has duplicate credential references`);
    authReferences.add(identity);
  }
  if (mapping.authRequired && mapping.authReferences.length === 0) {
    throw new Error(`${mapping.id} requires an explicit credential reference`);
  }
  if (!allowedTransports.has(mapping.transport)) throw new Error(`${mapping.id} uses an unsupported transport`);
  if (!mapping.commandOrUrl || typeof mapping.allowAbsoluteTrailingArgs !== "boolean") {
    throw new Error(`${mapping.id} has an incomplete execution mapping`);
  }
  const mappingIdentities = new Set(
    [mapping.id, mapping.name, ...mapping.aliases].map((identity) => identity.toLowerCase()),
  );
  for (const identity of mappingIdentities) {
    if (identities.has(identity)) throw new Error(`duplicate MCP identity ${identity}`);
    identities.add(identity);
  }
  if (new Set(mapping.clients).size !== mapping.clients.length
    || mapping.clients.some((client) => !allowedClients.has(client))) {
    throw new Error(`${mapping.id} has invalid client support`);
  }
  if (new Set(mapping.capabilities).size !== mapping.capabilities.length
    || mapping.capabilities.some((capability) => !allowedCapabilities.has(capability))) {
    throw new Error(`${mapping.id} has invalid capabilities`);
  }
  const serializedArguments = mapping.argsPrefix.join(" ").toLowerCase();
  if (/(?:token|password|secret|api[-_]?key)=/.test(serializedArguments)) {
    throw new Error(`${mapping.id} embeds credential material in arguments`);
  }
  if (mapping.transport === "stdio") {
    if (/\s|[;|&><`]/.test(mapping.commandOrUrl)) {
      throw new Error(`${mapping.id} stdio mapping must use one executable token`);
    }
  } else {
    const endpoint = new URL(mapping.commandOrUrl);
    if (endpoint.protocol !== "https:" || endpoint.username || endpoint.password || endpoint.search || endpoint.hash) {
      throw new Error(`${mapping.id} endpoint must be credential-free HTTPS`);
    }
    if (mapping.argsPrefix.length > 0 || mapping.allowAbsoluteTrailingArgs) {
      throw new Error(`${mapping.id} remote mapping cannot declare process arguments`);
    }
  }
}

async function main(): Promise<void> {
  await readRegularJson(schemaPath, 64 * 1024);
  const value = await readRegularJson(catalogPath, 256 * 1024);
  if (typeof value !== "object" || value === null) throw new Error("MCP catalog root must be an object");
  const catalog = value as Catalog;
  if (catalog.schemaVersion !== 2 || !Array.isArray(catalog.mappings) || catalog.mappings.length === 0) {
    throw new Error("MCP catalog version or mappings are invalid");
  }
  const identities = new Set<string>();
  for (const mapping of catalog.mappings) validateMapping(mapping, identities);
  process.stdout.write(`Verified ${catalog.mappings.length} approved MCP mappings.\n`);
}

main().catch((error: unknown) => {
  process.stderr.write(`MCP catalog verification failed: ${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
