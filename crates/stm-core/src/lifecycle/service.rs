use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime},
};

use crate::{
    adapters::{compute_sha256, FixtureWorkspace},
    catalog::{load_tool_catalog, ToolCatalogMapping},
    domain::{
        inventory::{ExecutionMode, Freshness, InventoryState, MappingStatus, OwnershipKind},
        lifecycle::{
            LifecycleConsentAuthorization, LifecycleExecution, LifecycleExecutionResult,
            LifecycleExecutionStatus, LifecycleFollowUpAction, LifecycleItemResult,
            LifecycleItemStatus, LifecyclePlan, LifecyclePlanRequest, LifecycleResourceKind,
        },
        operation::{OperationReceipt, OperationStatus},
        recipe::{VerifiedArchiveBinary, VerifiedInstallerArtifact},
        source::{SourceAnalysisRecord, SourceAnalysisStatus, SourceKind, SourceTrust},
        tool::ToolRecord,
    },
    error::CoreError,
    feasibility::{process_supervisor::CancelSignal, source_analysis::analyze_source},
    ports::{HostExecutableResolver, ProcessLiveness, SnapshotStore},
    storage::OperationLogEntry,
};

use super::{
    evidence::ManagerEvidencePort,
    executor::LifecycleExecutionPort,
    planner::{
        prepare_codex_cleanup_retry_plan, prepare_codex_migration_inspection_plan,
        prepare_codex_migration_plan, prepare_exact_tool_plan, prepare_native_installer_plan,
        prepare_plan, prepare_setup_batch_with_bootstrap, prepare_setup_batch_with_bun_bootstrap,
        prepare_setup_batch_with_provider_bootstraps, PreparedPlan, ProviderBootstrapArtifacts,
    },
    source_probe::{analyze_source_with_probe, SourceProbe},
    source_registry::SourceAnalysisBinding,
    time::{format_timestamp, now, parse_timestamp},
};

const SOURCE_CACHE_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Clone)]
pub struct LifecycleService {
    workspace: FixtureWorkspace,
    state: Arc<Mutex<LifecycleState>>,
    executor: Arc<dyn LifecycleExecutionPort>,
    source_probe: Arc<dyn SourceProbe>,
    manager_evidence: Arc<dyn ManagerEvidencePort>,
    host: Arc<dyn HostExecutableResolver>,
    storage: Arc<dyn SnapshotStore>,
    process_liveness: Arc<dyn ProcessLiveness>,
    snapshot_merge: Arc<Mutex<()>>,
    recovery_blocker: Arc<Mutex<Option<String>>>,
}

#[derive(Default)]
struct LifecycleState {
    sequence: u64,
    plans: HashMap<String, StoredPlan>,
    operations: HashMap<String, OperationRuntime>,
    sources: HashMap<String, SourceAnalysisBinding>,
    source_cache: HashMap<String, CachedSource>,
    active_managers: HashSet<String>,
}

#[derive(Clone)]
struct StoredPlan {
    prepared: PreparedPlan,
    started: bool,
}

#[derive(Clone)]
struct OperationRuntime {
    result: LifecycleExecutionResult,
    cancel: CancelSignal,
}

#[derive(Clone)]
struct CachedSource {
    handle: String,
    expires_at: SystemTime,
}

impl LifecycleService {
    pub fn with_dependencies(
        workspace: FixtureWorkspace,
        executor: Arc<dyn LifecycleExecutionPort>,
        source_probe: Arc<dyn SourceProbe>,
        manager_evidence: Arc<dyn ManagerEvidencePort>,
        host: Arc<dyn HostExecutableResolver>,
        storage: Arc<dyn SnapshotStore>,
        process_liveness: Arc<dyn ProcessLiveness>,
    ) -> Self {
        let service = Self {
            workspace,
            state: Arc::new(Mutex::new(LifecycleState::default())),
            executor,
            source_probe,
            manager_evidence,
            host,
            storage,
            process_liveness,
            snapshot_merge: Arc::new(Mutex::new(())),
            recovery_blocker: Arc::new(Mutex::new(None)),
        };
        match service.recover_interrupted_operations() {
            Ok(false) => {}
            Ok(true) => {
                *service
                    .recovery_blocker
                    .lock()
                    .expect("lifecycle recovery blocker") = Some(
                    "Another managed lifecycle process is still active; new execution is blocked."
                        .to_string(),
                );
            }
            Err(_) => {
                *service
                    .recovery_blocker
                    .lock()
                    .expect("lifecycle recovery blocker") = Some(
                    "Interrupted lifecycle recovery could not be persisted; managed execution is blocked."
                        .to_string(),
                );
            }
        }
        service
    }

    #[cfg(test)]
    pub(crate) fn with_ports(
        workspace: FixtureWorkspace,
        executor: Arc<dyn LifecycleExecutionPort>,
        source_probe: Arc<dyn SourceProbe>,
        manager_evidence: Arc<dyn ManagerEvidencePort>,
    ) -> Self {
        Self::with_dependencies(
            workspace.clone(),
            executor,
            source_probe,
            manager_evidence,
            Arc::new(test_support::TestHost),
            test_support::TestSnapshotStore::shared(workspace.db_path()),
            Arc::new(test_support::TestProcessLiveness),
        )
    }

    #[cfg(test)]
    pub(crate) fn test_default(workspace: FixtureWorkspace) -> Self {
        Self::with_ports(
            workspace,
            Arc::new(test_support::TestExecutor),
            Arc::new(test_support::TestSourceProbe),
            Arc::new(test_support::TestManagerEvidence),
        )
    }

    #[cfg(test)]
    pub(crate) fn test_storage(&self) -> Arc<dyn SnapshotStore> {
        self.storage.clone()
    }

