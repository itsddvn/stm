use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    application::{
        adapters::FixtureWorkspace,
        catalog::load_tool_catalog,
        dto::{
            AppViewModelDto, McpServerViewModelDto, OperationViewModelDto, RefreshStatusDto,
            SkillViewModelDto, SourceAnalysisViewModelDto, SurfaceStateDto, ToolViewModelDto,
            UpdateViewModelDto,
        },
        events::{AppEvent, AppEventType},
        inventory::{catalog_inventory, scan_inventory, InventorySnapshot, ManagerScanReport},
        mcp::{discover_mcp, McpInventorySnapshot},
        skills::{scan_skills, SkillInventorySnapshot},
        storage::{OperationLogEntry, ScanErrorEntry, SnapshotBundle, StorageHealth},
        versioning::{build_application_updates, load_version_catalog, VersionCatalog},
    },
    capabilities::{current_target, resolve_setup},
    catalog::load_tool_catalog as load_capability_catalog,
    domain::portable::{
        is_valid_credential_reference, looks_like_machine_path, validate_portable_document,
        PortableImportResult, PortableResource, PortableSetupDocument,
    },
    domain::{
        inventory::{Freshness, InventoryState, LoadState, OwnershipKind, SurfaceStateContract},
        lifecycle::{
            LifecycleConsentAuthorization, LifecycleExecutionResult, LifecyclePlan,
            LifecyclePlanRequest,
        },
        mcp::McpDiscoveryReport,
        migration::{codex_npm_to_homebrew_recipe, MigrationCandidate},
        operation::{
            ConsentRecord, OperationPlan, OperationPlanStep, OperationReceipt,
            OperationResourceType,
        },
        provider::{
            DetectedProvider, InstallProviderPreference, PreferenceSnapshot, PreferencesStore,
            ProviderInventory, ProviderKind, ProviderTrust,
        },
        recipe::{VerifiedInstallerArtifact, PINNED_BUN_VERSION},
        skill::SkillScanReport,
        source::SourceKind,
    },
    error::CoreError,
    feasibility::{
        elevation::{strategy_for_current_host, ElevationStrategy},
        ui_contract::{verify_locked_ui_contract, UiContractVerification},
    },
    lifecycle::LifecycleService,
    ports::{LiveInventoryPort, SnapshotStore},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsReport {
    pub ui_contract: UiContractVerification,
    pub storage: StorageHealth,
    pub elevation: ElevationStrategy,
    pub catalog_version: String,
    pub managers: Vec<ManagerScanReport>,
    pub skills: SkillScanReport,
    pub mcp: McpDiscoveryReport,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteOperationRequest {
    pub operation_id: String,
    pub command_alias: String,
    pub args: Vec<String>,
    pub timeout_ms: u64,
    pub output_limit_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeadlessScanResult {
    pub snapshot: AppViewModelDto,
    pub status: RefreshStatusDto,
    pub diagnostics: DiagnosticsReport,
    pub events: Vec<AppEvent>,
    pub elevation_requested: bool,
}

pub struct PhaseThreeApplicationService {
    workspace: FixtureWorkspace,
    lifecycle: LifecycleService,
    preferences: Arc<dyn PreferencesStore>,
    storage: Arc<dyn SnapshotStore>,
    live_inventory: bool,
    live_inventory_reader: Option<Arc<dyn LiveInventoryPort>>,
}

impl PhaseThreeApplicationService {
    pub fn with_services(
        project_root: impl Into<PathBuf>,
        lifecycle: LifecycleService,
        storage: Arc<dyn SnapshotStore>,
        preferences: Arc<dyn PreferencesStore>,
        live_inventory_reader: Arc<dyn LiveInventoryPort>,
    ) -> Self {
        Self {
            workspace: FixtureWorkspace::new(project_root),
            lifecycle,
            storage,
            preferences,
            live_inventory: true,
            live_inventory_reader: Some(live_inventory_reader),
        }
    }

    pub fn with_fixture_services(
        project_root: impl Into<PathBuf>,
        lifecycle: LifecycleService,
        storage: Arc<dyn SnapshotStore>,
        preferences: Arc<dyn PreferencesStore>,
    ) -> Self {
        Self {
            workspace: FixtureWorkspace::new(project_root),
            lifecycle,
            storage,
            preferences,
            live_inventory: false,
            live_inventory_reader: None,
        }
    }

    #[cfg(test)]
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        let project_root = project_root.into();
        let workspace = FixtureWorkspace::new(project_root.clone());
        let lifecycle = LifecycleService::test_default(workspace);
        let storage = lifecycle.test_storage();
        Self {
            workspace: FixtureWorkspace::new(project_root),
            lifecycle,
            storage,
            preferences: Arc::new(crate::domain::provider::MemoryPreferencesStore::new()),
            live_inventory: false,
            live_inventory_reader: None,
        }
    }

    pub fn refresh_snapshot(&self) -> Result<AppViewModelDto, CoreError> {
        let (snapshot, _, warnings) = self.build_snapshot()?;
        Ok(self.to_app_view(&snapshot, &warnings))
    }

    pub fn refresh_status(&self) -> Result<RefreshStatusDto, CoreError> {
        let (snapshot, _, warnings) = self.ensure_snapshot()?;
        Ok(RefreshStatusDto {
            surface: self.surface_for_snapshot(&snapshot, &warnings),
            last_snapshot_at: snapshot.generated_at,
            warning_count: warnings.len(),
            warnings,
            in_progress: false,
            can_cancel: false,
            operation_id: None,
            current_step: None,
            steps_completed: 0,
            total_steps: 0,
            snapshot: None,
            result: None,
            error_message: None,
        })
    }

    pub fn headless_scan(&self) -> Result<HeadlessScanResult, CoreError> {
        let (snapshot, storage_health, warnings) = self.build_snapshot()?;
        let snapshot_view = self.to_app_view(&snapshot, &warnings);
        let status = RefreshStatusDto {
            surface: self.surface_for_snapshot(&snapshot, &warnings),
            last_snapshot_at: snapshot.generated_at.clone(),
            warning_count: warnings.len(),
            warnings,
            in_progress: false,
            can_cancel: false,
            operation_id: None,
            current_step: None,
            steps_completed: 0,
            total_steps: 0,
            snapshot: None,
            result: None,
            error_message: None,
        };

        Ok(HeadlessScanResult {
            snapshot: snapshot_view,
            status,
            diagnostics: self.diagnostics()?,
            events: build_headless_scan_events(&storage_health),
            elevation_requested: false,
        })
    }

    pub fn list_tools(&self) -> Result<Vec<ToolViewModelDto>, CoreError> {
        let (snapshot, _, _) = self.ensure_snapshot()?;
        Ok(snapshot.tools.iter().map(ToolViewModelDto::from).collect())
    }

    pub fn get_tool_detail(&self, id: &str) -> Result<Option<ToolViewModelDto>, CoreError> {
        Ok(self.list_tools()?.into_iter().find(|tool| tool.id == id))
    }

    pub fn list_skills(&self) -> Result<Vec<SkillViewModelDto>, CoreError> {
        let (snapshot, _, _) = self.ensure_snapshot()?;
        Ok(snapshot
            .skills
            .iter()
            .map(SkillViewModelDto::from)
            .collect())
    }

    pub fn get_skill_detail(&self, id: &str) -> Result<Option<SkillViewModelDto>, CoreError> {
        Ok(self.list_skills()?.into_iter().find(|skill| skill.id == id))
    }

    pub fn list_mcp_servers(&self) -> Result<Vec<McpServerViewModelDto>, CoreError> {
        let (snapshot, _, _) = self.ensure_snapshot()?;
        Ok(snapshot
            .mcp_servers
            .iter()
            .map(McpServerViewModelDto::from)
            .collect())
    }

    pub fn get_mcp_detail(&self, id: &str) -> Result<Option<McpServerViewModelDto>, CoreError> {
        Ok(self
            .list_mcp_servers()?
            .into_iter()
            .find(|server| server.id == id))
    }

    pub fn list_updates(&self) -> Result<Vec<UpdateViewModelDto>, CoreError> {
        let (snapshot, _, _) = self.ensure_snapshot()?;
        Ok(snapshot
            .updates
            .iter()
            .map(UpdateViewModelDto::from)
            .collect())
    }

    pub fn list_operations(&self) -> Result<Vec<OperationViewModelDto>, CoreError> {
        let (snapshot, _, _) = self.ensure_snapshot()?;
        Ok(snapshot
            .operations
            .iter()
            .map(OperationViewModelDto::from)
            .collect())
    }

    pub fn analyze_source(
        &self,
        kind: SourceKind,
        url: &str,
    ) -> Result<SourceAnalysisViewModelDto, CoreError> {
        let (analysis, lifecycle_request) = self.lifecycle.analyze_source(kind, url)?;
        Ok(SourceAnalysisViewModelDto::from_analysis(
            analysis,
            lifecycle_request,
        ))
    }

    pub fn prepare_lifecycle(
        &self,
        request: LifecyclePlanRequest,
    ) -> Result<LifecyclePlan, CoreError> {
        self.prepare_lifecycle_with_providers(request, ProviderInventory::default())
    }

    pub fn prepare_lifecycle_with_providers(
        &self,
        mut request: LifecyclePlanRequest,
        providers: ProviderInventory,
    ) -> Result<LifecyclePlan, CoreError> {
        if request.action == "migrate-cleanup-retry" {
            let homebrew_approved = providers
                .homebrew
                .as_ref()
                .is_some_and(|provider| provider.trust == ProviderTrust::ApprovedRoot);
            if !homebrew_approved {
                return Err(CoreError::LifecycleConsentDenied(
                    "migration cleanup retry requires an approved Homebrew provider".to_string(),
                ));
            }
            return self.lifecycle.prepare_codex_cleanup_retry(request);
        }
        if request.action == "inspect-migration" {
            return self.lifecycle.prepare_codex_migration_inspection(request);
        }
        if matches!(
            request.action.as_str(),
            "migrate-with-cleanup" | "migrate-keep-source"
        ) {
            let eligible = self
                .migration_candidates(&providers)?
                .iter()
                .any(|candidate| candidate.recipe.resource_id == request.resource_id);
            if !eligible {
                return Err(CoreError::LifecycleConsentDenied(
                    "migration is not eligible under fresh owner and provider evidence".to_string(),
                ));
            }
            let cleanup = request.action == "migrate-with-cleanup";
            return self.lifecycle.prepare_codex_migration(request, cleanup);
        }
        if request.resource_kind == crate::domain::lifecycle::LifecycleResourceKind::Tool {
            request.mapping_id = None;
        }
        let request = self.normalize_setup_request(request, providers)?;
        self.lifecycle.prepare(request)
    }

    pub fn setup_queue_bootstrap_requirements(
        &self,
        request: &LifecyclePlanRequest,
        providers: &ProviderInventory,
    ) -> Result<Vec<ProviderKind>, CoreError> {
        if request.action != "setup-queue" {
            return Ok(Vec::new());
        }
        let planned = with_planned_bun(with_planned_homebrew(providers.clone()));
        let (_, rows) = self.resolve_setup_rows(planned)?;
        let requested = requested_setup_ids(request);
        let mappings = rows
            .iter()
            .filter(|row| requested.iter().any(|id| id == &row.id))
            .filter_map(|row| row.mapping_id.as_deref())
            .collect::<Vec<_>>();
        let mut requirements = Vec::with_capacity(2);
        if providers.trusted(ProviderKind::Homebrew).is_none()
            && mappings
                .iter()
                .any(|mapping| mapping.starts_with("homebrew:"))
        {
            requirements.push(ProviderKind::Homebrew);
        }
        if providers.trusted(ProviderKind::Bun).is_none()
            && mappings.iter().any(|mapping| mapping.starts_with("bun:"))
        {
            requirements.push(ProviderKind::Bun);
        }
        Ok(requirements)
    }

    pub fn prepare_lifecycle_with_provider_bootstraps(
        &self,
        request: LifecyclePlanRequest,
        providers: ProviderInventory,
        homebrew_artifact: Option<&VerifiedInstallerArtifact>,
        bun_artifact: Option<&crate::domain::recipe::VerifiedArchiveBinary>,
    ) -> Result<LifecyclePlan, CoreError> {
        let planned = match (homebrew_artifact.is_some(), bun_artifact.is_some()) {
            (true, true) => with_planned_bun(with_planned_homebrew(providers)),
            (true, false) => with_planned_homebrew(providers),
            (false, true) => with_planned_bun(providers),
            (false, false) => {
                return Err(CoreError::MalformedInput(
                    "provider bootstrap artifacts are missing".to_string(),
                ))
            }
        };
        let (snapshot, rows) = self.resolve_setup_rows(planned)?;
        let catalog = load_capability_catalog(&self.workspace)?;
        let request = crate::capabilities::InstallerService::normalize_setup_queue(
            request,
            &snapshot.tools,
            &catalog,
            &rows,
        )?;
        self.lifecycle.prepare_setup_with_provider_bootstraps(
            request,
            homebrew_artifact,
            bun_artifact,
        )
    }

    fn normalize_setup_request(
        &self,
        request: LifecyclePlanRequest,
        providers: ProviderInventory,
    ) -> Result<LifecyclePlanRequest, CoreError> {
        if request.action != "setup-queue" {
            return Ok(request);
        }
        let (snapshot, rows) = self.resolve_setup_rows(providers)?;
        let catalog = load_capability_catalog(&self.workspace)?;
        crate::capabilities::InstallerService::normalize_setup_queue(
            request,
            &snapshot.tools,
            &catalog,
            &rows,
        )
    }

    fn resolve_setup_rows(
        &self,
        providers: ProviderInventory,
    ) -> Result<(SnapshotBundle, Vec<crate::domain::setup::SetupRow>), CoreError> {
        let (snapshot, _, _) = self.ensure_snapshot()?;
        let catalog = load_capability_catalog(&self.workspace)?;
        let prefs = self.preferences.load();
        let setup = resolve_setup(
            &catalog,
            &snapshot.tools,
            &current_target(),
            prefs.provider_preference,
            providers,
            prefs.quick_setup_dismissed,
        )?;
        let mut rows = setup.tools;
        rows.extend(setup.optional_mcp);
        Ok((snapshot, rows))
    }

    pub fn migration_candidates(
        &self,
        providers: &ProviderInventory,
    ) -> Result<Vec<MigrationCandidate>, CoreError> {
        let Some(homebrew) = providers.homebrew.as_ref() else {
            return Ok(Vec::new());
        };
        if homebrew.trust != ProviderTrust::ApprovedRoot {
            return Ok(Vec::new());
        }
        let (snapshot, _, _) = self.build_snapshot()?;
        let Some(codex) = snapshot.tools.iter().find(|tool| tool.id == "codex-cli") else {
            return Ok(Vec::new());
        };
        if codex.installed_version.is_none()
            || !codex.manager.eq_ignore_ascii_case("npm")
            || codex.package_id != "@openai/codex"
        {
            return Ok(Vec::new());
        }
        Ok(vec![MigrationCandidate {
            recipe: codex_npm_to_homebrew_recipe(),
            source_owner: codex.owner.clone(),
            target_owner: "Homebrew".to_string(),
            cleanup_old_owner: true,
        }])
    }

    pub fn native_confirmation_summary(
        &self,
        plan_id: &str,
        locale: &str,
    ) -> Result<String, CoreError> {
        self.lifecycle.native_confirmation_summary(plan_id, locale)
    }

    pub fn start_lifecycle(
        &self,
        plan_id: &str,
        authorization: LifecycleConsentAuthorization,
    ) -> Result<LifecycleExecutionResult, CoreError> {
        self.lifecycle.start(plan_id, authorization)
    }

    pub fn lifecycle_status(
        &self,
        operation_id: &str,
    ) -> Result<LifecycleExecutionResult, CoreError> {
        self.lifecycle.status(operation_id)
    }

    pub fn cancel_lifecycle(
        &self,
        operation_id: &str,
    ) -> Result<LifecycleExecutionResult, CoreError> {
        self.lifecycle.cancel(operation_id)
    }

    pub fn current_snapshot(&self) -> Result<AppViewModelDto, CoreError> {
        let (snapshot, _, warnings) = self.ensure_snapshot()?;
        Ok(self.to_app_view(&snapshot, &warnings))
    }

    pub fn refresh_snapshot_with_progress<F, C>(
        &self,
        mut emit_progress: F,
        is_cancelled: C,
    ) -> Result<AppViewModelDto, CoreError>
    where
        F: FnMut(AppEvent),
        C: Fn() -> bool,
    {
        let (snapshot, _, warnings) =
            self.build_snapshot_with_progress(&mut emit_progress, &is_cancelled)?;
        Ok(self.to_app_view(&snapshot, &warnings))
    }

    pub fn generate_operation_plan(
        &self,
        resource_type: OperationResourceType,
        resource_id: &str,
        action: &str,
    ) -> OperationPlan {
        OperationPlan {
            id: format!("{resource_id}-{action}-read-only-plan"),
            resource_type,
            resource_id: resource_id.to_string(),
            action: action.to_string(),
            execution_mode: crate::domain::inventory::ExecutionMode::DetectOnly,
            ownership_kind: OwnershipKind::Unknown,
            requires_consent: false,
            warnings: vec!["Phase 3 is read-only. Mutation planning remains disabled.".to_string()],
            steps: vec![OperationPlanStep {
                id: "inspect".to_string(),
                label: "Inspect read-only boundary".to_string(),
                detail: "Review authoritative owner, mapping status, and diagnostics.".to_string(),
            }],
        }
    }

    pub fn record_consent(&self, operation_id: &str, actor: &str, granted: bool) -> ConsentRecord {
        ConsentRecord {
            operation_id: operation_id.to_string(),
            granted,
            actor: actor.to_string(),
            recorded_at: "2026-08-20T00:00:00Z".to_string(),
        }
    }

    pub fn execute_operation(
        &self,
        request: ExecuteOperationRequest,
    ) -> Result<OperationReceipt, CoreError> {
        Err(CoreError::ProcessExecution(format!(
            "operation {} rejected: Phase 3 remains read-only for {} {:?}",
            request.operation_id, request.command_alias, request.args
        )))
    }

    pub fn cancel_operation(&self, _operation_id: &str) -> bool {
        false
    }

    pub fn diagnostics(&self) -> Result<DiagnosticsReport, CoreError> {
        let ui_contract = verify_locked_ui_contract(self.workspace.project_root())?;
        let catalog = load_tool_catalog(&self.workspace)?;
        let versions = load_version_catalog(&self.workspace)?;
        let inventory = scan_inventory(&self.workspace, &catalog, &versions)?;
        let skills = scan_skills(&self.workspace, &versions)?;
        let mcp = discover_mcp(&self.workspace)?;
        let storage_health = self.storage.health();

        let mut warnings = inventory.warnings.clone();
        warnings.extend(skills.report.warnings.clone());
        warnings.extend(mcp.report.warnings.clone());

        Ok(DiagnosticsReport {
            ui_contract,
            storage: storage_health,
            elevation: strategy_for_current_host(),
            catalog_version: catalog.version,
            managers: inventory.managers,
            skills: skills.report,
            mcp: mcp.report,
            warnings,
        })
    }

    pub fn import_portable_bytes(&self, bytes: &[u8]) -> Result<PortableImportResult, CoreError> {
        let document =
            PortableSetupDocument::validate_bytes(bytes).map_err(CoreError::MalformedInput)?;
        let warnings = validate_portable_document(&document, &current_target())?;
        let catalog = load_capability_catalog(&self.workspace)?;
        let mut review_required_ids = Vec::new();
        for resource in &document.resources {
            if looks_like_machine_path(&resource.id) {
                return Err(CoreError::MalformedInput(
                    "portable resource IDs may not contain machine paths".to_string(),
                ));
            }
            if !matches!(
                resource.desired_action.as_str(),
                "keep" | "install" | "update" | "enable" | "add" | "review"
            ) {
                return Err(CoreError::MalformedInput(format!(
                    "unsupported portable desired action: {}",
                    resource.desired_action
                )));
            }
            let known = match resource.kind.as_str() {
                "tool" => catalog.get(&resource.id).is_some(),
                "skill" | "mcp" => false,
                _ => {
                    return Err(CoreError::MalformedInput(format!(
                        "unsupported portable resource kind: {}",
                        resource.kind
                    )))
                }
            };
            if !known {
                review_required_ids.push(resource.id.clone());
            }
        }
        Ok(PortableImportResult {
            document,
            warnings,
            review_required_ids,
        })
    }

    pub fn export_portable_setup(&self, target: &str) -> Result<Vec<u8>, CoreError> {
        if crate::catalog::load_platform_profiles()?
            .for_target(target)
            .is_none()
        {
            return Err(CoreError::MalformedInput(format!(
                "unsupported portable target: {target}"
            )));
        }
        let (snapshot, _, _) = self.build_snapshot()?;
        if snapshot.freshness != Freshness::Fresh {
            return Err(CoreError::MalformedInput(
                "portable export requires a fresh authoritative scan".to_string(),
            ));
        }
        let mut resources = snapshot
            .tools
            .iter()
            .filter(|tool| tool.installed_version.is_some())
            .map(|tool| PortableResource {
                kind: "tool".to_string(),
                id: tool.id.clone(),
                desired_action: "keep".to_string(),
                credential_reference_ids: Vec::new(),
            })
            .collect::<Vec<_>>();
        resources.extend(snapshot.skills.iter().map(|skill| PortableResource {
            kind: "skill".to_string(),
            id: skill.id.clone(),
            desired_action: "keep".to_string(),
            credential_reference_ids: Vec::new(),
        }));
        resources.extend(snapshot.mcp_servers.iter().map(|server| {
            PortableResource {
                kind: "mcp".to_string(),
                id: server.id.clone(),
                desired_action: "enable".to_string(),
                credential_reference_ids: server
                    .auth_references
                    .iter()
                    .filter(|reference| {
                        !matches!(
                            reference.kind,
                            crate::domain::mcp::AuthReferenceKind::FileReference
                        ) && is_valid_credential_reference(&reference.reference)
                    })
                    .map(|reference| reference.reference.clone())
                    .collect(),
            }
        }));
        let document = PortableSetupDocument {
            schema_version: crate::domain::portable::PORTABLE_SCHEMA_VERSION,
            target: target.to_string(),
            resources,
        };
        let bytes = document
            .to_safe_json_bytes()
            .map_err(CoreError::MalformedInput)?;
        PortableSetupDocument::validate_bytes(&bytes).map_err(CoreError::MalformedInput)?;
        Ok(bytes)
    }

    pub fn quick_setup(
        &self,
        providers: ProviderInventory,
    ) -> Result<crate::capabilities::QuickSetupView, CoreError> {
        let (snapshot, _, _) = if self.live_inventory {
            self.build_snapshot()?
        } else {
            self.ensure_snapshot()?
        };
        let catalog = load_capability_catalog(&self.workspace)?;
        let prefs = self.preferences.load();
        let actual_providers = providers.clone();
        let planning_providers = if self.live_inventory {
            with_planned_bun(with_planned_homebrew(providers))
        } else {
            providers
        };
        let mut view = resolve_setup(
            &catalog,
            &snapshot.tools,
            &current_target(),
            prefs.provider_preference,
            planning_providers,
            prefs.quick_setup_dismissed,
        )?;
        view.providers = actual_providers;
        Ok(view)
    }

    pub fn set_provider_preference(
        &self,
        preference: InstallProviderPreference,
    ) -> Result<(), CoreError> {
        self.preferences
            .set_provider_preference(preference)
            .map(|_| ())
            .map_err(CoreError::MalformedInput)
    }

    pub fn dismiss_quick_setup(&self) -> Result<(), CoreError> {
        self.preferences
            .dismiss_quick_setup()
            .map(|_| ())
            .map_err(CoreError::MalformedInput)
    }

    pub fn setup_preferences(&self) -> PreferenceSnapshot {
        self.preferences.load()
    }

    pub fn validate_portable_bytes(
        &self,
        bytes: &[u8],
    ) -> Result<(PortableSetupDocument, Vec<String>), CoreError> {
        let document =
            PortableSetupDocument::validate_bytes(bytes).map_err(CoreError::MalformedInput)?;
        let warnings = validate_portable_document(&document, &current_target())?;
        Ok((document, warnings))
    }

    pub fn validate_portable_setup(
        &self,
        document: &PortableSetupDocument,
        current_target: &str,
    ) -> Result<Vec<String>, CoreError> {
        validate_portable_document(document, current_target)
    }

    fn ensure_snapshot(&self) -> Result<(SnapshotBundle, StorageHealth, Vec<String>), CoreError> {
        let store = &self.storage;
        let health = store.health();
        if let Some(mut snapshot) = store.load_snapshot()? {
            self.merge_lifecycle_receipts(store.as_ref(), &mut snapshot)?;
            let warnings = snapshot
                .errors
                .iter()
                .map(|error| format!("{}: {}", error.code, error.detail))
                .collect();
            Ok((snapshot, health, warnings))
        } else {
            self.build_snapshot()
        }
    }

    fn build_snapshot(&self) -> Result<(SnapshotBundle, StorageHealth, Vec<String>), CoreError> {
        self.build_snapshot_with_progress(&mut |_| {}, &|| false)
    }

    fn build_snapshot_with_progress<F, C>(
        &self,
        emit_progress: &mut F,
        is_cancelled: &C,
    ) -> Result<(SnapshotBundle, StorageHealth, Vec<String>), CoreError>
    where
        F: FnMut(AppEvent),
        C: Fn() -> bool,
    {
        check_refresh_cancelled(is_cancelled, "refresh-start")?;
        emit_progress(app_event(
            "phase-three-refresh-started",
            AppEventType::RefreshStarted,
            if self.live_inventory {
                "Started live tool and provider inventory scan."
            } else {
                "Started fixture-backed Phase 3 inventory scan."
            },
        ));
        let catalog = load_tool_catalog(&self.workspace)?;
        emit_progress(app_event(
            "phase-three-catalog-validated",
            AppEventType::CatalogValidated,
            "Validated catalog schemas and semantic invariants.",
        ));
        check_refresh_cancelled(is_cancelled, "catalog-validated")?;
        let versions = if let Some(reader) = &self.live_inventory_reader {
            reader.load_version_catalog(&self.workspace)?
        } else if self.live_inventory {
            VersionCatalog::default()
        } else {
            load_version_catalog(&self.workspace)?
        };
        let mut inventory = if self.live_inventory {
            catalog_inventory(&catalog)
        } else {
            scan_inventory(&self.workspace, &catalog, &versions)?
        };
        let mut live_warnings = Vec::new();
        if self.live_inventory {
            let lifecycle = &self.lifecycle;
            live_warnings = std::thread::scope(|scope| {
                let handles = inventory
                    .tools
                    .iter_mut()
                    .filter(|tool| tool.recommended)
                    .map(|tool| {
                        scope.spawn(move || {
                            let tool_id = tool.id.clone();
                            let tool_name = tool.name.clone();
                            lifecycle
                                .refresh_tool_inventory(&tool_id, tool)
                                .err()
                                .map(|error| {
                                    tool.installed_version = None;
                                    tool.available_version = None;
                                    tool.state = InventoryState::Unknown;
                                    tool.reason_code = Some(
                                        "inventory.live_evidence_unavailable".to_string(),
                                    );
                                    tool.lifecycle_confidence =
                                        "Live manager evidence unavailable; mutation remains blocked"
                                            .to_string();
                                    format!("{tool_name}: {error}")
                                })
                        })
                    })
                    .collect::<Vec<_>>();
                handles
                    .into_iter()
                    .filter_map(|handle| handle.join().ok().flatten())
                    .collect::<Vec<_>>()
            });
            if !live_warnings.is_empty() {
                inventory.freshness = Freshness::Stale;
            }
        }
        emit_progress(app_event(
            "phase-three-inventory-scanned",
            AppEventType::InventoryScanned,
            "Completed manager inventory and allowlisted probe reconciliation.",
        ));
        check_refresh_cancelled(is_cancelled, "inventory-scanned")?;
        let skills = if let Some(reader) = &self.live_inventory_reader {
            reader.scan_skills(&self.workspace, &versions)?
        } else if self.live_inventory {
            SkillInventorySnapshot {
                skills: Vec::new(),
                report: SkillScanReport {
                    roots: Vec::new(),
                    skills: Vec::new(),
                    warnings: Vec::new(),
                },
            }
        } else {
            scan_skills(&self.workspace, &versions)?
        };
        emit_progress(app_event(
            "phase-three-skills-scanned",
            AppEventType::SkillsScanned,
            "Completed bounded global skill-root scan.",
        ));
        check_refresh_cancelled(is_cancelled, "skills-scanned")?;
        let mcp = if let Some(reader) = &self.live_inventory_reader {
            reader.discover_mcp(&self.workspace)?
        } else if self.live_inventory {
            McpInventorySnapshot {
                servers: Vec::new(),
                report: McpDiscoveryReport {
                    servers: Vec::new(),
                    malformed_entries: Vec::new(),
                    warnings: Vec::new(),
                },
            }
        } else {
            discover_mcp(&self.workspace)?
        };
        emit_progress(app_event(
            "phase-three-mcp-discovered",
            AppEventType::McpDiscovered,
            "Completed read-only MCP client discovery and redaction.",
        ));
        check_refresh_cancelled(is_cancelled, "mcp-discovered")?;
        let (skill_records, mcp_servers, operations, errors, mut warnings) = if self.live_inventory
        {
            let errors = self.collect_scan_errors(&skills, &mcp);
            let mut warnings = warnings_for_scan(&inventory, &skills, &mcp, &errors);
            warnings.extend(live_warnings);
            (skills.skills, mcp.servers, Vec::new(), errors, warnings)
        } else {
            let operations: Vec<OperationLogEntry> = self
                .workspace
                .read_json("tests/fixtures/catalog/operations.json")?;
            let errors = self.collect_scan_errors(&skills, &mcp);
            let warnings = warnings_for_scan(&inventory, &skills, &mcp, &errors);
            (skills.skills, mcp.servers, operations, errors, warnings)
        };
        let updates = build_application_updates(&inventory.tools, &skill_records, &versions);

        let mut snapshot = SnapshotBundle {
            generated_at: if self.live_inventory {
                crate::lifecycle::time::format_timestamp(std::time::SystemTime::now())?
            } else {
                "2026-08-20T09:00:00+07:00".to_string()
            },
            catalog_version: catalog.version.clone(),
            freshness: inventory.freshness.clone(),
            tools: inventory.tools,
            skills: skill_records,
            mcp_servers,
            updates,
            operations,
            errors,
        };

        let store = &self.storage;
        let health = store.health();
        self.lifecycle.with_snapshot_merge(|| {
            if let Some(previous) = store.load_snapshot()? {
                let pending = merge_authoritative_lifecycle_tool_state(
                    &previous,
                    &mut snapshot,
                    &versions,
                    |tool_id, tool| self.lifecycle.refresh_tool_postcondition(tool_id, tool),
                );
                if pending {
                    snapshot.freshness = Freshness::Stale;
                    warnings.push(
                        "A verified lifecycle postcondition remains preserved while live manager evidence is unavailable."
                            .to_string(),
                    );
                }
            }
            store.persist_snapshot(&snapshot)?;
            self.merge_lifecycle_receipts(store.as_ref(), &mut snapshot)
        })?;
        if health.recovered_from_corruption {
            emit_progress(app_event(
                "phase-three-snapshot-recovered",
                AppEventType::SnapshotRecovered,
                "Recovered the last good read-only snapshot after SQLite corruption detection.",
            ));
        }
        emit_progress(app_event(
            "phase-three-snapshot-committed",
            AppEventType::SnapshotCommitted,
            "Committed the read-only snapshot transaction.",
        ));
        emit_progress(app_event(
            "phase-three-diagnostics-ready",
            AppEventType::DiagnosticsReady,
            "Prepared diagnostics and zero-elevation evidence for headless consumption.",
        ));
        Ok((snapshot, health, warnings))
    }

    fn collect_scan_errors(
        &self,
        skills: &SkillInventorySnapshot,
        mcp: &McpInventorySnapshot,
    ) -> Vec<ScanErrorEntry> {
        let mut errors = Vec::new();
        for skill in &skills.report.skills {
            if let Some(reason) = &skill.rejected_reason {
                errors.push(ScanErrorEntry {
                    scope: "skills".to_string(),
                    code: reason.clone(),
                    detail: skill.slug.clone(),
                });
            }
        }
        for entry in &mcp.report.malformed_entries {
            errors.push(ScanErrorEntry {
                scope: "mcp".to_string(),
                code: "malformed_entry".to_string(),
                detail: format!("{} ({})", entry.entry_name, entry.reason),
            });
        }
        errors
    }
    fn merge_lifecycle_receipts(
        &self,
        store: &dyn SnapshotStore,
        snapshot: &mut SnapshotBundle,
    ) -> Result<(), CoreError> {
        let receipts = store.load_lifecycle_receipts()?;
        for receipt in receipts {
            if snapshot
                .operations
                .iter()
                .all(|operation| operation.receipt.operation_id != receipt.receipt.operation_id)
            {
                snapshot.operations.insert(0, receipt);
            }
        }
        Ok(())
    }

    fn surface_for_snapshot(
        &self,
        snapshot: &SnapshotBundle,
        warnings: &[String],
    ) -> SurfaceStateDto {
        let load_state = if snapshot.tools.is_empty()
            && snapshot.skills.is_empty()
            && snapshot.mcp_servers.is_empty()
        {
            LoadState::Empty
        } else if !snapshot.errors.is_empty() {
            LoadState::Partial
        } else {
            LoadState::Ready
        };

        let reason_code = if load_state == LoadState::Empty {
            Some("inventory.empty".to_string())
        } else if !snapshot.errors.is_empty() {
            Some("inventory.partial".to_string())
        } else if snapshot.freshness == Freshness::Stale {
            Some("inventory.stale".to_string())
        } else if warnings
            .iter()
            .any(|warning| warning.contains("auth_reference_missing"))
        {
            Some("mcp.auth_reference_missing".to_string())
        } else {
            None
        };

        SurfaceStateDto::from(SurfaceStateContract {
            load_state,
            reason_code,
            freshness: snapshot.freshness.clone(),
        })
    }

    fn to_app_view(&self, snapshot: &SnapshotBundle, warnings: &[String]) -> AppViewModelDto {
        AppViewModelDto {
            surface: self.surface_for_snapshot(snapshot, warnings),
            tools: snapshot.tools.iter().map(ToolViewModelDto::from).collect(),
            skills: snapshot
                .skills
                .iter()
                .map(SkillViewModelDto::from)
                .collect(),
            mcp_servers: snapshot
                .mcp_servers
                .iter()
                .map(McpServerViewModelDto::from)
                .collect(),
            updates: snapshot
                .updates
                .iter()
                .map(UpdateViewModelDto::from)
                .collect(),
            operations: snapshot
                .operations
                .iter()
                .map(OperationViewModelDto::from)
                .collect(),
        }
    }
}

fn merge_authoritative_lifecycle_tool_state<F>(
    previous: &SnapshotBundle,
    current: &mut SnapshotBundle,
    versions: &VersionCatalog,
    mut refresh: F,
) -> bool
where
    F: FnMut(&str, &mut crate::domain::tool::ToolRecord) -> Result<bool, CoreError>,
{
    let mut pending = false;
    for previous_tool in previous
        .tools
        .iter()
        .filter(|tool| tool.lifecycle_confidence.starts_with("Live "))
    {
        if let Some(current_tool) = current
            .tools
            .iter_mut()
            .find(|tool| tool.id == previous_tool.id)
        {
            if !matches!(refresh(&previous_tool.id, current_tool), Ok(true)) {
                let mut carried = previous_tool.clone();
                carried.lifecycle_confidence = format!(
                    "Live post-operation evidence pending scan convergence: {}",
                    previous_tool.lifecycle_confidence
                );
                *current_tool = carried;
                pending = true;
            }
        }
    }

    current.updates = build_application_updates(&current.tools, &current.skills, versions);
    pending
}
fn with_planned_bun(mut providers: ProviderInventory) -> ProviderInventory {
    if providers.trusted(ProviderKind::Bun).is_none() {
        providers.bun = Some(DetectedProvider {
            kind: ProviderKind::Bun,
            path: format!("stm-user-data/providers/bun/{PINNED_BUN_VERSION}/bin/bun"),
            version: Some(PINNED_BUN_VERSION.to_string()),
            trust: ProviderTrust::ApprovedRoot,
        });
    }
    providers
}

fn check_refresh_cancelled<C>(is_cancelled: &C, stage: &str) -> Result<(), CoreError>
where
    C: Fn() -> bool,
{
    if is_cancelled() {
        Err(CoreError::ProcessExecution(format!(
            "refresh cancelled before {stage}"
        )))
    } else {
        Ok(())
    }
}

fn with_planned_homebrew(mut providers: ProviderInventory) -> ProviderInventory {
    if providers.trusted(ProviderKind::Homebrew).is_none() {
        providers.homebrew = Some(DetectedProvider {
            kind: ProviderKind::Homebrew,
            path: "/opt/homebrew/bin/brew".to_string(),
            version: None,
            trust: ProviderTrust::ApprovedRoot,
        });
    }
    providers
}

fn requested_setup_ids(request: &LifecyclePlanRequest) -> Vec<String> {
    if !request.children.is_empty() {
        return request
            .children
            .iter()
            .filter(|child| {
                child.resource_kind == crate::domain::lifecycle::LifecycleResourceKind::Tool
            })
            .map(|child| child.resource_id.trim_start_matches("update-").to_string())
            .collect();
    }
    request
        .item_ids
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|id| id.trim_start_matches("update-").to_string())
        .collect()
}

