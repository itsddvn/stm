use std::{path::PathBuf, sync::Arc};

use crate::{
    catalog::ToolCatalogMapping,
    domain::{
        lifecycle::{LifecycleConsentAuthorization, LifecycleExecutionResult},
        mcp::McpDiscoveryReport,
        operation::{ConsentRecord, OperationPlan, OperationReceipt},
        skill::SkillScanReport,
        source::{SourceAnalysisRecord, SourceKind},
    },
    error::CoreError,
    feasibility::{
        elevation::ElevationStrategy,
        manager_probe::{ManagerKind, ManagerProbeReport},
        process_supervisor::{CancelSignal, ExecutionOutcome, ExecutionRequest},
    },
    lifecycle::{CompiledManagerCommand, ExecutableIdentity},
    storage::{OperationLogEntry, SnapshotBundle, StorageHealth},
};

pub trait CatalogSource {
    fn validate(&self) -> Result<(), CoreError>;
}

pub trait SkillClient {
    fn scan(&self) -> Result<SkillScanReport, CoreError>;
}

pub trait McpClientConfiguration {
    fn discover(&self) -> Result<McpDiscoveryReport, CoreError>;
}

pub trait SourceAnalyzer {
    fn analyze(&self, kind: SourceKind, url: &str) -> Result<SourceAnalysisRecord, CoreError>;
}

pub trait ReceiptRepository {
    fn persist_consent(&self, consent: &ConsentRecord) -> Result<(), CoreError>;
    fn persist_receipt(&self, receipt: &OperationReceipt) -> Result<(), CoreError>;
}

pub trait Clock {
    fn now_rfc3339(&self) -> String;
}

pub trait ProcessSupervisor {
    fn execute(
        &self,
        request: &ExecutionRequest,
        cancel: &CancelSignal,
    ) -> Result<ExecutionOutcome, CoreError>;
}

pub trait ProcessLiveness: Send + Sync {
    fn is_alive(&self, process_id: u32) -> bool;
}

pub trait ElevationBroker {
    fn current_strategy(&self) -> ElevationStrategy;
}

pub trait ApplicationUpdater {
    fn current_channel(&self) -> Result<String, CoreError>;
}

pub trait SnapshotStore: Send + Sync {
    fn health(&self) -> StorageHealth;
    fn persist_snapshot(&self, snapshot: &SnapshotBundle) -> Result<(), CoreError>;
    fn load_snapshot(&self) -> Result<Option<SnapshotBundle>, CoreError>;
    fn persist_lifecycle_receipt(
        &self,
        operation: &OperationLogEntry,
        result: &LifecycleExecutionResult,
        authorization: &LifecycleConsentAuthorization,
        recorded_at: &str,
    ) -> Result<(), CoreError>;
    fn reconcile_lifecycle_receipt(
        &self,
        operation: &OperationLogEntry,
        result: &LifecycleExecutionResult,
        recorded_at: &str,
    ) -> Result<(), CoreError>;
    fn checkpoint_lifecycle_result(
        &self,
        result: &LifecycleExecutionResult,
        recorded_at: &str,
    ) -> Result<(), CoreError>;
    fn persist_lifecycle_child_process(
        &self,
        operation_id: &str,
        child_process_id: u32,
    ) -> Result<(), CoreError>;
    fn load_lifecycle_receipts(&self) -> Result<Vec<OperationLogEntry>, CoreError>;
}

pub trait SnapshotStoreFactory: Send + Sync {
    fn open(&self, path: PathBuf) -> Result<Arc<dyn SnapshotStore>, CoreError>;
}

pub trait HostExecutableResolver: Send + Sync {
    fn compile_manager_command(
        &self,
        mapping: &ToolCatalogMapping,
        action: &str,
        target_version: Option<&str>,
    ) -> Result<Option<CompiledManagerCommand>, CoreError>;
    fn manager_evidence_executable(
        &self,
        mapping: &ToolCatalogMapping,
        action: &str,
    ) -> Result<Option<PathBuf>, CoreError>;
    fn executable_identity(&self, path: PathBuf) -> Result<ExecutableIdentity, CoreError>;
    fn resolve_executable(&self, name: &str) -> Option<PathBuf>;
    fn expected_stm_bun_binary_path(&self) -> PathBuf;
}

pub trait InventoryAdapter {
    fn probe(&self, manager: ManagerKind) -> Result<ManagerProbeReport, CoreError>;
}

pub trait OperationPlanner {
    fn plan(
        &self,
        resource_type: &str,
        resource_id: &str,
        action: &str,
    ) -> Result<OperationPlan, CoreError>;
}