    pub(crate) fn refresh_tool_inventory(
        &self,
        tool_id: &str,
        current: &mut ToolRecord,
    ) -> Result<bool, CoreError> {
        let catalog = load_tool_catalog(&self.workspace)?;
        let Some(entry) = catalog.tools.iter().find(|entry| entry.id == tool_id) else {
            return Ok(false);
        };
        let platform = crate::inventory::current_platform_slug();
        let platform_mappings = entry
            .mappings
            .iter()
            .filter(|mapping| mapping.platform == platform)
            .collect::<Vec<_>>();
        if platform_mappings.is_empty() {
            return Ok(false);
        }

        let managed_mappings = platform_mappings
            .iter()
            .copied()
            .filter(|mapping| {
                mapping.mapping_status == MappingStatus::Supported
                    && mapping.execution_mode == ExecutionMode::ManagedExecute
                    && mapping.ownership_kind == OwnershipKind::ManagerOwned
            })
            .collect::<Vec<_>>();
        let mut observed = Vec::new();
        let mut evidence_error = None;
        for mapping in &managed_mappings {
            let mut executable = None;
            for action in ["update", "install", "uninstall"] {
                if let Some(candidate) = self.host.manager_evidence_executable(mapping, action)? {
                    executable = Some(candidate);
                    break;
                }
            }
            let Some(executable) = executable else {
                continue;
            };
            let evidence_path = executable.to_str().ok_or_else(|| {
                CoreError::CommandDenied(
                    "reviewed manager executable path is not UTF-8".to_string(),
                )
            })?;
            match self.manager_evidence.inspect(mapping, evidence_path) {
                Ok(evidence) => observed.push((*mapping, evidence)),
                Err(error) => evidence_error = Some(error),
            }
        }

        let selected = observed
            .iter()
            .find(|(mapping, evidence)| {
                evidence.installed
                    && mapping.manager.eq_ignore_ascii_case(&current.manager)
                    && mapping.package_id == current.package_id
            })
            .or_else(|| observed.iter().find(|(_, evidence)| evidence.installed));
        if let Some((mapping, evidence)) = selected {
            apply_live_mapping(current, mapping);
            current.installed_version = evidence.current_version.clone();
            current.available_version = Some(evidence.target_version.clone());
            current.state = if !evidence.installed {
                InventoryState::Missing
            } else if evidence.update_available {
                InventoryState::ManagedUpdateAvailable
            } else {
                InventoryState::ManagedCurrent
            };
            current.reason_code = None;
            current.lifecycle_confidence = evidence.source.clone();
            return Ok(true);
        }
        if let Some((alias, path)) = entry
            .aliases
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(entry.probe_key.as_str()))
            .find_map(|alias| {
                self.host
                    .resolve_executable(alias)
                    .map(|path| (alias, path))
            })
        {
            current.owner = "External".to_string();
            current.manager = "external".to_string();
            current.package_id = alias.to_string();
            current.mapping_status = MappingStatus::DetectOnly;
            current.ownership_kind = OwnershipKind::External;
            current.execution_mode = ExecutionMode::DetectOnly;
            current.installed_version = Some("Detected".to_string());
            current.available_version = None;
            current.state = InventoryState::ManagedCurrent;
            current.reason_code = None;
            current.lifecycle_confidence =
                format!("Live executable detected at {}", path.display());
            return Ok(true);
        }
        if let Some((mapping, evidence)) = observed.first() {
            apply_live_mapping(current, mapping);
            current.installed_version = None;
            current.available_version = Some(evidence.target_version.clone());
            current.state = InventoryState::Missing;
            current.reason_code = None;
            current.lifecycle_confidence = evidence.source.clone();
            return Ok(true);
        }
        if let Some(mapping) = platform_mappings
            .iter()
            .copied()
            .find(|mapping| mapping.execution_mode == ExecutionMode::VendorHandoff)
        {
            apply_live_mapping(current, mapping);
            current.installed_version = None;
            current.available_version = None;
            current.state = InventoryState::External;
            current.reason_code = None;
            current.lifecycle_confidence =
                "Live scan: official vendor installer is available".to_string();
            return Ok(true);
        }
        if let Some(error) = evidence_error {
            return Err(error);
        }
        if let Some(mapping) = managed_mappings.first() {
            apply_live_mapping(current, mapping);
            current.installed_version = None;
            current.available_version = None;
            current.state = InventoryState::Missing;
            current.reason_code = Some("manager.bootstrap_required".to_string());
            current.lifecycle_confidence =
                "Live scan: install provider is missing and can be bootstrapped".to_string();
            return Ok(true);
        }
        if let Some(mapping) = platform_mappings.first() {
            apply_live_mapping(current, mapping);
            current.installed_version = None;
            current.available_version = None;
            current.state = InventoryState::External;
            current.reason_code = None;
            current.lifecycle_confidence = "Live scan: external owner requires handoff".to_string();
            return Ok(true);
        }
        Ok(false)
    }

    pub(crate) fn refresh_tool_postcondition(
        &self,
        tool_id: &str,
        current: &mut ToolRecord,
    ) -> Result<bool, CoreError> {
        self.refresh_tool_inventory(tool_id, current)
    }

    fn ensure_recovery_ready(&self) -> Result<(), CoreError> {
        let blocked = self
            .recovery_blocker
            .lock()
            .expect("lifecycle recovery blocker")
            .is_some();
        if blocked {
            match self.recover_interrupted_operations() {
                Ok(false) => {
                    *self
                        .recovery_blocker
                        .lock()
                        .expect("lifecycle recovery blocker") = None;
                }
                Ok(true) => {}
                Err(_) => {}
            }
        }
        if let Some(reason) = self
            .recovery_blocker
            .lock()
            .expect("lifecycle recovery blocker")
            .clone()
        {
            Err(CoreError::LifecycleConsentDenied(reason))
        } else {
            Ok(())
        }
    }

    pub(crate) fn with_snapshot_merge<T>(
        &self,
        operation: impl FnOnce() -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        let _merge = self.snapshot_merge.lock().expect("snapshot merge lock");
        operation()
    }

    fn recover_interrupted_operations(&self) -> Result<bool, CoreError> {
        let _snapshot_merge = self
            .snapshot_merge
            .lock()
            .expect("lifecycle snapshot merge");
        let store = &self.storage;
        let interrupted = store
            .load_lifecycle_receipts()?
            .into_iter()
            .filter(|entry| {
                entry
                    .lifecycle_result
                    .as_ref()
                    .is_some_and(|result| result.status == LifecycleExecutionStatus::InProgress)
            })
            .collect::<Vec<_>>();
        let mut live_process_found = false;
        for mut operation in interrupted {
            let owner_is_live = operation
                .owner_process_id
                .is_some_and(|pid| self.process_liveness.is_alive(pid));
            let child_is_live = operation
                .child_process_id
                .is_some_and(|pid| self.process_liveness.is_alive(pid));
            if owner_is_live || child_is_live {
                live_process_found = true;
                continue;
            }
            let mut result = operation
                .lifecycle_result
                .clone()
                .expect("filtered lifecycle result");
            let request =
                operation
                    .lifecycle_request
                    .clone()
                    .unwrap_or_else(|| LifecyclePlanRequest {
                        resource_kind: LifecycleResourceKind::Operation,
                        action: "inspect-receipt".to_string(),
                        resource_id: result.operation_id.clone(),
                        source_analysis_handle: None,
                        item_ids: None,
                        children: Vec::new(),
                        mapping_id: None,
                    });
            let restart_request = restart_safe_request(&operation, request);
            let evidence_summary = match prepare_plan(
                &self.workspace,
                self.manager_evidence.as_ref(),
                self.host.as_ref(),
                restart_request.clone(),
                None,
                0,
                now(),
            ) {
                Ok(prepared) if prepared.plan.confidence.starts_with("Authoritative:") => {
                    "Fresh authoritative manager evidence was captured.".to_string()
                }
                Ok(_) => "Fresh manager evidence did not authorize execution.".to_string(),
                Err(_) => "Fresh manager evidence was unavailable.".to_string(),
            };
            let completed_at = format_timestamp(now())?;
            result.status = LifecycleExecutionStatus::Recoverable;
            result.can_cancel = false;
            result.receipt = None;
            result.redacted_detail = format!(
                "Interrupted lifecycle operation reconciled during startup. {evidence_summary} Review a new plan before retry."
            );
            result.retry_actions.clear();
            result.recovery_actions = vec![LifecycleFollowUpAction {
                id: format!("recover:{}", operation.resource),
                label: if restart_request.action == "reanalyze-source" {
                    "Re-analyze source before retry".to_string()
                } else {
                    "Refresh evidence and review recovery".to_string()
                },
                plan_request: restart_request.clone(),
            }];
            operation.receipt.status = OperationStatus::Recoverable;
            operation.receipt.completed_at = Some(completed_at.clone());
            operation.receipt.summary = result.redacted_detail.clone();
            operation.receipt.details = vec![
                format!("Plan digest: {}", result.plan_digest),
                evidence_summary,
            ];
            operation.lifecycle_request = Some(restart_request);
            operation.lifecycle_result = Some(result.clone());
            operation.owner_process_id = None;
            operation.child_process_id = None;
            store.reconcile_lifecycle_receipt(&operation, &result, &completed_at)?;
        }
        Ok(live_process_found)
    }

    pub fn analyze_source(
        &self,
        kind: SourceKind,
        submitted_url: &str,
    ) -> Result<(SourceAnalysisRecord, LifecyclePlanRequest), CoreError> {
        let preliminary = analyze_source(kind.clone(), submitted_url)?;
        let cache_key = format!(
            "{:?}|{}",
            kind,
            preliminary
                .normalized_url
                .as_deref()
                .unwrap_or(&preliminary.submitted_url)
        );
        if let Some(cached) = self.cached_source(&cache_key, now()) {
            let state = self.state.lock().expect("lifecycle state");
            if let Some(binding) = state.sources.get(&cached.handle) {
                return Ok((
                    binding.record.clone(),
                    request_for_source(&kind, &cached.handle, binding),
                ));
            }
        }

        let initial_resource_id = self.resolve_catalog_source(&kind, &preliminary)?;
        let (mut record, resource_id) = if let Some(expected_resource_id) = initial_resource_id {
            let mut probed =
                analyze_source_with_probe(kind.clone(), submitted_url, self.source_probe.as_ref())?;
            let resolved_after_probe = self.resolve_catalog_source(&kind, &probed)?;
            if resolved_after_probe.as_deref() != Some(expected_resource_id.as_str()) {
                probed.status = SourceAnalysisStatus::Blocked;
                probed.trust = SourceTrust::Blocked;
                probed.risk_flags.push(
                    "Resolved source no longer matches the reviewed catalog identity.".to_string(),
                );
                probed.notes.push(
                    "Managed planning remains blocked; review the canonical catalog source again."
                        .to_string(),
                );
                (probed, None)
            } else {
                (probed, Some(expected_resource_id))
            }
        } else {
            let mut inspect_only = preliminary;
            inspect_only.notes.push(
                "No allowlisted source resolver matched; network probing was skipped.".to_string(),
            );
            (inspect_only, None)
        };
        if let Some(resource_id) = resource_id.as_deref() {
            record.trust = SourceTrust::CatalogMatch;
            record.publisher = "Locked tool catalog identity matched".to_string();
            if let Some(entry) = load_tool_catalog(&self.workspace)?.get(resource_id) {
                record.detected_name = entry.name.clone();
                record.target = format!(
                    "Canonical tool {} and its current-platform owner mapping",
                    entry.id
                );
            }
        }
        if record.status != SourceAnalysisStatus::ReviewReady {
            record.trust = SourceTrust::Blocked;
        }

        let issued_at = now();
        let mut state = self.state.lock().expect("lifecycle state");
        state.sequence = state.sequence.saturating_add(1);
        let handle_material =
            serde_json::to_vec(&(state.sequence, &cache_key, format_timestamp(issued_at)?))?;
        let digest = compute_sha256([handle_material]);
        let handle = format!("source-analysis-{}", &digest[7..23]);
        let binding = SourceAnalysisBinding {
            record: record.clone(),
            resource_id,
            expires_at: issued_at + SOURCE_CACHE_TTL,
        };
        state.sources.insert(handle.clone(), binding.clone());
        state.source_cache.insert(
            cache_key,
            CachedSource {
                handle: handle.clone(),
                expires_at: issued_at + SOURCE_CACHE_TTL,
            },
        );
        Ok((record, request_for_source(&kind, &handle, &binding)))
    }

    pub fn prepare(&self, request: LifecyclePlanRequest) -> Result<LifecyclePlan, CoreError> {
        self.ensure_recovery_ready()?;
        let source = self.source_binding(&request)?;
        let sequence = {
            let mut state = self.state.lock().expect("lifecycle state");
            state.sequence = state.sequence.saturating_add(1);
            state.sequence
        };
        let prepared = prepare_plan(
            &self.workspace,
            self.manager_evidence.as_ref(),
            self.host.as_ref(),
            request,
            source.as_ref(),
            sequence,
            now(),
        )?;
        let plan = prepared.plan.clone();
        self.state.lock().expect("lifecycle state").plans.insert(
            plan.plan_id.clone(),
            StoredPlan {
                prepared,
                started: false,
            },
        );
        Ok(plan)
    }

    pub fn prepare_native_installer(
        &self,
        request: LifecyclePlanRequest,
        artifact: &VerifiedInstallerArtifact,
    ) -> Result<LifecyclePlan, CoreError> {
        self.ensure_recovery_ready()?;
        let sequence = {
            let mut state = self.state.lock().expect("lifecycle state");
            state.sequence = state.sequence.saturating_add(1);
            state.sequence
        };
        let prepared =
            prepare_native_installer_plan(self.host.as_ref(), request, artifact, sequence, now())?;
        let plan = prepared.plan.clone();
        self.state.lock().expect("lifecycle state").plans.insert(
            plan.plan_id.clone(),
            StoredPlan {
                prepared,
                started: false,
            },
        );
        Ok(plan)
    }

    pub fn prepare_setup_with_bootstrap(
        &self,
        request: LifecyclePlanRequest,
        artifact: &VerifiedInstallerArtifact,
    ) -> Result<LifecyclePlan, CoreError> {
        self.ensure_recovery_ready()?;
        let sequence = {
            let mut state = self.state.lock().expect("lifecycle state");
            state.sequence = state.sequence.saturating_add(3);
            state.sequence
        };
        let prepared = prepare_setup_batch_with_bootstrap(
            &self.workspace,
            self.manager_evidence.as_ref(),
            self.host.as_ref(),
            request,
            artifact,
            sequence,
            now(),
        )?;
        let plan = prepared.plan.clone();
        self.state.lock().expect("lifecycle state").plans.insert(
            plan.plan_id.clone(),
            StoredPlan {
                prepared,
                started: false,
            },
        );
        Ok(plan)
    }

    pub fn prepare_setup_with_bun_bootstrap(
        &self,
        request: LifecyclePlanRequest,
        artifact: &crate::domain::recipe::VerifiedArchiveBinary,
    ) -> Result<LifecyclePlan, CoreError> {
        self.ensure_recovery_ready()?;
        let sequence = {
            let mut state = self.state.lock().expect("lifecycle state");
            state.sequence = state.sequence.saturating_add(3);
            state.sequence
        };
        let prepared = prepare_setup_batch_with_bun_bootstrap(
            &self.workspace,
            self.manager_evidence.as_ref(),
            self.host.as_ref(),
            request,
            artifact,
            sequence,
            now(),
        )?;
        let plan = prepared.plan.clone();
        self.state.lock().expect("lifecycle state").plans.insert(
            plan.plan_id.clone(),
            StoredPlan {
                prepared,
                started: false,
            },
        );
        Ok(plan)
    }

    pub fn prepare_setup_with_provider_bootstraps(
        &self,
        request: LifecyclePlanRequest,
        homebrew_artifact: Option<&VerifiedInstallerArtifact>,
        bun_artifact: Option<&VerifiedArchiveBinary>,
    ) -> Result<LifecyclePlan, CoreError> {
        self.ensure_recovery_ready()?;
        let sequence = {
            let mut state = self.state.lock().expect("lifecycle state");
            state.sequence = state.sequence.saturating_add(4);
            state.sequence
        };
        let prepared = prepare_setup_batch_with_provider_bootstraps(
            &self.workspace,
            self.manager_evidence.as_ref(),
            self.host.as_ref(),
            request,
            ProviderBootstrapArtifacts {
                homebrew: homebrew_artifact,
                bun: bun_artifact,
            },
            sequence,
            now(),
        )?;
        let plan = prepared.plan.clone();
        self.state.lock().expect("lifecycle state").plans.insert(
            plan.plan_id.clone(),
            StoredPlan {
                prepared,
                started: false,
            },
        );
        Ok(plan)
    }
    pub fn prepare_codex_migration(
        &self,
        request: LifecyclePlanRequest,
        cleanup_old_owner: bool,
    ) -> Result<LifecyclePlan, CoreError> {
        self.ensure_recovery_ready()?;
        let sequence = {
            let mut state = self.state.lock().expect("lifecycle state");
            state.sequence = state.sequence.saturating_add(3);
            state.sequence
        };
        let prepared = prepare_codex_migration_plan(
            &self.workspace,
            self.manager_evidence.as_ref(),
            self.host.as_ref(),
            request,
            cleanup_old_owner,
            sequence,
            now(),
        )?;
        let plan = prepared.plan.clone();
        self.state.lock().expect("lifecycle state").plans.insert(
            plan.plan_id.clone(),
            StoredPlan {
                prepared,
                started: false,
            },
        );
        Ok(plan)
    }

    pub fn prepare_codex_migration_inspection(
        &self,
        request: LifecyclePlanRequest,
    ) -> Result<LifecyclePlan, CoreError> {
        self.ensure_recovery_ready()?;
        let sequence = {
            let mut state = self.state.lock().expect("lifecycle state");
            state.sequence = state.sequence.saturating_add(1);
            state.sequence
        };
        let prepared =
            prepare_codex_migration_inspection_plan(&self.workspace, request, sequence, now())?;
        let plan = prepared.plan.clone();
        self.state.lock().expect("lifecycle state").plans.insert(
            plan.plan_id.clone(),
            StoredPlan {
                prepared,
                started: false,
            },
        );
        Ok(plan)
    }

    pub fn prepare_codex_cleanup_retry(
        &self,
        request: LifecyclePlanRequest,
    ) -> Result<LifecyclePlan, CoreError> {
        self.ensure_recovery_ready()?;
        let sequence = {
            let mut state = self.state.lock().expect("lifecycle state");
            state.sequence = state.sequence.saturating_add(2);
            state.sequence
        };
        let prepared = prepare_codex_cleanup_retry_plan(
            &self.workspace,
            self.manager_evidence.as_ref(),
            self.host.as_ref(),
            request,
            sequence,
            now(),
        )?;
        let plan = prepared.plan.clone();
        self.state.lock().expect("lifecycle state").plans.insert(
            plan.plan_id.clone(),
            StoredPlan {
                prepared,
                started: false,
            },
        );
        Ok(plan)
    }

    pub fn native_confirmation_summary(
        &self,
        plan_id: &str,
        locale: &str,
    ) -> Result<String, CoreError> {
        let state = self.state.lock().expect("lifecycle state");
        let stored = state
            .plans
            .get(plan_id)
            .ok_or_else(|| CoreError::LifecyclePlanNotFound(plan_id.to_string()))?;
        let plan = &stored.prepared.plan;
        let item_count = match &plan.execution {
            LifecycleExecution::Batch { items } => {
                let requested = items
                    .iter()
                    .filter(|item| !item.canonical_id.starts_with("provider:"))
                    .count();
                requested.max(1)
            }
            _ => 1,
        };
        if locale.starts_with("en") {
            Ok(format!(
                "STM will install or update {item_count} selected item(s).\n\nYour system may ask for confirmation. Continue?"
            ))
        } else {
            Ok(format!(
                "STM sẽ cài đặt hoặc cập nhật {item_count} công cụ đã chọn.\n\nHệ thống có thể yêu cầu xác nhận. Tiếp tục?"
            ))
        }
    }

    pub fn start(
        &self,
        plan_id: &str,
        authorization: LifecycleConsentAuthorization,
    ) -> Result<LifecycleExecutionResult, CoreError> {
        self.ensure_recovery_ready()?;
        let stored = {
            let state = self.state.lock().expect("lifecycle state");
            state
                .plans
                .get(plan_id)
                .cloned()
                .ok_or_else(|| CoreError::LifecyclePlanNotFound(plan_id.to_string()))?
        };
        if stored.started {
            return Err(CoreError::LifecycleConsentDenied(
                "lifecycle plans are single-use".to_string(),
            ));
        }
        validate_authorization(&stored.prepared.plan, &authorization, now())?;

        let (operation_id, initial, cancel) = {
            let mut state = self.state.lock().expect("lifecycle state");
            let managers = manager_keys(&stored.prepared.plan);
            if managers
                .iter()
                .any(|manager| state.active_managers.contains(manager))
            {
                return Err(CoreError::LifecycleConsentDenied(
                    "the authoritative package manager is already executing another lifecycle operation"
                        .to_string(),
                ));
            }
            {
                let stored_plan = state
                    .plans
                    .get_mut(plan_id)
                    .ok_or_else(|| CoreError::LifecyclePlanNotFound(plan_id.to_string()))?;
                if stored_plan.started {
                    return Err(CoreError::LifecycleConsentDenied(
                        "lifecycle plans are single-use".to_string(),
                    ));
                }
                stored_plan.started = true;
            }
            state.active_managers.extend(managers);
            let operation_id = opaque_operation_id(&stored.prepared.plan, state.sequence);
            let initial = initial_result(&stored.prepared.plan, &operation_id);
            let cancel = CancelSignal::default();
            state.operations.insert(
                operation_id.clone(),
                OperationRuntime {
                    result: initial.clone(),
                    cancel: cancel.clone(),
                },
            );
            (operation_id, initial, cancel)
        };

        let operation = operation_log_entry(&stored.prepared.plan, &initial, &authorization, None);
        let journal = {
            let _snapshot_merge = self
                .snapshot_merge
                .lock()
                .expect("lifecycle snapshot merge");
            self.storage.persist_lifecycle_receipt(
                &operation,
                &initial,
                &authorization,
                &authorization.granted_at,
            )
        };
        if let Err(error) = journal {
            let mut state = self.state.lock().expect("lifecycle state");
            state.operations.remove(&operation_id);
            for manager in manager_keys(&stored.prepared.plan) {
                state.active_managers.remove(&manager);
            }
            if let Some(stored_plan) = state.plans.get_mut(plan_id) {
                stored_plan.started = false;
            }
            return Err(error);
        }

        let service = self.clone();
        let prepared = stored.prepared;
        thread::spawn(move || {
            let result = service.execute(&prepared, &operation_id, &cancel);
            service.finish(&prepared.plan, result, authorization);
        });
        Ok(initial)
    }

    pub fn status(&self, operation_id: &str) -> Result<LifecycleExecutionResult, CoreError> {
        self.state
            .lock()
            .expect("lifecycle state")
            .operations
            .get(operation_id)
            .map(|runtime| runtime.result.clone())
            .ok_or_else(|| CoreError::LifecycleOperationNotFound(operation_id.to_string()))
    }

    pub fn cancel(&self, operation_id: &str) -> Result<LifecycleExecutionResult, CoreError> {
        let mut state = self.state.lock().expect("lifecycle state");
        let runtime = state
            .operations
            .get_mut(operation_id)
            .ok_or_else(|| CoreError::LifecycleOperationNotFound(operation_id.to_string()))?;
        if runtime.result.status == LifecycleExecutionStatus::InProgress {
            runtime.cancel.cancel();
            runtime.result.can_cancel = false;
            runtime.result.redacted_detail =
                "Cancellation requested; waiting for the active process tree to stop.".to_string();
        }
        Ok(runtime.result.clone())
    }

    fn cached_source(&self, key: &str, current: SystemTime) -> Option<CachedSource> {
        self.state
            .lock()
            .expect("lifecycle state")
            .source_cache
            .get(key)
            .filter(|cached| cached.expires_at > current)
            .cloned()
    }

    fn source_binding(
        &self,
        request: &LifecyclePlanRequest,
    ) -> Result<Option<SourceAnalysisBinding>, CoreError> {
        let Some(handle) = request.source_analysis_handle.as_deref() else {
            return Ok(None);
        };
        let checked_at = now();
        let binding = self
            .state
            .lock()
            .expect("lifecycle state")
            .sources
            .get(handle)
            .cloned()
            .ok_or_else(|| {
                CoreError::LifecycleEvidenceChanged(
                    "source analysis handle is unknown or expired".to_string(),
                )
            })?;
        if binding.expires_at <= checked_at {
            let mut state = self.state.lock().expect("lifecycle state");
            state.sources.remove(handle);
            state
                .source_cache
                .retain(|_, cached| cached.handle != handle);
            return Err(CoreError::LifecycleEvidenceChanged(
                "source analysis handle is unknown or expired".to_string(),
            ));
        }
        let Some(expected_resource_id) = binding.resource_id.as_deref() else {
            return Ok(Some(binding));
        };
        let submitted_url = binding.record.normalized_url.as_deref().ok_or_else(|| {
            CoreError::LifecycleEvidenceChanged(
                "reviewed source identity is unavailable".to_string(),
            )
        })?;
        let mut refreshed = analyze_source_with_probe(
            binding.record.kind.clone(),
            submitted_url,
            self.source_probe.as_ref(),
        )?;
        let resolved_resource_id = self.resolve_catalog_source(&binding.record.kind, &refreshed)?;
        if resolved_resource_id.as_deref() != Some(expected_resource_id) {
            return Err(CoreError::LifecycleEvidenceChanged(
                "reviewed source evidence changed; analyze the source again".to_string(),
            ));
        }
        refreshed.trust = SourceTrust::CatalogMatch;
        refreshed.publisher = "Locked tool catalog identity matched".to_string();
        let refreshed_expires_at = checked_at + SOURCE_CACHE_TTL;
        let refreshed_binding = SourceAnalysisBinding {
            record: refreshed,
            resource_id: binding.resource_id,
            expires_at: refreshed_expires_at,
        };
        let mut state = self.state.lock().expect("lifecycle state");
        state
            .sources
            .insert(handle.to_string(), refreshed_binding.clone());
        if let Some(cached) = state
            .source_cache
            .values_mut()
            .find(|cached| cached.handle == handle)
        {
            cached.expires_at = refreshed_expires_at;
        }
        Ok(Some(refreshed_binding))
    }

    fn resolve_catalog_source(
        &self,
        kind: &SourceKind,
        record: &SourceAnalysisRecord,
    ) -> Result<Option<String>, CoreError> {
        if *kind != SourceKind::Tool || record.status != SourceAnalysisStatus::ReviewReady {
            return Ok(None);
        }
        let Some(normalized) = record.normalized_url.as_deref() else {
            return Ok(None);
        };
        let identity = source_identity(normalized)?;
        let catalog = load_tool_catalog(&self.workspace)?;
        Ok(catalog
            .tools
            .iter()
            .find(|entry| {
                source_identity(&entry.source_url).ok().as_deref() == Some(identity.as_str())
            })
            .map(|entry| entry.id.clone()))
    }

    fn revalidate(&self, original: &PreparedPlan) -> Result<(), CoreError> {
        if matches!(
            original.plan.execution,
            LifecycleExecution::NativeInstaller { .. }
                | LifecycleExecution::ArchiveInstaller { .. }
        ) {
            for expected in &original.executable_identities {
                let current = self.host.executable_identity(expected.path.clone())?;
                if &current != expected {
                    return Err(CoreError::LifecycleEvidenceChanged(
                        "native installer artifact or opener identity changed".to_string(),
                    ));
                }
            }
            return Ok(());
        }
        for precondition in &original.preconditions {
            self.revalidate(precondition)?;
        }
        let source = self.source_binding(&original.plan.request)?;
        let current = if original.exact_mapping {
            prepare_exact_tool_plan(
                &self.workspace,
                self.manager_evidence.as_ref(),
                self.host.as_ref(),
                original.plan.request.clone(),
                0,
                now(),
            )?
        } else {
            prepare_plan(
                &self.workspace,
                self.manager_evidence.as_ref(),
                self.host.as_ref(),
                original.plan.request.clone(),
                source.as_ref(),
                0,
                now(),
            )?
        };
        if current.evidence_fingerprint != original.evidence_fingerprint
            || current.executable_identities != original.executable_identities
        {
            return Err(CoreError::LifecycleEvidenceChanged(
                "canonical mapping, versions, ownership, privilege, or executable identity changed; review a fresh plan".to_string(),
            ));
        }
        Ok(())
    }

    fn execute(
        &self,
        prepared: &PreparedPlan,
        operation_id: &str,
        cancel: &CancelSignal,
    ) -> LifecycleExecutionResult {
        if prepared.children.is_empty() {
            if let Err(error) = self.revalidate(prepared) {
                return failed_result(
                    &prepared.plan,
                    operation_id,
                    format!("Revalidation failed before execution: {error}"),
                );
            }
            let outcome = if cancel.is_cancelled() {
                (cancelled_item(&prepared.plan), false)
            } else {
                self.execute_plan(
                    &prepared.plan,
                    operation_id,
                    cancel,
                    &prepared.executable_identities,
                    false,
                )
            };
            return aggregate_result(&prepared.plan, operation_id, vec![outcome]);
        }

        let mut outcomes = Vec::with_capacity(prepared.children.len());
        let mut completed = HashMap::<String, LifecycleItemStatus>::new();
        let parent_expiry = parse_timestamp(&prepared.plan.expires_at).ok();
        for (child_index, child) in prepared.children.iter().enumerate() {
            if cancel.is_cancelled() {
                let outcome = (cancelled_item(&child.plan), false);
                completed.insert(child.dependency_key.clone(), outcome.0.status.clone());
                outcomes.push(outcome);
                continue;
            }
            let dependencies_satisfied = child.depends_on.iter().all(|dependency| {
                matches!(
                    completed.get(dependency),
                    Some(LifecycleItemStatus::Success)
                )
            });
            if !dependencies_satisfied {
                let outcome = (
                    LifecycleItemResult {
                        id: child.plan.canonical_id.clone(),
                        label: format!("{} {}", child.plan.request.action, child.plan.resource_id),
                        status: LifecycleItemStatus::Skipped,
                        receipt: None,
                        redacted_detail: "Required dependency did not complete successfully."
                            .to_string(),
                    },
                    false,
                );
                completed.insert(child.dependency_key.clone(), outcome.0.status.clone());
                outcomes.push(outcome);
                continue;
            }
            if parent_expiry.is_some_and(|expiry| now() > expiry) {
                let outcome = (
                    LifecycleItemResult {
                        id: child.plan.canonical_id.clone(),
                        label: format!("{} {}", child.plan.request.action, child.plan.resource_id),
                        status: LifecycleItemStatus::Failed,
                        receipt: None,
                        redacted_detail:
                            "Plan expired before this mutating child; review a fresh plan."
                                .to_string(),
                    },
                    false,
                );
                completed.insert(child.dependency_key.clone(), outcome.0.status.clone());
                outcomes.push(outcome);
                continue;
            }
            if !child.precondition_executable_paths.is_empty() {
                let expected_version =
                    child
                        .precondition_expected_version
                        .as_deref()
                        .ok_or_else(|| {
                            CoreError::MalformedInput(
                                "migration cleanup precondition version is missing".to_string(),
                            )
                        });
                let precondition = expected_version.and_then(|version| {
                    self.executor
                        .verify_migration_target(&child.precondition_executable_paths, version)
                });
                if let Err(error) = precondition {
                    let outcome = (
                        LifecycleItemResult {
                            id: child.plan.canonical_id.clone(),
                            label: format!(
                                "{} {}",
                                child.plan.request.action, child.plan.resource_id
                            ),
                            status: LifecycleItemStatus::Failed,
                            receipt: None,
                            redacted_detail: format!(
                                "Migration cleanup precondition failed: {error}"
                            ),
                        },
                        false,
                    );
                    completed.insert(child.dependency_key.clone(), outcome.0.status.clone());
                    outcomes.push(outcome);
                    if let Err(checkpoint_error) =
                        self.checkpoint_batch_result(&prepared.plan, operation_id, &outcomes)
                    {
                        if let Some((item, recovery)) = outcomes.last_mut() {
                            *recovery = true;
                            item.redacted_detail = format!(
                                "{} Child checkpoint persistence failed: {checkpoint_error}",
                                item.redacted_detail
                            );
                        }
                    }
                    continue;
                }
            }
            let mut outcome = if child.staged {
                match prepare_plan(&self.workspace, self.manager_evidence.as_ref(), self.host.as_ref(), child.plan.request.clone(),
                None,
                0,
                now(),) {
                    Ok(fresh)
                        if fresh.recipe_fingerprint == child.recipe_fingerprint
                            && matches!(
                                fresh.plan.execution,
                                LifecycleExecution::ManagedExecute { .. }
                            ) =>
                    {
                        self.execute_plan(
                            &fresh.plan,
                            operation_id,
                            cancel,
                            &fresh.executable_identities,
                            false,
                        )
                    }
                    Ok(_) => (
                        LifecycleItemResult {
                            id: child.plan.canonical_id.clone(),
                            label: format!(
                                "{} {}",
                                child.plan.request.action, child.plan.resource_id
                            ),
                            status: LifecycleItemStatus::Failed,
                            receipt: None,
                            redacted_detail:
                                "Provider bootstrap completed, but the dependent recipe no longer matches the reviewed mapping."
                                    .to_string(),
                        },
                        false,
                    ),
                    Err(error) => (
                        LifecycleItemResult {
                            id: child.plan.canonical_id.clone(),
                            label: format!(
                                "{} {}",
                                child.plan.request.action, child.plan.resource_id
                            ),
                            status: LifecycleItemStatus::Failed,
                            receipt: None,
                            redacted_detail: format!(
                                "Failed to compile dependent recipe after bootstrap: {error}"
                            ),
                        },
                        false,
                    ),
                }
            } else {
                match self.revalidate(child) {
                    Ok(()) => self.execute_plan(
                        &child.plan,
                        operation_id,
                        cancel,
                        &child.executable_identities,
                        child.dependency_key.starts_with("migration-"),
                    ),
                    Err(error) => (
                        LifecycleItemResult {
                            id: child.plan.canonical_id.clone(),
                            label: format!(
                                "{} {}",
                                child.plan.request.action, child.plan.resource_id
                            ),
                            status: LifecycleItemStatus::Failed,
                            receipt: None,
                            redacted_detail: format!(
                                "Revalidation failed before execution: {error}"
                            ),
                        },
                        false,
                    ),
                }
            };
            if outcome.0.status == LifecycleItemStatus::Success
                && !child.postcondition_executable_paths.is_empty()
            {
                if let Err(error) = self.executor.verify_migration_target(
                    &child.postcondition_executable_paths,
                    &child.plan.target_version,
                ) {
                    outcome = (
                        LifecycleItemResult {
                            id: child.plan.canonical_id.clone(),
                            label: format!(
                                "{} {}",
                                child.plan.request.action, child.plan.resource_id
                            ),
                            status: LifecycleItemStatus::Failed,
                            receipt: None,
                            redacted_detail: format!(
                                "Migration target activation verification failed: {error}"
                            ),
                        },
                        false,
                    );
                }
            }
            if outcome.0.status == LifecycleItemStatus::Success
                && child.dependency_key.starts_with("migration-target:")
            {
                if let Err(error) = self.merge_codex_migration_target_state(&child.plan) {
                    outcome = (
                        LifecycleItemResult {
                            id: child.plan.canonical_id.clone(),
                            label: format!(
                                "{} {}",
                                child.plan.request.action, child.plan.resource_id
                            ),
                            status: LifecycleItemStatus::Failed,
                            receipt: None,
                            redacted_detail: format!(
                                "Migration target state persistence failed: {error}"
                            ),
                        },
                        false,
                    );
                }
            }
            completed.insert(child.dependency_key.clone(), outcome.0.status.clone());
            outcomes.push(outcome);
            if let Err(error) =
                self.checkpoint_batch_result(&prepared.plan, operation_id, &outcomes)
            {
                if let Some((item, recovery)) = outcomes.last_mut() {
                    *recovery = true;
                    item.redacted_detail = format!(
                        "{} Child checkpoint persistence failed: {error}",
                        item.redacted_detail
                    );
                }
                for remaining in prepared.children.iter().skip(child_index + 1) {
                    outcomes.push((
                        LifecycleItemResult {
                            id: remaining.plan.canonical_id.clone(),
                            label: format!(
                                "{} {}",
                                remaining.plan.request.action, remaining.plan.resource_id
                            ),
                            status: LifecycleItemStatus::Skipped,
                            receipt: None,
                            redacted_detail:
                                "Skipped because the prior child checkpoint could not be persisted."
                                    .to_string(),
                        },
                        false,
                    ));
                }
                break;
            }
        }
        aggregate_result(&prepared.plan, operation_id, outcomes)
    }

    fn checkpoint_batch_result(
        &self,
        plan: &LifecyclePlan,
        operation_id: &str,
        outcomes: &[(LifecycleItemResult, bool)],
    ) -> Result<(), CoreError> {
        let (total_steps, can_cancel) = match &plan.execution {
            LifecycleExecution::Batch { items } => (
                items.len(),
                execution_sequence_can_cancel(&items[outcomes.len().min(items.len())..]),
            ),
            _ => (outcomes.len(), false),
        };
        let result = LifecycleExecutionResult {
            operation_id: operation_id.to_string(),
            plan_digest: plan.digest.clone(),
            status: LifecycleExecutionStatus::InProgress,
            completed_steps: outcomes.len(),
            total_steps,
            can_cancel,
            receipt: None,
            redacted_detail: format!(
                "Persisted {} of {total_steps} lifecycle child checkpoints.",
                outcomes.len()
            ),
            items: outcomes.iter().map(|(item, _)| item.clone()).collect(),
            retry_actions: Vec::new(),
            recovery_actions: Vec::new(),
        };
        self.storage
            .checkpoint_lifecycle_result(&result, &format_timestamp(now())?)
    }

    fn execute_plan(
        &self,
        plan: &LifecyclePlan,
        operation_id: &str,
        cancel: &CancelSignal,
        identities: &[super::command::ExecutableIdentity],
        defer_snapshot_merge: bool,
    ) -> (LifecycleItemResult, bool) {
        let label = format!("{} {}", plan.request.action, plan.resource_id);
        let mut recovery_required = false;
        let item = match &plan.execution {
            LifecycleExecution::ManagedExecute { executable, argv }
            | LifecycleExecution::SignedProductUpdate { executable, argv }
            | LifecycleExecution::NativeInstaller {
                executable, argv, ..
            }
            | LifecycleExecution::ArchiveInstaller {
                executable, argv, ..
            } => {
                let on_spawn = |process_id| {
                    self.storage
                        .persist_lifecycle_child_process(operation_id, process_id)
                };
                let execution =
                    if matches!(plan.execution, LifecycleExecution::ArchiveInstaller { .. }) {
                        self.executor
                            .install_archive_binary(&argv[0], &argv[1], identities, cancel)
                    } else if matches!(plan.execution, LifecycleExecution::NativeInstaller { .. }) {
                        self.executor
                            .execute_native_installer(executable, argv, identities, &on_spawn)
                    } else {
                        self.executor
                            .execute_managed(executable, argv, identities, &on_spawn, cancel)
                    };
                match execution {
                    Ok(outcome) if outcome.cancelled => LifecycleItemResult {
                        id: plan.canonical_id.clone(),
                        label,
                        status: LifecycleItemStatus::Cancelled,
                        receipt: None,
                        redacted_detail: outcome.redacted_detail,
                    },
                    Ok(outcome)
                        if outcome.success
                            && matches!(
                                plan.execution,
                                LifecycleExecution::NativeInstaller { .. }
                            ) =>
                    {
                        let (package_id, expected_version, previous_install_time) =
                            match &plan.execution {
                                LifecycleExecution::NativeInstaller {
                                    package_id,
                                    expected_version,
                                    previous_receipt_install_time,
                                    ..
                                } => (package_id, expected_version, *previous_receipt_install_time),
                                _ => unreachable!(),
                            };
                        match self.executor.verify_homebrew_bootstrap(
                            package_id,
                            expected_version,
                            previous_install_time,
                        ) {
                            Ok(()) => LifecycleItemResult {
                                id: plan.canonical_id.clone(),
                                label,
                                status: LifecycleItemStatus::Success,
                                receipt: Some(receipt_id(plan)),
                                redacted_detail: outcome.redacted_detail,
                            },
                            Err(error) => LifecycleItemResult {
                                id: plan.canonical_id.clone(),
                                label,
                                status: LifecycleItemStatus::Failed,
                                receipt: None,
                                redacted_detail: format!(
                                    "macOS Installer closed without a verified Homebrew receipt and executable: {error}"
                                ),
                            },
                        }
                    }
                    Ok(outcome)
                        if outcome.success
                            && matches!(
                                plan.execution,
                                LifecycleExecution::ArchiveInstaller { .. }
                            ) =>
                    {
                        let (target_path, binary_sha256, expected_version) = match &plan.execution {
                            LifecycleExecution::ArchiveInstaller {
                                target_path,
                                binary_sha256,
                                expected_version,
                                ..
                            } => (target_path, binary_sha256, expected_version),
                            _ => unreachable!(),
                        };
                        match self.executor.verify_bun_bootstrap(
                            target_path,
                            binary_sha256,
                            expected_version,
                        ) {
                            Ok(()) => LifecycleItemResult {
                                id: plan.canonical_id.clone(),
                                label,
                                status: LifecycleItemStatus::Success,
                                receipt: Some(receipt_id(plan)),
                                redacted_detail: outcome.redacted_detail,
                            },
                            Err(error) => LifecycleItemResult {
                                id: plan.canonical_id.clone(),
                                label,
                                status: LifecycleItemStatus::Failed,
                                receipt: None,
                                redacted_detail: format!(
                                    "Bun provider postcondition failed: {error}"
                                ),
                            },
                        }
                    }
                    Ok(outcome) if outcome.success => {
                        match self.verify_and_merge_postcondition(
                            plan,
                            executable,
                            !defer_snapshot_merge,
                        ) {
                            Ok(()) => LifecycleItemResult {
                                id: plan.canonical_id.clone(),
                                label,
                                status: LifecycleItemStatus::Success,
                                receipt: Some(receipt_id(plan)),
                                redacted_detail: outcome.redacted_detail,
                            },
                            Err(error) if defer_snapshot_merge => LifecycleItemResult {
                                id: plan.canonical_id.clone(),
                                label,
                                status: LifecycleItemStatus::Failed,
                                receipt: None,
                                redacted_detail: format!(
                                    "Migration manager postcondition failed: {error}"
                                ),
                            },
                            Err(error) => {
                                recovery_required = true;
                                LifecycleItemResult {
                                    id: plan.canonical_id.clone(),
                                    label,
                                    status: LifecycleItemStatus::Failed,
                                    receipt: None,
                                    redacted_detail: format!(
                                        "Manager command completed, but live post-operation verification requires recovery: {error}"
                                    ),
                                }
                            }
                        }
                    }
                    Ok(outcome) => LifecycleItemResult {
                        id: plan.canonical_id.clone(),
                        label,
                        status: LifecycleItemStatus::Failed,
                        receipt: None,
                        redacted_detail: outcome.redacted_detail,
                    },
                    Err(error) => LifecycleItemResult {
                        id: plan.canonical_id.clone(),
                        label,
                        status: LifecycleItemStatus::Failed,
                        receipt: None,
                        redacted_detail: format!(
                            "Managed execution failed at the reviewed boundary: {error}"
                        ),
                    },
                }
            }
            LifecycleExecution::VendorHandoff { handoff_target } => {
                match self.executor.open_vendor_handoff(handoff_target) {
                    Ok(()) => LifecycleItemResult {
                        id: plan.canonical_id.clone(), label, status: LifecycleItemStatus::Success,
                        receipt: Some(receipt_id(plan)), redacted_detail: "Opened the reviewed vendor handoff; vendor execution and recovery remain outside STM.".to_string(),
                    },
                    Err(error) => LifecycleItemResult {
                        id: plan.canonical_id.clone(), label, status: LifecycleItemStatus::Failed,
                        receipt: None, redacted_detail: format!("Reviewed vendor handoff failed: {error}"),
                    },
                }
            }
            LifecycleExecution::DetectOnly { guidance } => LifecycleItemResult {
                id: plan.canonical_id.clone(),
                label,
                status: LifecycleItemStatus::Skipped,
                receipt: None,
                redacted_detail: guidance.clone(),
            },
            LifecycleExecution::Batch { .. } => LifecycleItemResult {
                id: plan.canonical_id.clone(),
                label,
                status: LifecycleItemStatus::Failed,
                receipt: None,
                redacted_detail: "Nested batch plans are not authorized.".to_string(),
            },
        };
        (item, recovery_required)
    }

    fn verify_and_merge_postcondition(
        &self,
        plan: &LifecyclePlan,
        executable: &str,
        persist_snapshot: bool,
    ) -> Result<(), CoreError> {
        let catalog = load_tool_catalog(&self.workspace)?;
        let entry = catalog
            .tools
            .iter()
            .find(|entry| entry.id == plan.resource_id)
            .ok_or_else(|| {
                CoreError::MalformedInput(
                    "post-operation catalog identity is unavailable".to_string(),
                )
            })?;
        let mapping = entry
            .mappings
            .iter()
            .find(|mapping| mapping_key(mapping) == plan.mapping_id)
            .ok_or_else(|| {
                CoreError::MalformedInput(
                    "post-operation manager mapping is unavailable".to_string(),
                )
            })?;
        let evidence = self.manager_evidence.inspect(mapping, executable)?;
        let postcondition_met = match plan.request.action.as_str() {
            "install" | "update" => {
                evidence.installed
                    && evidence.current_version.as_deref() == Some(plan.target_version.as_str())
            }
            "uninstall" => !evidence.installed,
            _ => false,
        };
        if !postcondition_met {
            return Err(CoreError::ProcessExecution(
                "authoritative manager state does not match the reviewed transition".to_string(),
            ));
        }
        if !persist_snapshot {
            return Ok(());
        }
        let _merge = self.snapshot_merge.lock().expect("snapshot merge lock");
        let store = &self.storage;
        let Some(mut snapshot) = store.load_snapshot()? else {
            return Ok(());
        };
        if let Some(tool) = snapshot
            .tools
            .iter_mut()
            .find(|tool| tool.id == plan.resource_id)
        {
            apply_live_mapping(tool, mapping);
            tool.installed_version = evidence.current_version;
            tool.available_version = Some(evidence.target_version);
            tool.state = if evidence.installed {
                InventoryState::ManagedCurrent
            } else {
                InventoryState::Missing
            };
            tool.reason_code = None;
            tool.lifecycle_confidence = evidence.source;
        }
        snapshot.generated_at = format_timestamp(now())?;
        snapshot.freshness = Freshness::Fresh;
        store.persist_snapshot(&snapshot)?;
        Ok(())
    }
    fn merge_codex_migration_target_state(
        &self,
        target_plan: &LifecyclePlan,
    ) -> Result<(), CoreError> {
        if target_plan.mapping_id != "homebrew:codex" || target_plan.resource_id != "codex-cli" {
            return Err(CoreError::LifecycleEvidenceChanged(
                "unexpected migration target identity".to_string(),
            ));
        }
        let _merge = self.snapshot_merge.lock().expect("snapshot merge lock");
        let store = &self.storage;
        let Some(mut snapshot) = store.load_snapshot()? else {
            return Ok(());
        };
        if let Some(tool) = snapshot
            .tools
            .iter_mut()
            .find(|tool| tool.id == "codex-cli")
        {
            tool.installed_version = Some(target_plan.target_version.clone());
            tool.available_version = Some(target_plan.target_version.clone());
            tool.state = InventoryState::ManagedCurrent;
            tool.owner = "Homebrew".to_string();
            tool.ownership_kind = OwnershipKind::ManagerOwned;
            tool.execution_mode = ExecutionMode::ManagedExecute;
            tool.manager = "homebrew".to_string();
            tool.package_id = "codex".to_string();
            tool.reason_code = None;
            tool.lifecycle_confidence = "Live verified Codex provider migration".to_string();
        }
        snapshot.generated_at = format_timestamp(now())?;
        snapshot.freshness = Freshness::Fresh;
        store.persist_snapshot(&snapshot)
    }

    fn finish(
        &self,
        plan: &LifecyclePlan,
        mut result: LifecycleExecutionResult,
        authorization: LifecycleConsentAuthorization,
    ) {
        let completed_at =
            format_timestamp(now()).unwrap_or_else(|_| authorization.granted_at.clone());
        let mut persisted_result = result.clone();
        sanitize_persisted_follow_ups(plan, &mut persisted_result);
        let operation = operation_log_entry(
            plan,
            &persisted_result,
            &authorization,
            Some(completed_at.clone()),
        );
        let _snapshot_merge = self
            .snapshot_merge
            .lock()
            .expect("lifecycle snapshot merge");
        let persistence = self.storage.persist_lifecycle_receipt(
            &operation,
            &persisted_result,
            &authorization,
            &completed_at,
        );
        if let Err(error) = persistence {
            let mutation_completed = result
                .items
                .iter()
                .any(|item| item.status == LifecycleItemStatus::Success);
            result.status = if mutation_completed {
                LifecycleExecutionStatus::Recoverable
            } else {
                LifecycleExecutionStatus::Failed
            };
            result.receipt = None;
            if mutation_completed {
                result.recovery_actions.push(LifecycleFollowUpAction {
                    id: format!("recover:{}", plan.resource_id),
                    label: "Inspect state and review recovery".to_string(),
                    plan_request: plan.request.clone(),
                });
            }
            result.redacted_detail =
                format!("Operation finished but durable receipt storage failed: {error}");
        }
        let mut state = self.state.lock().expect("lifecycle state");
        for manager in manager_keys(plan) {
            state.active_managers.remove(&manager);
        }
        if let Some(runtime) = state.operations.get_mut(&result.operation_id) {
            runtime.result = result;
        }
    }
}