fn warnings_for_scan(
    inventory: &InventorySnapshot,
    skills: &SkillInventorySnapshot,
    mcp: &McpInventorySnapshot,
    errors: &[ScanErrorEntry],
) -> Vec<String> {
    let mut warnings = inventory.warnings.clone();
    warnings.extend(skills.report.warnings.clone());
    warnings.extend(mcp.report.warnings.clone());
    warnings.extend(
        errors
            .iter()
            .map(|error| format!("{}:{}", error.scope, error.code)),
    );
    warnings
}

fn build_headless_scan_events(storage_health: &StorageHealth) -> Vec<AppEvent> {
    let mut events = vec![
        app_event(
            "phase-three-refresh-started",
            AppEventType::RefreshStarted,
            "Started fixture-backed Phase 3 inventory scan.",
        ),
        app_event(
            "phase-three-catalog-validated",
            AppEventType::CatalogValidated,
            "Validated catalog schemas and semantic invariants.",
        ),
        app_event(
            "phase-three-inventory-scanned",
            AppEventType::InventoryScanned,
            "Completed manager inventory and allowlisted probe reconciliation.",
        ),
        app_event(
            "phase-three-skills-scanned",
            AppEventType::SkillsScanned,
            "Completed bounded global skill-root scan.",
        ),
        app_event(
            "phase-three-mcp-discovered",
            AppEventType::McpDiscovered,
            "Completed read-only MCP client discovery and redaction.",
        ),
    ];

    if storage_health.recovered_from_corruption {
        events.push(app_event(
            "phase-three-snapshot-recovered",
            AppEventType::SnapshotRecovered,
            "Recovered the last good read-only snapshot after SQLite corruption detection.",
        ));
    }

    events.push(app_event(
        "phase-three-snapshot-committed",
        AppEventType::SnapshotCommitted,
        "Committed the read-only snapshot transaction.",
    ));
    events.push(app_event(
        "phase-three-diagnostics-ready",
        AppEventType::DiagnosticsReady,
        "Prepared diagnostics and zero-elevation evidence for headless consumption.",
    ));

    events
}

