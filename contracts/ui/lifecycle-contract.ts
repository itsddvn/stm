import type { SourceKind } from "./view-model-contract";

export type LifecycleResourceKind = SourceKind | "product" | "operation";
export type LifecyclePrivilege = "none" | "user_confirmation" | "elevation_required" | "vendor_controlled";
export type LifecycleItemStatus = "pending" | "in_progress" | "success" | "failed" | "cancelled" | "skipped";
export type LifecycleRevalidationState = "fresh" | "required" | "expired" | "evidence_changed";

export interface LifecycleChildIntent {
  resourceKind: LifecycleResourceKind;
  resourceId: string;
  desiredAction: string;
  mappingId?: string;
  dependsOn?: string[];
}

export interface LifecyclePlanRequest {
  resourceKind: LifecycleResourceKind;
  action: string;
  resourceId: string;
  sourceAnalysisHandle?: string;
  itemIds?: string[];
  children?: LifecycleChildIntent[];
  mappingId?: string;
}

interface LifecyclePlanBase<TRequest extends LifecyclePlanRequest = LifecyclePlanRequest> {
  request: TRequest;
  planId: string;
  canonicalId: string;
  mappingId: string;
  resourceId: string;
  owner: string;
  source: string;
  currentVersion: string;
  targetVersion: string;
  privilege: LifecyclePrivilege;
  affectedPaths: string[];
  affectedRecords: string[];
  confidence: string;
  limitations: string[];
  digest: string;
  expiresAt: string;
  revalidation: {
    state: LifecycleRevalidationState;
    checkedAt: string;
    checks: string[];
  };
}

export interface ManagedLifecyclePlan<TRequest extends LifecyclePlanRequest = LifecyclePlanRequest>
  extends LifecyclePlanBase<TRequest> {
  execution: {
    mode: "managed_execute" | "signed_product_update";
    executable: string;
    argv: string[];
  };
}

export interface NativeInstallerLifecyclePlan<TRequest extends LifecyclePlanRequest = LifecyclePlanRequest>
  extends LifecyclePlanBase<TRequest> {
  execution: {
    mode: "native_installer";
    executable: string;
    argv: string[];
    artifactSha256: string;
    signerTeamId: string;
    packageId: string;
    expectedVersion: string;
    previousReceiptInstallTime?: number;
  };
}

export interface ArchiveInstallerLifecyclePlan<TRequest extends LifecyclePlanRequest = LifecyclePlanRequest>
  extends LifecyclePlanBase<TRequest> {
  execution: {
    mode: "archive_installer";
    executable: string;
    argv: string[];
    archiveSha256: string;
    binarySha256: string;
    targetPath: string;
    expectedVersion: string;
  };
}

export interface ManagedConfigMutationLifecyclePlan<
  TRequest extends LifecyclePlanRequest = LifecyclePlanRequest,
> extends LifecyclePlanBase<TRequest> {
  execution: {
    mode: "managed_config_mutation";
    action: string;
  };
}

export interface VendorHandoffLifecyclePlan<TRequest extends LifecyclePlanRequest = LifecyclePlanRequest>
  extends LifecyclePlanBase<TRequest> {
  execution: {
    mode: "vendor_handoff";
    handoffTarget: string;
  };
}

export interface ReviewOnlyLifecyclePlan<TRequest extends LifecyclePlanRequest = LifecyclePlanRequest>
  extends LifecyclePlanBase<TRequest> {
  execution: {
    mode: "detect_only";
    guidance: string;
  };
}

export interface BatchLifecyclePlan<TRequest extends LifecyclePlanRequest = LifecyclePlanRequest>
  extends LifecyclePlanBase<TRequest> {
  execution: {
    mode: "batch";
    items: AtomicLifecyclePlan[];
  };
}

export type AtomicLifecyclePlan<TRequest extends LifecyclePlanRequest = LifecyclePlanRequest> =
  | ManagedLifecyclePlan<TRequest>
  | ArchiveInstallerLifecyclePlan<TRequest>
  | NativeInstallerLifecyclePlan<TRequest>
  | ManagedConfigMutationLifecyclePlan<TRequest>
  | VendorHandoffLifecyclePlan<TRequest>
  | ReviewOnlyLifecyclePlan<TRequest>;

export type LifecyclePlan<TRequest extends LifecyclePlanRequest = LifecyclePlanRequest> =
  | AtomicLifecyclePlan<TRequest>
  | BatchLifecyclePlan<TRequest>;

export interface LifecycleItemResult {
  id: string;
  label: string;
  status: LifecycleItemStatus;
  receipt?: string;
  redactedDetail: string;
}

export interface LifecycleFollowUpAction {
  id: string;
  label: string;
  planRequest: LifecyclePlanRequest;
}

export interface LifecycleConsentAuthorization {
  planDigest: string;
  planExpiresAt: string;
  grantedAt: string;
}

export interface LifecycleExecutionResult<TPlan extends LifecyclePlan = LifecyclePlan> {
  operationId: string;
  planDigest: TPlan["digest"];
  status: "in_progress" | "success" | "partial" | "failed" | "cancelled" | "recoverable";
  completedSteps: number;
  totalSteps: number;
  canCancel: boolean;
  receipt?: string;
  redactedDetail: string;
  items: LifecycleItemResult[];
  retryActions: LifecycleFollowUpAction[];
  recoveryActions: LifecycleFollowUpAction[];
}

export interface LifecycleIpcClient {
  prepareLifecycle<TRequest extends LifecyclePlanRequest>(request: TRequest): Promise<LifecyclePlan<TRequest>>;
  startLifecycle(planId: string, authorization: LifecycleConsentAuthorization): Promise<LifecycleExecutionResult>;
  getLifecycleStatus(operationId: string): Promise<LifecycleExecutionResult>;
  cancelLifecycle(operationId: string): Promise<LifecycleExecutionResult>;
}