fn operation_log_entry(
    plan: &LifecyclePlan,
    result: &LifecycleExecutionResult,
    authorization: &LifecycleConsentAuthorization,
    completed_at: Option<String>,
) -> OperationLogEntry {
    let receipt = result
        .receipt
        .clone()
        .unwrap_or_else(|| format!("receipt:{}", result.operation_id));
    OperationLogEntry {
        receipt: OperationReceipt {
            id: receipt,
            operation_id: result.operation_id.clone(),
            status: operation_status(&result.status),
            started_at: authorization.granted_at.clone(),
            completed_at,
            summary: result.redacted_detail.clone(),
            details: std::iter::once(format!("Plan digest: {}", result.plan_digest))
                .chain(result.items.iter().map(|item| {
                    format!(
                        "{} | {} | {} | {} | {}",
                        item.id,
                        item.label,
                        item_status_label(&item.status),
                        item.receipt.as_deref().unwrap_or("no receipt"),
                        item.redacted_detail
                    )
                }))
                .collect(),
        },
        resource: plan.resource_id.clone(),
        action: plan.request.action.clone(),
        owner: plan.owner.clone(),
        lifecycle_request: Some(persisted_lifecycle_request(plan, result)),
        lifecycle_result: Some(result.clone()),
        owner_process_id: Some(std::process::id()),
        child_process_id: None,
    }
}

