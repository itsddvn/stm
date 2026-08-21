import type {
  ProductUpdateAction,
  McpPresentationAction,
  SkillPresentationAction,
  ToolPresentationAction,
  UpdateSelectionAction,
} from "./action-contract";
import type { InventoryState, ReasonCode, SurfaceState } from "./state-contract";
import type { LifecyclePlanRequest } from "./lifecycle-contract";

export type ExecutionMode = "managed_execute" | "vendor_handoff" | "detect_only";
export type OwnershipKind = "manager_owned" | "vendor_owned" | "system_owned" | "external" | "unknown";
export type SourceKind = "tool" | "skill" | "mcp";
export type SourceTrust = "catalog_match" | "review_required" | "blocked";

export interface SourceAnalysisViewModel {
  kind: SourceKind;
  submittedUrl: string;
  normalizedUrl?: string;
  status: "review_ready" | "blocked";
  detectedName: string;
  sourceHost: string;
  sourceType: string;
  publisher: string;
  target: string;
  trust: SourceTrust;
  riskFlags: string[];
  notes: string[];
  lifecycleRequest: LifecyclePlanRequest;
}


export interface ToolViewModel {
  id: string;
  name: string;
  summary: string;
  kind: string;
  groups: string[];
  recommended: boolean;
  state: InventoryState;
  owner: string;
  ownershipKind: OwnershipKind;
  executionMode: ExecutionMode;
  installedVersion?: string;
  availableVersion?: string;
  manager: string;
  packageId: string;
  platform: string;
  privilege: "none" | "required" | "unknown";
  lifecycleConfidence: string;
  reasonCode?: ReasonCode;
  primaryAction: ToolPresentationAction;
}

export interface SkillTargetViewModel {
  client: "Codex" | "Claude Code" | "AgentKit";
  path: string;
  state: "current" | "modified" | "failed" | "missing";
}

export interface SkillViewModel {
  id: string;
  name: string;
  description: string;
  source: string;
  revision: string;
  availableRevision?: string;
  digest: string;
  state: InventoryState;
  purposes: string[];
  targets: SkillTargetViewModel[];
  riskFlags: string[];
  diff: { file: string; change: "added" | "modified" | "removed"; summary: string }[];
  primaryAction: SkillPresentationAction;
  resolutionActions: SkillPresentationAction[];
}
export type McpTransport = "stdio" | "streamable_http" | "sse";
export type McpClient = "Codex" | "Claude Code" | "Cursor";

export interface McpClientBindingViewModel {
  client: McpClient;
  state: "enabled" | "disabled" | "unsupported";
  scope: "global";
}

export interface McpServerViewModel {
  id: string;
  name: string;
  description: string;
  source: string;
  transport: McpTransport;
  commandOrUrl: string;
  clients: McpClientBindingViewModel[];
  capabilities: string[];
  trust: "verified" | "review_required" | "blocked";
  authState: "none" | "reference_configured" | "reference_missing";
  health: "healthy" | "degraded" | "unreachable" | "unknown";
  lastChecked: string;
  state: InventoryState;
  primaryAction: McpPresentationAction;
  toggleAction: McpPresentationAction;
  removeAction: McpPresentationAction;
}


export interface UpdateViewModel {
  id: string;
  resourceType: "tool" | "skill" | "product";
  name: string;
  current: string;
  target: string;
  executionMode: ExecutionMode | "signed_product_update";
  selected: false;
  risk: string;
  selectionAction?: UpdateSelectionAction;
  reviewAction?: ProductUpdateAction;
}

export interface OperationViewModel {
  id: string;
  resource: string;
  action: string;
  status: "success" | "partial" | "cancelled" | "failed" | "recoverable" | "in_progress";
  startedAt: string;
  owner: string;
  detail: string;
  receipt: string;
  details: string[];
  lifecycleRequest: LifecyclePlanRequest;
}

export interface AppViewModel {
  surface: SurfaceState;
  tools: ToolViewModel[];
  skills: SkillViewModel[];
  mcpServers: McpServerViewModel[];
  updates: UpdateViewModel[];
  operations: OperationViewModel[];
}
