import type { PortableSetupDocument } from "../../contracts/ui/setup-contract";

const MAX_PORTABLE_BYTES = 64 * 1024;
const MAX_PORTABLE_RESOURCES = 32;
const DOCUMENT_KEYS = new Set(["schemaVersion", "target", "resources"]);
const RESOURCE_KEYS = new Set(["kind", "id", "desiredAction", "credentialReferenceIds"]);
const FORBIDDEN_KEYS = new Set(["command", "args", "executable", "shell", "script"]);

export function validatePortableSetupText(text: string, currentTarget: string): {
  document: PortableSetupDocument;
  warnings: string[];
} {
  if (new TextEncoder().encode(text).length > MAX_PORTABLE_BYTES) {
    throw new Error("portable setup exceeds 64 KiB");
  }
  const parsed: unknown = JSON.parse(text);
  if (!isRecord(parsed)) throw new Error("invalid portable setup");
  rejectForbiddenKeys(parsed, "portable setup");
  for (const key of Object.keys(parsed)) {
    if (!DOCUMENT_KEYS.has(key)) throw new Error(`unknown portable field: ${key}`);
  }
  if (parsed.schemaVersion !== 1) throw new Error("unsupported portable schema version");
  if (typeof parsed.target !== "string" || parsed.target.trim().length === 0) {
    throw new Error("portable setup requires a target");
  }
  if (!Array.isArray(parsed.resources)) throw new Error("portable resources must be an array");
  if (parsed.resources.length > MAX_PORTABLE_RESOURCES) {
    throw new Error(`portable setup exceeds ${MAX_PORTABLE_RESOURCES} resources`);
  }
  const resources = parsed.resources.map((resource, index) => {
    if (!isRecord(resource)) throw new Error(`invalid portable resource ${index}`);
    rejectForbiddenKeys(resource, `resource ${index}`);
    for (const key of Object.keys(resource)) {
      if (!RESOURCE_KEYS.has(key)) throw new Error(`unknown portable field: ${key}`);
    }
    if (typeof resource.id !== "string" || resource.id.trim().length === 0) {
      throw new Error("portable resource id is required");
    }
    if (looksLikeMachinePath(resource.id)) {
      throw new Error("portable resource IDs may not contain machine paths");
    }
    if (typeof resource.kind !== "string" || !["tool", "skill", "mcp"].includes(resource.kind)) {
      throw new Error("unsupported portable resource kind");
    }
    if (typeof resource.desiredAction !== "string" || !["keep", "install", "update", "enable", "add", "review"].includes(resource.desiredAction)) {
      throw new Error("unsupported portable desired action");
    }
    if (resource.credentialReferenceIds !== undefined) {
      if (!Array.isArray(resource.credentialReferenceIds)
        || resource.credentialReferenceIds.some((reference) => typeof reference !== "string" || !/^[A-Za-z0-9_-]{1,64}$/.test(reference))) {
        throw new Error("credential references must be bounded identifiers");
      }
    }
    return {
      kind: resource.kind,
      id: resource.id,
      desiredAction: resource.desiredAction,
      credentialReferenceIds: resource.credentialReferenceIds as string[] | undefined,
    };
  });
  const document: PortableSetupDocument = {
    schemaVersion: 1,
    target: parsed.target,
    resources,
  };
  const warnings: string[] = [];
  if (document.target !== currentTarget) {
    throw new Error(`this file is for ${document.target} and cannot be imported on ${currentTarget}`);
  }
  return { document, warnings };
}


export function assertPortableExportSafe(text: string) {
  if (/(eyJ[A-Za-z0-9_-]+\.){2}[A-Za-z0-9_-]+/.test(text)
    || /AKIA[0-9A-Z]{16}/.test(text)
    || /-----BEGIN [A-Z ]*PRIVATE KEY-----/.test(text)
    || /ghp_[A-Za-z0-9]{36}/.test(text)
    || /sk_(live|test)_[A-Za-z0-9]{24,}/.test(text)) {
    throw new Error("portable export contains a secret-shaped value");
  }
}
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function looksLikeMachinePath(value: string) {
  return value.startsWith("/")
    || value.startsWith("\\")
    || value.startsWith("~/")
    || value.startsWith("~\\")
    || value.startsWith("$HOME/")
    || value.startsWith("$HOME\\")
    || value.startsWith("%USERPROFILE%")
    || value.toLowerCase().startsWith("file:")
    || value.toLowerCase().startsWith("$env:userprofile")
    || /^[A-Za-z]:/.test(value);
}

function rejectForbiddenKeys(value: Record<string, unknown>, context: string) {
  for (const key of Object.keys(value)) {
    if (FORBIDDEN_KEYS.has(key.toLowerCase())) {
      throw new Error(`${context} may not provide shell or executable content`);
    }
  }
}