fn app_event(id: &str, event_type: AppEventType, message: &str) -> AppEvent {
    AppEvent {
        id: id.to_string(),
        event_type,
        operation_id: None,
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::application::events::AppEventType;
    use crate::domain::inventory::InventoryState;

    #[test]
    fn refresh_builds_read_only_snapshot() {
        let service = PhaseThreeApplicationService::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
        );
        let snapshot = service.refresh_snapshot().expect("snapshot");
        assert!(snapshot.tools.iter().any(|tool| tool.recommended));
        assert!(snapshot.tools.iter().any(|tool| tool.id == "cursor"));
        assert!(snapshot
            .skills
            .iter()
            .any(|skill| skill.id == "frontend-design"));
        assert!(snapshot
            .mcp_servers
            .iter()
            .any(|server| server.id == "github"));
    }

    #[test]
    fn diagnostics_report_zero_elevation_and_malformed_isolation() {
        let service = PhaseThreeApplicationService::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
        );
        let diagnostics = service.diagnostics().expect("diagnostics");
        assert!(!diagnostics.ui_contract.locked);
        assert!(diagnostics
            .mcp
            .malformed_entries
            .iter()
            .any(|entry| entry.entry_name == "Broken Entry"));
    }

    #[test]
    fn refresh_status_and_updates_preserve_read_only_contract() {
        let service = PhaseThreeApplicationService::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
        );

        let status = service.refresh_status().expect("status");
        assert_eq!(status.surface.load_state, LoadState::Partial);
        assert_eq!(
            status.surface.reason_code.as_deref(),
            Some("inventory.partial")
        );

        let snapshot = service.refresh_snapshot().expect("snapshot");
        let release_pilot = snapshot
            .updates
            .iter()
            .find(|update| update.name == "Release Pilot")
            .expect("release pilot update");
        assert_eq!(
            release_pilot
                .selection_action
                .as_ref()
                .and_then(|action| action.disabled_reason_code.as_deref()),
            Some("action.update.conflict_resolution_required")
        );

        let sentry = snapshot
            .mcp_servers
            .iter()
            .find(|server| server.id == "sentry")
            .expect("sentry");
        assert_eq!(
            sentry.state,
            crate::domain::inventory::InventoryState::Blocked
        );
    }

    #[test]
    fn authoritative_lifecycle_merge_rebuilds_the_update_queue() {
        let service = PhaseThreeApplicationService::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
        );
        let (mut current, _, _) = service.build_snapshot().expect("snapshot");
        let mut previous = current.clone();

        let versions = load_version_catalog(&service.workspace).expect("versions");
        let expected_version = Some(
            versions
                .tool_updates
                .get("codex-cli")
                .expect("codex update evidence")
                .target_version
                .clone(),
        );
        let tool = previous
            .tools
            .iter_mut()
            .find(|tool| tool.id == "codex-cli")
            .expect("codex tool");
        tool.installed_version = expected_version.clone();
        tool.available_version = expected_version.clone();
        tool.state = InventoryState::ManagedCurrent;
        tool.lifecycle_confidence = "Live manager postcondition".to_string();

        let pending =
            merge_authoritative_lifecycle_tool_state(&previous, &mut current, &versions, |_, _| {
                Err(CoreError::ProcessExecution(
                    "manager evidence unavailable".to_string(),
                ))
            });
        assert!(pending);
        assert_eq!(
            current
                .tools
                .iter()
                .find(|candidate| candidate.id == "codex-cli")
                .and_then(|candidate| candidate.installed_version.clone()),
            expected_version
        );
        assert!(!current
            .updates
            .iter()
            .any(|update| update.id == "update-codex-cli"));
        let mut subsequent = service.build_snapshot().expect("snapshot").0;
        let expected_for_refresh = expected_version.clone();
        let pending = merge_authoritative_lifecycle_tool_state(
            &current,
            &mut subsequent,
            &versions,
            |_, tool| {
                tool.installed_version = expected_for_refresh.clone();
                tool.state = InventoryState::ManagedCurrent;
                tool.lifecycle_confidence = "Live refreshed manager evidence".to_string();
                Ok(true)
            },
        );
        let subsequent_version = subsequent
            .tools
            .iter()
            .find(|candidate| candidate.id == "codex-cli")
            .and_then(|candidate| candidate.installed_version.clone());
        assert!(!pending);
        assert_eq!(subsequent_version, expected_version);
    }

    #[test]
    fn native_quick_setup_uses_live_missing_state_and_planned_provider_bootstrap() {
        let mut service = PhaseThreeApplicationService::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
        );
        service.live_inventory = true;
        let view = service
            .quick_setup(ProviderInventory::default())
            .expect("live Quick Setup");
        assert!(view.providers.homebrew.is_none());
        let orbstack = view
            .tools
            .iter()
            .find(|row| row.id == "orbstack")
            .expect("OrbStack row");
        assert_eq!(
            orbstack.action,
            crate::domain::setup::SetupRowAction::Install
        );
        assert!(orbstack.selected);
        assert_eq!(orbstack.mapping_id.as_deref(), Some("homebrew:orbstack"));
        let cloudflared = view
            .tools
            .iter()
            .find(|row| row.id == "cloudflared")
            .expect("cloudflared row");
        assert_eq!(
            cloudflared.action,
            crate::domain::setup::SetupRowAction::Install
        );
        assert_eq!(
            cloudflared.mapping_id.as_deref(),
            Some("homebrew:cloudflared")
        );
    }

    #[test]
    fn standalone_tool_plan_ignores_client_supplied_mapping() {
        let service = PhaseThreeApplicationService::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
        );
        let plan = service
            .prepare_lifecycle_with_providers(
                LifecyclePlanRequest {
                    resource_kind: crate::domain::lifecycle::LifecycleResourceKind::Tool,
                    action: "update".to_string(),
                    resource_id: "codex-cli".to_string(),
                    source_analysis_handle: None,
                    item_ids: None,
                    children: Vec::new(),
                    mapping_id: Some("bun:@openai/codex".to_string()),
                },
                ProviderInventory::default(),
            )
            .expect("server-owned mapping plan");
        assert_ne!(plan.mapping_id, "bun:@openai/codex");
    }

    #[test]
    fn non_tool_id_collision_cannot_trigger_provider_bootstrap() {
        let service = PhaseThreeApplicationService::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
        );
        let requirements = service
            .setup_queue_bootstrap_requirements(
                &LifecyclePlanRequest {
                    resource_kind: crate::domain::lifecycle::LifecycleResourceKind::Operation,
                    action: "setup-queue".to_string(),
                    resource_id: "portable-import".to_string(),
                    source_analysis_handle: None,
                    item_ids: None,
                    children: vec![crate::domain::lifecycle::LifecycleChildIntent {
                        resource_kind: crate::domain::lifecycle::LifecycleResourceKind::Skill,
                        resource_id: "orbstack".to_string(),
                        desired_action: "review".to_string(),
                        mapping_id: None,
                        depends_on: Vec::new(),
                    }],
                    mapping_id: None,
                },
                &ProviderInventory::default(),
            )
            .expect("bootstrap requirements");
        assert!(requirements.is_empty());
    }

    #[test]
    fn headless_scan_returns_snapshot_events_and_zero_elevation_requests() {
        let service = PhaseThreeApplicationService::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
        );

        let scan = service.headless_scan().expect("headless scan");
        assert!(scan.snapshot.tools.iter().any(|tool| tool.recommended));
        assert_eq!(
            scan.events.first().map(|event| &event.event_type),
            Some(&AppEventType::RefreshStarted)
        );
        assert!(scan
            .events
            .iter()
            .any(|event| event.event_type == AppEventType::DiagnosticsReady));
        assert!(!scan.elevation_requested);
        assert!(!scan.diagnostics.elevation.captures_password);
        assert!(!scan.diagnostics.elevation.persistent_helper);
    }

    #[test]
    fn portable_export_is_fresh_secret_free_and_additive() {
        let service = PhaseThreeApplicationService::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
        );
        let bytes = service
            .export_portable_setup("macos_arm64")
            .expect("export");
        let text = String::from_utf8(bytes.clone()).expect("utf8");
        assert!(!text.contains("providerPreference"));
        assert!(!text.contains("commandOrUrl"));
        assert!(!text.contains("/Users/"));
        let document = PortableSetupDocument::validate_bytes(&bytes).expect("portable");
        assert!(document
            .resources
            .iter()
            .filter(|resource| resource.kind == "tool")
            .all(|resource| resource.desired_action == "keep"));
        assert!(document
            .resources
            .iter()
            .any(|resource| resource.kind == "mcp"));
        assert!(document
            .resources
            .iter()
            .flat_map(|resource| &resource.credential_reference_ids)
            .all(|reference| is_valid_credential_reference(reference)));
        assert!(!text.contains(".env"));
    }

    #[test]
    fn portable_import_blocks_target_mismatch_and_marks_custom_review() {
        let service = PhaseThreeApplicationService::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
        );
        let mismatch = br#"{"schemaVersion":1,"target":"windows_x64","resources":[]}"#;
        assert!(service.import_portable_bytes(mismatch).is_err());
        let custom = br#"{"schemaVersion":1,"target":"macos_arm64","resources":[{"kind":"tool","id":"https://example.invalid/custom","desiredAction":"review"}]}"#;
        let imported = service
            .import_portable_bytes(custom)
            .expect("custom review");
        assert_eq!(
            imported.review_required_ids,
            vec!["https://example.invalid/custom"]
        );
        let windows_path = br#"{"schemaVersion":1,"target":"macos_arm64","resources":[{"kind":"tool","id":"C:\\Users\\alice\\tool","desiredAction":"review"}]}"#;
        assert!(service.import_portable_bytes(windows_path).is_err());
    }

    #[test]
    fn migration_prepare_rechecks_authoritative_eligibility() {
        let service = PhaseThreeApplicationService::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
        );
        let error = service
            .prepare_lifecycle_with_providers(
                LifecyclePlanRequest {
                    resource_kind: crate::domain::lifecycle::LifecycleResourceKind::Operation,
                    action: "migrate-with-cleanup".to_string(),
                    resource_id: "codex-cli".to_string(),
                    source_analysis_handle: None,
                    item_ids: None,
                    children: Vec::new(),
                    mapping_id: None,
                },
                ProviderInventory::default(),
            )
            .expect_err("migration must require approved Homebrew eligibility");
        assert!(matches!(error, CoreError::LifecycleConsentDenied(_)));
    }
}
