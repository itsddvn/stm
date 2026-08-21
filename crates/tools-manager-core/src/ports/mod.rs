use std::path::Path;

use crate::{
    domain::{
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

pub trait ElevationBroker {
    fn current_strategy(&self) -> ElevationStrategy;
}

pub trait ApplicationUpdater {
    fn current_channel(&self) -> Result<String, CoreError>;
}

pub trait SqliteStore {
    fn open(&self, path: &Path) -> Result<(), CoreError>;
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
