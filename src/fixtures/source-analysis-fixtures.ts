import type { SourceAnalysisViewModel, SourceKind } from "../../contracts/ui/view-model-contract";

const sourceAnalysisHandles = new Map<string, { normalizedUrl: string; resourceId: string }>();

const defaults: Record<SourceKind, Omit<SourceAnalysisViewModel, "kind" | "submittedUrl" | "normalizedUrl" | "sourceHost" | "status" | "trust" | "lifecycleRequest">> = {
  tool: {
    detectedName: "Developer tool source",
    sourceType: "Package or release source",
    publisher: "Publisher review required",
    target: "Canonical tool and owner mapping",
    riskFlags: ["Network metadata", "Ownership must be verified"],
    notes: ["No executable or install arguments are accepted from this URL.", "Managed execution remains unavailable until a reviewed mapping matches."],
  },
  skill: {
    detectedName: "Agent Skill source",
    sourceType: "Git repository and optional skill path",
    publisher: "Catalog match required",
    target: "Approved global client roots",
    riskFlags: ["Active instruction content", "Scripts and symlinks require review"],
    notes: ["Project-local targets are never offered.", "Materialization remains blocked until immutable provenance matches a trusted catalog entry."],
  },
  mcp: {
    detectedName: "Remote MCP server",
    sourceType: "Streamable HTTP endpoint",
    publisher: "Server owner review required",
    target: "Selected global MCP client configurations",
    riskFlags: ["Remote tool capabilities", "Authentication reference may be required"],
    notes: ["Credential values are never stored in the fixture.", "Client configuration is not changed by this review."],
  },
};

export function analyzeSourceFixture(kind: SourceKind, submittedUrl: string): SourceAnalysisViewModel {
  const source = submittedUrl.trim();
  try {
    const url = new URL(source);
    const safeSource = sanitizeSourceUrl(url);
    if (url.protocol !== "https:") {
      return blockedAnalysis(kind, safeSource, url.hostname || "Invalid source", "Use a complete HTTPS URL.");
    }
    if (url.username || url.password || url.search) {
      return blockedAnalysis(
        kind,
        safeSource,
        url.hostname || "Invalid source",
        "Remove embedded credentials and query parameters before review.",
      );
    }

    const normalized = new URL(safeSource);
    normalized.hash = "";
    const knownSource = normalized.hostname === "github.com"
      || normalized.hostname === "githubusercontent.com"
      || normalized.hostname.endsWith(".githubusercontent.com");
    const base = defaults[kind];
    const detectedName = detectName(kind, normalized, base.detectedName);
    const resourceId = slug(detectedName);
    const lifecycleHandle = `fixture-source:${kind}:${resourceId}:${stableToken(normalized.toString())}`;
    sourceAnalysisHandles.set(lifecycleHandle, { normalizedUrl: normalized.toString(), resourceId });
    return {
      ...base,
      kind,
      submittedUrl: normalized.toString(),
      normalizedUrl: normalized.toString(),
      sourceHost: normalized.hostname,
      status: "review_ready",
      trust: knownSource && kind !== "mcp" ? "catalog_match" : "review_required",
      detectedName,
      publisher: knownSource && kind !== "mcp" ? "Known source · catalog evidence pending" : base.publisher,
      lifecycleRequest: {
        resourceKind: kind,
        action: kind === "mcp" ? "add" : "install",
        resourceId,
        sourceAnalysisHandle: lifecycleHandle,
      },
    };
  } catch {
    return blockedAnalysis(kind, "", "Invalid source", "Enter a complete HTTPS URL.");
  }
}

export function resolveSourceAnalysisHandle(handle: string) {
  return sourceAnalysisHandles.get(handle);
}

function sanitizeSourceUrl(url: URL) {
  const sanitized = new URL(url.toString());
  sanitized.username = "";
  sanitized.password = "";
  sanitized.search = "";
  sanitized.hash = "";
  return sanitized.toString();
}

function detectName(kind: SourceKind, url: URL, fallback: string) {
  const path = url.pathname.toLowerCase();
  if (kind === "tool" && path.includes("codex")) return "Codex CLI";
  if (kind === "skill" && path.includes("frontend-design")) return "Frontend Design skill";
  if (kind === "mcp" && url.hostname.includes("sentry")) return "Sentry MCP";
  return fallback;
}

function blockedAnalysis(
  kind: SourceKind,
  submittedUrl: string,
  sourceHost: string,
  note: string,
): SourceAnalysisViewModel {
  const base = defaults[kind];
  return {
    ...base,
    kind,
    submittedUrl,
    sourceHost,
    status: "blocked",
    trust: "blocked",
    riskFlags: ["Source validation failed"],
    notes: [note, "No analysis or installation preview was created."],
    lifecycleRequest: {
      resourceKind: kind,
      action: "blocked",
      resourceId: "blocked-source",
      sourceAnalysisHandle: `fixture-source:${kind}:blocked`,
    },
  };
}

function slug(value: string) {
  return value.toLowerCase().replaceAll(/[^a-z0-9]+/g, "-").replaceAll(/^-|-$/g, "");
}

function stableToken(value: string) {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}
