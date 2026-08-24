export type InstallProviderPreference = "automatic" | "prefer_homebrew" | "prefer_bun";
export type SetupRowAction = "install" | "update" | "installed" | "handoff" | "guidance" | "blocked";

export interface SetupRowView {
  id: string;
  name: string;
  summary: string;
  selected: boolean;
  optional: boolean;
  action: SetupRowAction;
  reason?: string;
  owner: string;
  mappingId?: string;
}

export interface ProviderStatusView {
  homebrew?: string;
  bun?: string;
  npm?: string;
}

export interface QuickSetupView {
  target: string;
  preference: InstallProviderPreference;
  dismissed: boolean;
  providers: ProviderStatusView;
  tools: SetupRowView[];
  optional: SetupRowView[];
}
export interface PortableSetupDocument {
  schemaVersion: 1;
  target: string;
  resources: Array<{ kind: string; id: string; desiredAction: string; credentialReferenceIds?: string[] }>;
}

export interface PortableImportResult {
  document: PortableSetupDocument;
  warnings: string[];
  reviewRequiredIds: string[];
}

export interface MigrationCandidate {
  recipe: {
    id: string;
    resourceId: string;
    sourceMappingId: string;
    targetMappingId: string;
    targetExecutablePaths: string[];
    sharedConfigIds: string[];
    cleanupOldOwnerDefault: boolean;
  };
  sourceOwner: string;
  targetOwner: string;
  cleanupOldOwner: boolean;
}
