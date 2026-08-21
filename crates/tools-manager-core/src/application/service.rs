use std::path::PathBuf;

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
        inventory::{scan_inventory, InventorySnapshot, ManagerScanReport},
        mcp::{discover_mcp, McpInventorySnapshot},
        skills::{scan_skills, SkillInventorySnapshot},
        storage::{
            OperationLogEntry, ScanErrorEntry, SnapshotBundle, SqliteSnapshotStore, StorageHealth,
        },
        versioning::{build_application_updates, load_version_catalog, VersionCatalog},
    },
    domain::{
        inventory::{Freshness, LoadState, OwnershipKind, SurfaceStateContract},
        lifecycle::{
            LifecycleConsentAuthorization, LifecycleExecutionResult, LifecyclePlan,
            LifecyclePlanRequest,
        },
        mcp::McpDiscoveryReport,
        operation::{
            ConsentRecord, OperationPlan, OperationPlanStep, OperationReceipt,
            OperationResourceType,
        },
        skill::SkillScanReport,
        source::SourceKind,
    },
    error::CoreError,
    feasibility::{
        elevation::{strategy_for_current_host, ElevationStrategy},
        ui_contract::{verify_locked_ui_contract, UiContractVerification},
    },
    lifecycle::LifecycleService,
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
}

impl PhaseThreeApplicationService {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        let workspace = FixtureWorkspace::new(project_root);
        Self {
            lifecycle: LifecycleService::new(workspace.clone()),
            workspace,
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
        self.lifecycle.prepare(request)
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
        let (_, storage_health) = SqliteSnapshotStore::open(self.workspace.db_path())?;

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

    fn ensure_snapshot(&self) -> Result<(SnapshotBundle, StorageHealth, Vec<String>), CoreError> {
        let (store, health) = SqliteSnapshotStore::open(self.workspace.db_path())?;
        if let Some(mut snapshot) = store.load_snapshot()? {
            self.merge_lifecycle_receipts(&store, &mut snapshot)?;
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
            "Started fixture-backed Phase 3 inventory scan.",
        ));
        let catalog = load_tool_catalog(&self.workspace)?;
        emit_progress(app_event(
            "phase-three-catalog-validated",
            AppEventType::CatalogValidated,
            "Validated catalog schemas and semantic invariants.",
        ));
        check_refresh_cancelled(is_cancelled, "catalog-validated")?;
        let versions = load_version_catalog(&self.workspace)?;
        let inventory = scan_inventory(&self.workspace, &catalog, &versions)?;
        emit_progress(app_event(
            "phase-three-inventory-scanned",
            AppEventType::InventoryScanned,
            "Completed manager inventory and allowlisted probe reconciliation.",
        ));
        check_refresh_cancelled(is_cancelled, "inventory-scanned")?;
        let skills = scan_skills(&self.workspace, &versions)?;
        emit_progress(app_event(
            "phase-three-skills-scanned",
            AppEventType::SkillsScanned,
            "Completed bounded global skill-root scan.",
        ));
        check_refresh_cancelled(is_cancelled, "skills-scanned")?;
        let mcp = discover_mcp(&self.workspace)?;
        emit_progress(app_event(
            "phase-three-mcp-discovered",
            AppEventType::McpDiscovered,
            "Completed read-only MCP client discovery and redaction.",
        ));
        check_refresh_cancelled(is_cancelled, "mcp-discovered")?;
        let operations: Vec<OperationLogEntry> = self
            .workspace
            .read_json("tests/fixtures/catalog/operations.json")?;
        let updates = build_application_updates(&inventory.tools, &skills.skills, &versions);
        let errors = self.collect_scan_errors(&skills, &mcp);
        let mut warnings = warnings_for_scan(&inventory, &skills, &mcp, &errors);

        let mut snapshot = SnapshotBundle {
            generated_at: "2026-08-20T09:00:00+07:00".to_string(),
            catalog_version: catalog.version.clone(),
            freshness: inventory.freshness.clone(),
            tools: inventory.tools,
            skills: skills.skills,
            mcp_servers: mcp.servers,
            updates,
            operations,
            errors,
        };

        let (store, health) = SqliteSnapshotStore::open(self.workspace.db_path())?;
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
            self.merge_lifecycle_receipts(&store, &mut snapshot)
        })?;
        if store.recovered_from_corruption() {
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
        store: &SqliteSnapshotStore,
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
        assert_eq!(
            snapshot
                .tools
                .iter()
                .filter(|tool| tool.recommended)
                .count(),
            10
        );
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
        assert!(diagnostics.ui_contract.locked);
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
        let tool = previous
            .tools
            .iter_mut()
            .find(|tool| tool.id == "codex-cli")
            .expect("codex tool");
        tool.installed_version = tool.available_version.clone();
        tool.state = InventoryState::ManagedCurrent;
        tool.lifecycle_confidence = "Live manager postcondition".to_string();
        let expected_version = tool.available_version.clone();
        let versions = load_version_catalog(&service.workspace).expect("versions");

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
    fn headless_scan_returns_snapshot_events_and_zero_elevation_requests() {
        let service = PhaseThreeApplicationService::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
        );

        let scan = service.headless_scan().expect("headless scan");
        assert_eq!(
            scan.snapshot
                .tools
                .iter()
                .filter(|tool| tool.recommended)
                .count(),
            10
        );
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
}