fn sanitize_persisted_follow_ups(plan: &LifecyclePlan, result: &mut LifecycleExecutionResult) {
    for action in result
        .retry_actions
        .iter_mut()
        .chain(result.recovery_actions.iter_mut())
    {
        if action.plan_request.source_analysis_handle.is_some() {
            action.label = "Re-analyze source before retry".to_string();
            action.plan_request = LifecyclePlanRequest {
                resource_kind: action.plan_request.resource_kind.clone(),
                action: "reanalyze-source".to_string(),
                resource_id: plan.resource_id.clone(),
                source_analysis_handle: None,
                item_ids: None,
                children: Vec::new(),
                mapping_id: None,
            };
        }
    }
}

fn restart_safe_request(
    operation: &OperationLogEntry,
    request: LifecyclePlanRequest,
) -> LifecyclePlanRequest {
    if request.source_analysis_handle.is_some() {
        LifecyclePlanRequest {
            resource_kind: request.resource_kind.clone(),
            action: "reanalyze-source".to_string(),
            resource_id: operation.resource.clone(),
            source_analysis_handle: None,
            item_ids: None,
            children: Vec::new(),
            mapping_id: None,
        }
    } else {
        request
    }
}
fn restart_safe_plan_request(plan: &LifecyclePlan) -> LifecyclePlanRequest {
    if plan.request.source_analysis_handle.is_some() {
        LifecyclePlanRequest {
            resource_kind: plan.request.resource_kind.clone(),
            action: "reanalyze-source".to_string(),
            resource_id: plan.resource_id.clone(),
            source_analysis_handle: None,
            item_ids: None,
            children: Vec::new(),
            mapping_id: None,
        }
    } else {
        plan.request.clone()
    }
}

