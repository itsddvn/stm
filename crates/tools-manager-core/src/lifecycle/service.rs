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
        source::{SourceAnalysisRecord, SourceAnalysisStatus, SourceKind, SourceTrust},
        tool::ToolRecord,
    },
    error::CoreError,
    feasibility::{process_supervisor::CancelSignal, source_analysis::analyze_source},
    storage::{OperationLogEntry, SqliteSnapshotStore},
};

use super::{
    command::manager_evidence_executable,
    evidence::{real_manager_evidence, ManagerEvidencePort},
    executor::{real_executor, LifecycleExecutionPort},
    planner::{prepare_plan, PreparedPlan},
    source_probe::{analyze_source_with_probe, BoundedHttpsSourceProbe, SourceProbe},
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
    pub fn new(workspace: FixtureWorkspace) -> Self {
        Self::with_ports(
            workspace,
            real_executor(),
            Arc::new(BoundedHttpsSourceProbe),
            real_manager_evidence(),
        )
    }

    pub(crate) fn with_ports(
        workspace: FixtureWorkspace,
        executor: Arc<dyn LifecycleExecutionPort>,
        source_probe: Arc<dyn SourceProbe>,
        manager_evidence: Arc<dyn ManagerEvidencePort>,
    ) -> Self {
        let service = Self {
            workspace,
            state: Arc::new(Mutex::new(LifecycleState::default())),
            executor,
            source_probe,
            manager_evidence,
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

    pub(crate) fn refresh_tool_postcondition(
        &self,
        tool_id: &str,
        current: &mut ToolRecord,
    ) -> Result<bool, CoreError> {
        let catalog = load_tool_catalog(&self.workspace)?;
        let Some(entry) = catalog.tools.iter().find(|entry| entry.id == tool_id) else {
            return Ok(false);
        };
        let Some(mapping) = crate::inventory::mapping_for_platform(
            entry,
            crate::inventory::current_platform_slug(),
            crate::inventory::current_native_linux_manager(),
        ) else {
            return Ok(false);
        };
        if mapping.mapping_status != MappingStatus::Supported
            || mapping.execution_mode != ExecutionMode::ManagedExecute
            || mapping.ownership_kind != OwnershipKind::ManagerOwned
        {
            return Ok(false);
        }
        let mut executable = None;
        for action in ["update", "install", "uninstall"] {
            if let Some(candidate) = manager_evidence_executable(mapping, action)? {
                executable = Some(candidate);
                break;
            }
        }
        let Some(executable) = executable else {
            return Ok(false);
        };
        let evidence_path = executable.to_str().ok_or_else(|| {
            CoreError::CommandDenied("reviewed manager executable path is not UTF-8".to_string())
        })?;
        let evidence = self.manager_evidence.inspect(mapping, evidence_path)?;
        current.installed_version = evidence.current_version;
        current.available_version = Some(evidence.target_version);
        current.state = if !evidence.installed {
            InventoryState::Missing
        } else if evidence.update_available {
            InventoryState::ManagedUpdateAvailable
        } else {
            InventoryState::ManagedCurrent
        };
        current.reason_code = None;
        current.lifecycle_confidence = evidence.source;
        Ok(true)
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
        let (store, _) = SqliteSnapshotStore::open(self.workspace.db_path())?;
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
            let owner_is_live = operation.owner_process_id.is_some_and(process_is_alive);
            let child_is_live = operation.child_process_id.is_some_and(process_is_alive);
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
                    });
            let restart_request = restart_safe_request(&operation, request);
            let evidence_summary = match prepare_plan(
                &self.workspace,
                self.manager_evidence.as_ref(),
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
            SqliteSnapshotStore::open(self.workspace.db_path()).and_then(|(store, _)| {
                store.persist_lifecycle_receipt(
                    &operation,
                    &initial,
                    &authorization,
                    &authorization.granted_at,
                )
            })
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
        if refreshed.status != SourceAnalysisStatus::ReviewReady
            || resolved_resource_id.as_deref() != Some(expected_resource_id)
        {
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
        let source = self.source_binding(&original.plan.request)?;
        let current = prepare_plan(
            &self.workspace,
            self.manager_evidence.as_ref(),
            original.plan.request.clone(),
            source.as_ref(),
            0,
            now(),
        )?;
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
                )
            };
            return aggregate_result(&prepared.plan, operation_id, vec![outcome]);
        }

        let mut outcomes = Vec::with_capacity(prepared.children.len());
        for child in &prepared.children {
            if cancel.is_cancelled() {
                outcomes.push((cancelled_item(&child.plan), false));
                continue;
            }
            match self.revalidate(child) {
                Ok(()) => outcomes.push(self.execute_plan(
                    &child.plan,
                    operation_id,
                    cancel,
                    &child.executable_identities,
                )),
                Err(error) => outcomes.push((
                    LifecycleItemResult {
                        id: child.plan.canonical_id.clone(),
                        label: format!("{} {}", child.plan.request.action, child.plan.resource_id),
                        status: LifecycleItemStatus::Failed,
                        receipt: None,
                        redacted_detail: format!("Revalidation failed before execution: {error}"),
                    },
                    false,
                )),
            }
        }
        aggregate_result(&prepared.plan, operation_id, outcomes)
    }

    fn execute_plan(
        &self,
        plan: &LifecyclePlan,
        operation_id: &str,
        cancel: &CancelSignal,
        identities: &[super::command::ExecutableIdentity],
    ) -> (LifecycleItemResult, bool) {
        let label = format!("{} {}", plan.request.action, plan.resource_id);
        let mut recovery_required = false;
        let item = match &plan.execution {
            LifecycleExecution::ManagedExecute { executable, argv }
            | LifecycleExecution::SignedProductUpdate { executable, argv } => {
                let on_spawn = |process_id| {
                    let (store, _) = SqliteSnapshotStore::open(self.workspace.db_path())?;
                    store.persist_lifecycle_child_process(operation_id, process_id)
                };
                match self
                    .executor
                    .execute_managed(executable, argv, identities, &on_spawn, cancel)
                {
                    Ok(outcome) if outcome.cancelled => LifecycleItemResult {
                        id: plan.canonical_id.clone(),
                        label,
                        status: LifecycleItemStatus::Cancelled,
                        receipt: None,
                        redacted_detail: outcome.redacted_detail,
                    },
                    Ok(outcome) if outcome.success => {
                        match self.verify_and_merge_postcondition(plan, executable) {
                            Ok(()) => LifecycleItemResult {
                                id: plan.canonical_id.clone(),
                                label,
                                status: LifecycleItemStatus::Success,
                                receipt: Some(receipt_id(plan)),
                                redacted_detail: outcome.redacted_detail,
                            },
                            Err(error) => {
                                recovery_required = true;
                                LifecycleItemResult {
                                    id: plan.canonical_id.clone(),
                                    label,
                                    status: LifecycleItemStatus::Success,
                                    receipt: Some(receipt_id(plan)),
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

        let _merge = self.snapshot_merge.lock().expect("snapshot merge lock");
        let (store, _) = SqliteSnapshotStore::open(self.workspace.db_path())?;
        let Some(mut snapshot) = store.load_snapshot()? else {
            return Ok(());
        };
        if let Some(tool) = snapshot
            .tools
            .iter_mut()
            .find(|tool| tool.id == plan.resource_id)
        {
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
        let persistence =
            SqliteSnapshotStore::open(self.workspace.db_path()).and_then(|(store, _)| {
                store.persist_lifecycle_receipt(
                    &operation,
                    &persisted_result,
                    &authorization,
                    &completed_at,
                )
            });
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
            result.can_cancel = false;
            result.receipt = None;
            if mutation_completed && result.recovery_actions.is_empty() {
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
        }
    } else {
        plan.request.clone()
    }
}
#[cfg(unix)]
fn process_is_alive(process_id: u32) -> bool {
    if process_id == 0 || process_id > libc::pid_t::MAX as u32 {
        return false;
    }
    let result = unsafe { libc::kill(process_id as libc::pid_t, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn process_is_alive(process_id: u32) -> bool {
    if process_id == 0 {
        return false;
    }
    use windows_sys::Win32::{
        Foundation::{CloseHandle, STILL_ACTIVE},
        System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if handle.is_null() {
        return false;
    }
    let mut exit_code = 0;
    let queried = unsafe { GetExitCodeProcess(handle, &mut exit_code) } != 0;
    unsafe {
        CloseHandle(handle);
    }
    queried && exit_code == STILL_ACTIVE as u32
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
        can_cancel: matches!(
            plan.execution,
            LifecycleExecution::ManagedExecute { .. }
                | LifecycleExecution::SignedProductUpdate { .. }
                | LifecycleExecution::Batch { .. }
        ),
        receipt: None,
        redacted_detail: "Lifecycle execution started from the reviewed immutable plan."
            .to_string(),
        items: Vec::new(),
        retry_actions: Vec::new(),
        recovery_actions: Vec::new(),
    }
}

fn aggregate_result(
    plan: &LifecyclePlan,
    operation_id: &str,
    outcomes: Vec<(LifecycleItemResult, bool)>,
) -> LifecycleExecutionResult {
    let recovery_required = outcomes.iter().any(|(_, required)| *required);
    let recovery_actions = outcomes
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
    } else if recovery_required && failures == 0 && cancelled == 0 && skipped == 0 {
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
    let retry_actions = items
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
        .collect();
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

include!("service_tests.rs");