fn opaque_operation_id(plan: &LifecyclePlan, sequence: u64) -> String {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let digest = compute_sha256([
        plan.digest.as_bytes().to_vec(),
        plan.plan_id.as_bytes().to_vec(),
        std::process::id().to_le_bytes().to_vec(),
        sequence.to_le_bytes().to_vec(),
        timestamp.to_le_bytes().to_vec(),
    ]);
    format!(
        "lifecycle-operation-{}",
        digest.trim_start_matches("sha256:")
    )
}

fn validate_authorization(
    plan: &LifecyclePlan,
    authorization: &LifecycleConsentAuthorization,
    current: SystemTime,
) -> Result<(), CoreError> {
    if authorization.plan_digest != plan.digest || authorization.plan_expires_at != plan.expires_at
    {
        return Err(CoreError::LifecycleConsentDenied(
            "consent does not match the reviewed immutable plan".to_string(),
        ));
    }
    let expiry = parse_timestamp(&authorization.plan_expires_at)?;
    let granted = parse_timestamp(&authorization.granted_at)?;
    let checked = parse_timestamp(&plan.revalidation.checked_at)?;

    if current > expiry || granted > expiry || granted + Duration::from_secs(30) < checked {
        return Err(CoreError::LifecycleConsentDenied(
            "consent is stale, expired, or predates plan revalidation".to_string(),
        ));
    }
    if granted > current + Duration::from_secs(30) {
        return Err(CoreError::LifecycleConsentDenied(
            "consent grant time is in the future".to_string(),
        ));
    }
    Ok(())
}
fn apply_live_mapping(current: &mut ToolRecord, mapping: &ToolCatalogMapping) {
    current.owner = match mapping.manager.as_str() {
        "homebrew" => "Homebrew",
        "npm" => "npm",
        "bun" => "Bun",
        "winget" => "WinGet",
        "apt" => "APT",
        "dnf" => "DNF",
        "pacman" => "Pacman",
        "vendor" => "Vendor updater",
        other => other,
    }
    .to_string();
    current.manager = mapping.manager.clone();
    current.package_id = mapping.package_id.clone();
    current.mapping_status = mapping.mapping_status.clone();
    current.ownership_kind = mapping.ownership_kind.clone();
    current.execution_mode = mapping.execution_mode.clone();
    current.platform = mapping.platform.clone();
    current.privilege = mapping.privilege.clone();
}

fn initial_result(plan: &LifecyclePlan, operation_id: &str) -> LifecycleExecutionResult {
    let item_count = match &plan.execution {
        LifecycleExecution::Batch { items } => items.len(),
        _ => 1,
    };
    LifecycleExecutionResult {
        operation_id: operation_id.to_string(),
        plan_digest: plan.digest.clone(),
        status: LifecycleExecutionStatus::InProgress,
        completed_steps: 0,
        total_steps: item_count,
        can_cancel: plan_can_cancel(plan),
        receipt: None,
        redacted_detail: "Lifecycle execution started from the reviewed immutable plan."
            .to_string(),
        items: Vec::new(),
        retry_actions: Vec::new(),
        recovery_actions: Vec::new(),
    }
}

fn plan_can_cancel(plan: &LifecyclePlan) -> bool {
    match &plan.execution {
        LifecycleExecution::ManagedExecute { .. }
        | LifecycleExecution::SignedProductUpdate { .. }
        | LifecycleExecution::ArchiveInstaller { .. } => true,
        LifecycleExecution::NativeInstaller { .. } => false,
        LifecycleExecution::Batch { items } => execution_sequence_can_cancel(items),
        LifecycleExecution::VendorHandoff { .. } | LifecycleExecution::DetectOnly { .. } => false,
    }
}

fn execution_sequence_can_cancel(items: &[LifecyclePlan]) -> bool {
    items
        .iter()
        .find_map(|item| match &item.execution {
            LifecycleExecution::ManagedExecute { .. }
            | LifecycleExecution::SignedProductUpdate { .. }
            | LifecycleExecution::ArchiveInstaller { .. } => Some(true),
            LifecycleExecution::NativeInstaller { .. } => Some(false),
            LifecycleExecution::Batch { items } => Some(execution_sequence_can_cancel(items)),
            LifecycleExecution::VendorHandoff { .. } | LifecycleExecution::DetectOnly { .. } => {
                None
            }
        })
        .unwrap_or(false)
}
fn aggregate_result(
    plan: &LifecyclePlan,
    operation_id: &str,
    outcomes: Vec<(LifecycleItemResult, bool)>,
) -> LifecycleExecutionResult {
    let recovery_required = outcomes.iter().any(|(_, required)| *required);
    let mut recovery_actions: Vec<LifecycleFollowUpAction> = outcomes
        .iter()
        .enumerate()
        .filter(|(_, (_, required))| *required)
        .map(|(index, _)| {
            let request = match &plan.execution {
                LifecycleExecution::Batch { items } => items[index].request.clone(),
                _ => plan.request.clone(),
            };
            LifecycleFollowUpAction {
                id: format!("recover:{}", request.resource_id),
                label: "Inspect state and review recovery".to_string(),
                plan_request: request,
            }
        })
        .collect();
    let items: Vec<_> = outcomes.into_iter().map(|(item, _)| item).collect();
    let successes = items
        .iter()
        .filter(|item| item.status == LifecycleItemStatus::Success)
        .count();
    let failures = items
        .iter()
        .filter(|item| item.status == LifecycleItemStatus::Failed)
        .count();
    let migration_plan = plan.mapping_id == "codex-npm-to-homebrew";
    let migration_needs_recovery = migration_plan
        && (recovery_required
            || items.iter().any(|item| {
                matches!(
                    item.status,
                    LifecycleItemStatus::Failed
                        | LifecycleItemStatus::Cancelled
                        | LifecycleItemStatus::Skipped
                )
            }));
    if migration_needs_recovery {
        let target_succeeded = plan
            .canonical_id
            .starts_with("migration:codex-npm-to-homebrew")
            && items
                .first()
                .is_some_and(|item| item.status == LifecycleItemStatus::Success);
        let (action, label) = if target_succeeded {
            (
                "migrate-cleanup-retry",
                "Review exact npm cleanup with verified Homebrew target",
            )
        } else {
            (
                "inspect-migration",
                "Inspect verified Homebrew target and remaining npm cleanup",
            )
        };
        recovery_actions.clear();
        recovery_actions.push(LifecycleFollowUpAction {
            id: "recover:codex-provider-migration".to_string(),
            label: label.to_string(),
            plan_request: LifecyclePlanRequest {
                resource_kind: LifecycleResourceKind::Operation,
                action: action.to_string(),
                resource_id: "codex-cli".to_string(),
                source_analysis_handle: None,
                item_ids: None,
                children: Vec::new(),
                mapping_id: None,
            },
        });
    }
    let cancelled = items
        .iter()
        .filter(|item| item.status == LifecycleItemStatus::Cancelled)
        .count();
    let skipped = items
        .iter()
        .filter(|item| item.status == LifecycleItemStatus::Skipped)
        .count();
    let status = if cancelled == items.len() {
        LifecycleExecutionStatus::Cancelled
    } else if recovery_required {
        LifecycleExecutionStatus::Recoverable
    } else if failures == 0 && cancelled == 0 && skipped == 0 {
        LifecycleExecutionStatus::Success
    } else if successes > 0 {
        LifecycleExecutionStatus::Partial
    } else if cancelled > 0 {
        LifecycleExecutionStatus::Cancelled
    } else {
        LifecycleExecutionStatus::Failed
    };
    let retry_actions = if migration_plan {
        Vec::new()
    } else {
        items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.status == LifecycleItemStatus::Failed)
            .map(|(index, _)| {
                let request = match &plan.execution {
                    LifecycleExecution::Batch { items } => items[index].request.clone(),
                    _ => plan.request.clone(),
                };
                LifecycleFollowUpAction {
                    id: format!("retry:{}", request.resource_id),
                    label: "Review fresh retry plan".to_string(),
                    plan_request: request,
                }
            })
            .collect()
    };
    let receipt = if matches!(
        status,
        LifecycleExecutionStatus::Success
            | LifecycleExecutionStatus::Partial
            | LifecycleExecutionStatus::Recoverable
    ) {
        Some(receipt_id(plan))
    } else {
        None
    };
    LifecycleExecutionResult {
        operation_id: operation_id.to_string(),
        plan_digest: plan.digest.clone(),
        status,
        completed_steps: items.len(),
        total_steps: items.len(),
        can_cancel: false,
        receipt,
        redacted_detail: if recovery_required {
            format!(
                "Lifecycle mutation completed, but post-operation verification requires recovery: {successes} mutated, {failures} failed, {cancelled} cancelled, {skipped} skipped."
            )
        } else {
            format!(
                "Lifecycle completed: {successes} succeeded, {failures} failed, {cancelled} cancelled, {skipped} skipped."
            )
        },
        items,
        retry_actions,
        recovery_actions,
    }
}

fn failed_result(
    plan: &LifecyclePlan,
    operation_id: &str,
    detail: String,
) -> LifecycleExecutionResult {
    LifecycleExecutionResult {
        operation_id: operation_id.to_string(),
        plan_digest: plan.digest.clone(),
        status: LifecycleExecutionStatus::Failed,
        completed_steps: 0,
        total_steps: 1,
        can_cancel: false,
        receipt: None,
        redacted_detail: detail,
        items: Vec::new(),
        retry_actions: vec![LifecycleFollowUpAction {
            id: format!("retry:{}", plan.resource_id),
            label: "Review fresh retry plan".to_string(),
            plan_request: plan.request.clone(),
        }],
        recovery_actions: Vec::new(),
    }
}

fn cancelled_item(plan: &LifecyclePlan) -> LifecycleItemResult {
    LifecycleItemResult {
        id: plan.canonical_id.clone(),
        label: format!("{} {}", plan.request.action, plan.resource_id),
        status: LifecycleItemStatus::Cancelled,
        receipt: None,
        redacted_detail: "Cancelled before this child plan started.".to_string(),
    }
}

fn receipt_id(plan: &LifecyclePlan) -> String {
    format!("receipt:{}:{}", plan.resource_id, &plan.digest[7..15])
}

fn item_status_label(status: &LifecycleItemStatus) -> &'static str {
    match status {
        LifecycleItemStatus::Pending => "pending",
        LifecycleItemStatus::InProgress => "in_progress",
        LifecycleItemStatus::Success => "success",
        LifecycleItemStatus::Failed => "failed",
        LifecycleItemStatus::Cancelled => "cancelled",
        LifecycleItemStatus::Skipped => "skipped",
    }
}

fn operation_status(status: &LifecycleExecutionStatus) -> OperationStatus {
    match status {
        LifecycleExecutionStatus::Success => OperationStatus::Success,
        LifecycleExecutionStatus::Partial => OperationStatus::Partial,
        LifecycleExecutionStatus::Cancelled => OperationStatus::Cancelled,
        LifecycleExecutionStatus::Recoverable => OperationStatus::Recoverable,
        LifecycleExecutionStatus::InProgress => OperationStatus::InProgress,
        LifecycleExecutionStatus::Failed => OperationStatus::Failed,
    }
}

fn persisted_lifecycle_request(
    plan: &LifecyclePlan,
    result: &LifecycleExecutionResult,
) -> LifecyclePlanRequest {
    if result.status != LifecycleExecutionStatus::Success {
        let mut actions = Vec::new();
        for action in result
            .retry_actions
            .iter()
            .chain(result.recovery_actions.iter())
        {
            if !actions.iter().any(|existing: &LifecycleFollowUpAction| {
                existing.plan_request == action.plan_request
            }) {
                actions.push(action.clone());
            }
        }
        if actions.len() == 1 {
            return actions[0].plan_request.clone();
        }
        if !actions.is_empty() {
            let mut request = plan.request.clone();
            request.source_analysis_handle = None;
            request.item_ids = Some(filtered_batch_item_ids(plan, &actions));
            return request;
        }
        return restart_safe_plan_request(plan);
    }
    LifecyclePlanRequest {
        resource_kind: LifecycleResourceKind::Operation,
        action: "inspect-receipt".to_string(),
        resource_id: result.operation_id.clone(),
        source_analysis_handle: None,
        item_ids: None,
        children: Vec::new(),
        mapping_id: None,
    }
}

fn filtered_batch_item_ids(
    plan: &LifecyclePlan,
    actions: &[LifecycleFollowUpAction],
) -> Vec<String> {
    let LifecycleExecution::Batch { items } = &plan.execution else {
        return actions
            .iter()
            .map(|action| action.plan_request.resource_id.clone())
            .collect();
    };
    let Some(original_item_ids) = plan.request.item_ids.as_ref() else {
        return actions
            .iter()
            .map(|action| action.plan_request.resource_id.clone())
            .collect();
    };
    let filtered = actions
        .iter()
        .filter_map(|action| {
            items
                .iter()
                .position(|item| item.request == action.plan_request)
                .and_then(|index| original_item_ids.get(index))
                .cloned()
        })
        .collect::<Vec<_>>();
    if filtered.len() == actions.len() {
        filtered
    } else {
        original_item_ids.clone()
    }
}

fn mapping_key(mapping: &ToolCatalogMapping) -> String {
    format!("{}:{}", mapping.manager, mapping.package_id)
}

fn manager_keys(plan: &LifecyclePlan) -> Vec<String> {
    match &plan.execution {
        LifecycleExecution::ManagedExecute { .. }
        | LifecycleExecution::SignedProductUpdate { .. } => plan
            .mapping_id
            .split(':')
            .next()
            .map(str::to_string)
            .into_iter()
            .collect(),
        LifecycleExecution::NativeInstaller { .. } => vec![plan.resource_id.clone()],
        LifecycleExecution::ArchiveInstaller { .. } => vec![plan.resource_id.clone()],
        LifecycleExecution::Batch { items } => {
            let mut managers = items.iter().flat_map(manager_keys).collect::<Vec<_>>();
            managers.sort();
            managers.dedup();
            managers
        }
        LifecycleExecution::VendorHandoff { .. } | LifecycleExecution::DetectOnly { .. } => {
            Vec::new()
        }
    }
}

fn request_for_source(
    kind: &SourceKind,
    handle: &str,
    binding: &SourceAnalysisBinding,
) -> LifecyclePlanRequest {
    LifecyclePlanRequest {
        resource_kind: match kind {
            SourceKind::Tool => LifecycleResourceKind::Tool,
            SourceKind::Skill => LifecycleResourceKind::Skill,
            SourceKind::Mcp => LifecycleResourceKind::Mcp,
        },
        action: if binding.record.status == SourceAnalysisStatus::ReviewReady {
            if *kind == SourceKind::Mcp {
                "add"
            } else {
                "install"
            }
        } else {
            "blocked"
        }
        .to_string(),
        resource_id: binding
            .resource_id
            .clone()
            .unwrap_or_else(|| "unmatched-source".to_string()),
        source_analysis_handle: Some(handle.to_string()),
        item_ids: None,
        children: Vec::new(),
        mapping_id: None,
    }
}

fn source_identity(value: &str) -> Result<String, CoreError> {
    let parsed = url::Url::parse(value)?;
    let scheme = parsed.scheme().to_ascii_lowercase();
    let host = parsed.host_str().unwrap_or("unknown").to_ascii_lowercase();
    let port = parsed.port_or_known_default().unwrap_or(0);
    let path = parsed.path().trim_end_matches('/');
    Ok(format!("{scheme}://{host}:{port}{path}"))
}

#[cfg(test)]
mod test_support {
    use std::{
        collections::{BTreeMap, HashMap},
        fs,
        path::PathBuf,
        sync::{Arc, LazyLock, Mutex},
        time::UNIX_EPOCH,
    };

    use crate::{
        catalog::ToolCatalogMapping,
        domain::lifecycle::{LifecycleConsentAuthorization, LifecycleExecutionResult},
        lifecycle::{
            command::{manager_command_vector, CompiledManagerCommand, ExecutableIdentity},
            evidence::{ManagerEvidencePort, ManagerStateEvidence},
            executor::{LifecycleExecutionPort, ManagedExecutionResult},
            source_probe::{SourceProbe, SourceProbeEvidence},
        },
        ports::{HostExecutableResolver, ProcessLiveness, SnapshotStore},
        storage::{OperationLogEntry, SnapshotBundle, StorageHealth},
        CoreError,
    };

    pub(super) struct TestHost;

    impl HostExecutableResolver for TestHost {
        fn compile_manager_command(
            &self,
            mapping: &ToolCatalogMapping,
            action: &str,
            target_version: Option<&str>,
        ) -> Result<Option<CompiledManagerCommand>, CoreError> {
            let Some((_, argv)) = manager_command_vector(mapping, action, target_version)? else {
                return Ok(None);
            };
            let executable = std::env::current_exe()?;
            Ok(Some(CompiledManagerCommand {
                executable: executable.clone(),
                argv,
                identities: vec![self.executable_identity(executable)?],
            }))
        }

        fn manager_evidence_executable(
            &self,
            mapping: &ToolCatalogMapping,
            action: &str,
        ) -> Result<Option<PathBuf>, CoreError> {
            Ok(manager_command_vector(mapping, action, Some("1.0.0"))?
                .map(|_| std::env::current_exe())
                .transpose()?)
        }

        fn executable_identity(&self, path: PathBuf) -> Result<ExecutableIdentity, CoreError> {
            let canonical_path = fs::canonicalize(&path)?;
            let metadata = fs::metadata(&canonical_path)?;
            let modified_epoch_seconds = metadata
                .modified()?
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let sha256 = crate::adapters::compute_sha256([fs::read(&canonical_path)?])
                .trim_start_matches("sha256:")
                .to_string();
            Ok(ExecutableIdentity {
                path,
                canonical_path,
                length: metadata.len(),
                modified_epoch_seconds,
                owner_id: 0,
                sha256,
            })
        }

        fn resolve_executable(&self, name: &str) -> Option<PathBuf> {
            matches!(
                name,
                "brew" | "npm" | "node" | "bun" | "winget" | "apt-get" | "dnf" | "pacman"
            )
            .then(std::env::current_exe)
            .transpose()
            .ok()
            .flatten()
        }

        fn expected_stm_bun_binary_path(&self) -> PathBuf {
            let data = std::env::var_os("STM_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    let home = std::env::var_os("HOME")
                        .or_else(|| std::env::var_os("USERPROFILE"))
                        .map(PathBuf::from)
                        .unwrap_or_else(std::env::temp_dir);
                    if cfg!(target_os = "macos") {
                        home.join("Library/Application Support/stm")
                    } else if cfg!(target_os = "windows") {
                        std::env::var_os("APPDATA")
                            .map(PathBuf::from)
                            .unwrap_or(home)
                            .join("stm")
                    } else {
                        home.join(".local/share/stm")
                    }
                });
            data.join("providers")
                .join("bun")
                .join(crate::domain::recipe::PINNED_BUN_VERSION)
                .join("bin")
                .join(if cfg!(target_os = "windows") {
                    "bun.exe"
                } else {
                    "bun"
                })
        }
    }

    #[derive(Default)]
    pub(super) struct TestSnapshotStore {
        path: PathBuf,
        snapshot: Mutex<Option<SnapshotBundle>>,
        receipts: Mutex<BTreeMap<String, OperationLogEntry>>,
    }

    impl TestSnapshotStore {
        pub(super) fn shared(path: PathBuf) -> Arc<Self> {
            static STORES: LazyLock<Mutex<HashMap<PathBuf, Arc<TestSnapshotStore>>>> =
                LazyLock::new(|| Mutex::new(HashMap::new()));
            let stores = &STORES;
            stores
                .lock()
                .expect("test stores")
                .entry(path.clone())
                .or_insert_with(|| {
                    Arc::new(Self {
                        path,
                        ..Self::default()
                    })
                })
                .clone()
        }
    }

    impl SnapshotStore for TestSnapshotStore {
        fn health(&self) -> StorageHealth {
            StorageHealth {
                path: self.path.display().to_string(),
                user_version: 3,
                recovered_from_corruption: false,
                last_good_available: self.snapshot.lock().expect("snapshot").is_some(),
            }
        }

        fn persist_snapshot(&self, snapshot: &SnapshotBundle) -> Result<(), CoreError> {
            *self.snapshot.lock().expect("snapshot") = Some(snapshot.clone());
            Ok(())
        }

        fn load_snapshot(&self) -> Result<Option<SnapshotBundle>, CoreError> {
            Ok(self.snapshot.lock().expect("snapshot").clone())
        }

        fn persist_lifecycle_receipt(
            &self,
            operation: &OperationLogEntry,
            result: &LifecycleExecutionResult,
            _: &LifecycleConsentAuthorization,
            _: &str,
        ) -> Result<(), CoreError> {
            let mut operation = operation.clone();
            operation.lifecycle_result = Some(result.clone());
            self.receipts
                .lock()
                .expect("receipts")
                .insert(result.operation_id.clone(), operation);
            Ok(())
        }

        fn reconcile_lifecycle_receipt(
            &self,
            operation: &OperationLogEntry,
            result: &LifecycleExecutionResult,
            _: &str,
        ) -> Result<(), CoreError> {
            let mut operation = operation.clone();
            operation.lifecycle_result = Some(result.clone());
            self.receipts
                .lock()
                .expect("receipts")
                .insert(result.operation_id.clone(), operation);
            Ok(())
        }

        fn checkpoint_lifecycle_result(
            &self,
            result: &LifecycleExecutionResult,
            _: &str,
        ) -> Result<(), CoreError> {
            let mut receipts = self.receipts.lock().expect("receipts");
            let operation = receipts.get_mut(&result.operation_id).ok_or_else(|| {
                CoreError::Sqlite("test lifecycle receipt is missing".to_string())
            })?;
            operation.lifecycle_result = Some(result.clone());
            operation.child_process_id = None;
            Ok(())
        }

        fn persist_lifecycle_child_process(
            &self,
            operation_id: &str,
            child_process_id: u32,
        ) -> Result<(), CoreError> {
            let mut receipts = self.receipts.lock().expect("receipts");
            let operation = receipts.get_mut(operation_id).ok_or_else(|| {
                CoreError::Sqlite("test lifecycle receipt is missing".to_string())
            })?;
            operation.child_process_id = Some(child_process_id);
            Ok(())
        }

        fn load_lifecycle_receipts(&self) -> Result<Vec<OperationLogEntry>, CoreError> {
            Ok(self
                .receipts
                .lock()
                .expect("receipts")
                .values()
                .cloned()
                .collect())
        }
    }

    pub(super) struct TestProcessLiveness;

    impl ProcessLiveness for TestProcessLiveness {
        fn is_alive(&self, process_id: u32) -> bool {
            process_id == std::process::id()
        }
    }

    pub(super) struct TestExecutor;

    impl LifecycleExecutionPort for TestExecutor {
        fn execute_managed(
            &self,
            _: &str,
            _: &[String],
            _: &[ExecutableIdentity],
            _: &(dyn Fn(u32) -> Result<(), CoreError> + Send + Sync),
            _: &crate::feasibility::process_supervisor::CancelSignal,
        ) -> Result<ManagedExecutionResult, CoreError> {
            Err(CoreError::ProcessExecution(
                "test application service cannot execute lifecycle mutations".to_string(),
            ))
        }

        fn open_vendor_handoff(&self, _: &str) -> Result<(), CoreError> {
            Ok(())
        }
    }

    pub(super) struct TestSourceProbe;

    impl SourceProbe for TestSourceProbe {
        fn probe(&self, url: &str) -> Result<SourceProbeEvidence, CoreError> {
            Ok(SourceProbeEvidence {
                final_url: url.to_string(),
                status: 200,
                content_length: Some(0),
                sampled_bytes: 0,
            })
        }
    }

    pub(super) struct TestManagerEvidence;

    impl ManagerEvidencePort for TestManagerEvidence {
        fn inspect(
            &self,
            _: &ToolCatalogMapping,
            _: &str,
        ) -> Result<ManagerStateEvidence, CoreError> {
            Ok(ManagerStateEvidence {
                installed: false,
                current_version: None,
                target_version: "1.0.0".to_string(),
                update_available: false,
                source: "test manager evidence".to_string(),
            })
        }
    }
}
include!("service_tests.rs");
