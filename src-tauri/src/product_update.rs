use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use sha2::{Digest, Sha256};
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;
use time::OffsetDateTime;
use tools_manager_core::{
    application::dto::OperationViewModelDto,
    domain::{
        lifecycle::{
            LifecycleConsentAuthorization, LifecycleExecution, LifecycleExecutionResult,
            LifecycleExecutionStatus, LifecycleItemResult, LifecycleItemStatus, LifecyclePlan,
            LifecyclePlanRequest, LifecyclePrivilege, LifecycleResourceKind, LifecycleRevalidation,
            LifecycleRevalidationState,
        },
        operation::OperationStatus,
    },
};

use crate::product_update_contract::{
    format_timestamp, opaque_id, plan_digest, terminal_result, validate_authorization,
};
use crate::product_update_receipt::{
    clear_pending_install, load_pending_install, persist_pending_install, persist_product_receipt,
};
use crate::signed_update_metadata::{verify_release_metadata, VerifiedReleaseMetadata};

const PLAN_TTL_SECONDS: i64 = 10 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProductUpdateIdentity {
    version: String,
    target: String,
    download_url: String,
    signature_sha256: String,
    metadata_sha256: String,
}

#[derive(Clone)]
struct ProductPlanEntry {
    plan: LifecyclePlan,
    identity: Option<ProductUpdateIdentity>,
}

#[derive(Default)]
pub struct ProductUpdateRuntime {
    sequence: AtomicU64,
    plans: Mutex<BTreeMap<String, ProductPlanEntry>>,
    operations: Arc<Mutex<BTreeMap<String, LifecycleExecutionResult>>>,
    operation_started_at: Mutex<BTreeMap<String, String>>,
    active_operation: Arc<Mutex<Option<String>>>,
    updater_enabled: bool,
}

struct ActiveProductGuard {
    active: Arc<Mutex<Option<String>>>,
    armed: bool,
}

impl ActiveProductGuard {
    fn acquire(active: &Arc<Mutex<Option<String>>>, plan_id: &str) -> Result<Self, String> {
        let mut operation = active
            .lock()
            .map_err(|_| "Product update exclusion state is unavailable".to_string())?;
        if operation.is_some() {
            return Err("Another product update is already starting or in progress".into());
        }
        *operation = Some(plan_id.to_string());
        drop(operation);
        Ok(Self {
            active: active.clone(),
            armed: true,
        })
    }

    fn transfer(&mut self) {
        self.armed = false;
    }
}

impl Drop for ActiveProductGuard {
    fn drop(&mut self) {
        if self.armed {
            if let Ok(mut operation) = self.active.lock() {
                *operation = None;
            }
        }
    }
}

impl ProductUpdateRuntime {
    pub fn new(updater_enabled: bool) -> Self {
        Self {
            updater_enabled,
            ..Self::default()
        }
    }

    pub async fn prepare(
        &self,
        app: &AppHandle,
        request: LifecyclePlanRequest,
    ) -> Result<LifecyclePlan, String> {
        let sequence = self.sequence.fetch_add(1, Ordering::SeqCst) + 1;
        let checked_at = OffsetDateTime::now_utc();
        let signed_metadata = if self.updater_enabled {
            verify_release_metadata(app).await.ok()
        } else {
            None
        };
        let update = if signed_metadata.is_some() {
            match app
                .updater_builder()
                .timeout(Duration::from_secs(15))
                .build()
            {
                Ok(updater) => updater.check().await.unwrap_or(None),
                Err(_) => None,
            }
        } else {
            None
        };
        let (current_version, target_version, identity, source, execution, confidence, limitations) =
            if let Some(update) = update {
                if update.download_url.scheme() != "https"
                    || !update.download_url.username().is_empty()
                    || update.download_url.password().is_some()
                    || update.download_url.fragment().is_some()
                {
                    return Err(
                        "Signed product update endpoint is not credential-free HTTPS".into(),
                    );
                }
                let metadata = signed_metadata.as_ref().ok_or_else(|| {
                    "Signed product update metadata authentication is unavailable".to_string()
                })?;
                if !metadata.matches_update(&update) {
                    return Err("Signed product update differs from authenticated metadata".into());
                }
                let identity = update_identity(&update, metadata);
                (
                    update.current_version,
                    Some(identity.version.clone()),
                    Some(identity.clone()),
                    identity.download_url.clone(),
                    LifecycleExecution::SignedProductUpdate {
                        executable: "tauri-signed-updater".into(),
                        argv: vec![
                            identity.target.clone(),
                            identity.version.clone(),
                            identity.signature_sha256.clone(),
                            identity.metadata_sha256.clone(),
                        ],
                    },
                    "Versioned updater metadata accepted; artifact signature is verified before installation."
                        .to_string(),
                    vec![
                        "Product update trust, artifacts, receipts, and recovery are separate from tool, skill, and MCP lifecycle state.".into(),
                        "Installation requires an authenticated release manifest and signed platform artifact; restart remains explicit after installation.".into(),
                    ],
                )
            } else {
                (
                    app.package_info().version.to_string(),
                    None,
                    None,
                    "Signed product channel".into(),
                    LifecycleExecution::DetectOnly {
                        guidance: "No newer signed release is available, or this internal build has no release updater configuration.".into(),
                    },
                    "No executable signed product update is available.".into(),
                    vec![
                        "Public release builds fail closed unless updater endpoint and public key configuration are injected by the protected release workflow.".into(),
                    ],
                )
            };
        let target_label = target_version
            .clone()
            .unwrap_or_else(|| current_version.clone());
        let plan_id = opaque_id("product-plan", sequence, &target_label, checked_at);
        let expires_at = checked_at + time::Duration::seconds(PLAN_TTL_SECONDS);
        let mut plan = LifecyclePlan {
            request,
            plan_id,
            canonical_id: "stm-product".into(),
            mapping_id: "signed-product-updater:stable".into(),
            resource_id: "stm".into(),
            owner: "Signed STM product channel".into(),
            source,
            current_version,
            target_version: target_label,
            privilege: if target_version.is_some() {
                LifecyclePrivilege::UserConfirmation
            } else {
                LifecyclePrivilege::None
            },
            affected_paths: vec!["application:com.itsddvn.stm".into()],
            affected_records: identity
                .as_ref()
                .map(|identity| {
                    vec![
                        "product-release:stable".into(),
                        format!("product-signature:{}", identity.signature_sha256),
                    ]
                })
                .unwrap_or_else(|| vec!["product-release:stable".into()]),
            confidence,
            limitations,
            digest: String::new(),
            expires_at: format_timestamp(expires_at)?,
            revalidation: LifecycleRevalidation {
                state: LifecycleRevalidationState::Fresh,
                checked_at: format_timestamp(checked_at)?,
                checks: vec![
                    "Recheck stable-channel version, target, exact HTTPS download URL, and signature fingerprint before installation.".into(),
                    "Reject downgrade, wrong target, missing signature, corrupt download, and expired consent.".into(),
                ],
            },
            execution,
        };
        plan.digest = plan_digest(&plan)?;
        self.plans
            .lock()
            .map_err(|_| "Product update plan state is unavailable".to_string())?
            .insert(
                plan.plan_id.clone(),
                ProductPlanEntry {
                    plan: plan.clone(),
                    identity,
                },
            );
        Ok(plan)
    }

    pub fn contains_plan(&self, plan_id: &str) -> bool {
        self.plans
            .lock()
            .is_ok_and(|plans| plans.contains_key(plan_id))
    }

    pub async fn start(
        &self,
        app: &AppHandle,
        plan_id: &str,
        authorization: LifecycleConsentAuthorization,
    ) -> Result<LifecycleExecutionResult, String> {
        let mut active_guard = ActiveProductGuard::acquire(&self.active_operation, plan_id)?;
        let entry = self
            .plans
            .lock()
            .map_err(|_| "Product update plan state is unavailable".to_string())?
            .remove(plan_id)
            .ok_or_else(|| "Product update plan is unavailable or already consumed".to_string())?;
        validate_authorization(&entry.plan, &authorization)?;
        let Some(identity) = entry.identity else {
            return Ok(terminal_result(
                &entry.plan,
                "product-update-unavailable",
                LifecycleExecutionStatus::Failed,
                LifecycleItemStatus::Skipped,
                "No newer signed product release is available.",
            ));
        };
        let target_version = identity.version.clone();
        let metadata = verify_release_metadata(app)
            .await
            .map_err(|_| "Signed product update metadata revalidation failed".to_string())?;
        let update = app
            .updater_builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| "Signed product updater is unavailable".to_string())?
            .check()
            .await
            .map_err(|_| "Signed product update revalidation failed".to_string())?
            .filter(|update| update_identity(update, &metadata) == identity)
            .ok_or_else(|| {
                "Signed product update evidence changed; review a fresh plan".to_string()
            })?;
        let operation_id = opaque_id(
            "product-operation",
            self.sequence.fetch_add(1, Ordering::SeqCst) + 1,
            &target_version,
            OffsetDateTime::now_utc(),
        );
        let started_at = format_timestamp(OffsetDateTime::now_utc())?;
        let progress = LifecycleExecutionResult {
            operation_id: operation_id.clone(),
            plan_digest: entry.plan.digest.clone(),
            status: LifecycleExecutionStatus::InProgress,
            completed_steps: 0,
            total_steps: 1,
            can_cancel: false,
            receipt: None,
            redacted_detail: "Downloading and verifying the signed STM product artifact.".into(),
            items: vec![LifecycleItemResult {
                id: "stm-product".into(),
                label: format!("Install STM {target_version}"),
                status: LifecycleItemStatus::InProgress,
                receipt: None,
                redacted_detail: "Signed artifact download is in progress.".into(),
            }],
            retry_actions: Vec::new(),
            recovery_actions: Vec::new(),
        };
        persist_pending_install(app, &entry.plan, &operation_id)?;
        if let Err(error) = persist_product_receipt(app, &progress) {
            let _ = clear_pending_install(app);
            return Err(error);
        }
        self.operations
            .lock()
            .map_err(|_| "Product update operation state is unavailable".to_string())?
            .insert(operation_id.clone(), progress.clone());
        self.operation_started_at
            .lock()
            .map_err(|_| "Product update history state is unavailable".to_string())?
            .insert(operation_id.clone(), started_at);
        let app = app.clone();
        let operations = self.operations.clone();
        let plan = entry.plan.clone();
        let active_operation = self.active_operation.clone();
        tauri::async_runtime::spawn(async move {
            let result = update.download_and_install(|_, _| {}, || {}).await;
            let mut terminal = match result {
                Ok(()) => terminal_result(
                    &plan,
                    &operation_id,
                    LifecycleExecutionStatus::Success,
                    LifecycleItemStatus::Success,
                    "Signed STM product artifact installed; restart STM to run the new version.",
                ),
                Err(_) => terminal_result(
                    &plan,
                    &operation_id,
                    LifecycleExecutionStatus::Failed,
                    LifecycleItemStatus::Failed,
                    "Signed product update failed without changing tool, skill, or MCP configuration state.",
                ),
            };
            let persisted = persist_product_receipt(&app, &terminal).is_ok();
            if !persisted && terminal.status == LifecycleExecutionStatus::Success {
                terminal.status = LifecycleExecutionStatus::Recoverable;
                terminal.receipt = None;
                terminal.redacted_detail =
                    "Signed product artifact installed, but its durable product receipt failed; review recovery before restart."
                        .into();
                if let Some(item) = terminal.items.first_mut() {
                    item.status = LifecycleItemStatus::Failed;
                    item.receipt = None;
                    item.redacted_detail = terminal.redacted_detail.clone();
                }
            }
            if persisted {
                let _ = clear_pending_install(&app);
            }
            if let Ok(mut state) = operations.lock() {
                state.insert(operation_id, terminal);
            }
            if let Ok(mut active) = active_operation.lock() {
                *active = None;
            }
        });
        active_guard.transfer();
        Ok(progress)
    }

    pub fn reconcile_startup(&self, app: &AppHandle) -> Result<(), String> {
        let Some(pending) = load_pending_install(app)? else {
            return Ok(());
        };
        let started_at = pending.started_at.clone();
        let operation_id = pending.operation_id.clone();
        let installed = app.package_info().version.to_string() == pending.target_version;
        let detail = if installed {
            "Signed STM product update completed before restart."
        } else {
            "A pending product install did not converge to the reviewed version; reinstall or review recovery."
        };
        let receipt = installed.then(|| format!("product-update:{}", pending.target_version));
        let result = LifecycleExecutionResult {
            operation_id: pending.operation_id.clone(),
            plan_digest: pending.plan_digest,
            status: if installed {
                LifecycleExecutionStatus::Success
            } else {
                LifecycleExecutionStatus::Recoverable
            },
            completed_steps: usize::from(installed),
            total_steps: 1,
            can_cancel: false,
            receipt: receipt.clone(),
            redacted_detail: detail.into(),
            items: vec![LifecycleItemResult {
                id: "stm-product".into(),
                label: format!("STM {}", pending.target_version),
                status: if installed {
                    LifecycleItemStatus::Success
                } else {
                    LifecycleItemStatus::Failed
                },
                receipt,
                redacted_detail: detail.into(),
            }],
            retry_actions: Vec::new(),
            recovery_actions: Vec::new(),
        };
        persist_product_receipt(app, &result)?;
        clear_pending_install(app)?;
        self.operations
            .lock()
            .map_err(|_| "Product update operation state is unavailable".to_string())?
            .insert(operation_id.clone(), result);
        self.operation_started_at
            .lock()
            .map_err(|_| "Product update history state is unavailable".to_string())?
            .insert(operation_id, started_at);
        Ok(())
    }

    pub fn operation_views(&self) -> Result<Vec<OperationViewModelDto>, String> {
        let operations = self
            .operations
            .lock()
            .map_err(|_| "Product update operation state is unavailable".to_string())?;
        let started = self
            .operation_started_at
            .lock()
            .map_err(|_| "Product update history state is unavailable".to_string())?;
        let mut views = operations
            .values()
            .map(|result| OperationViewModelDto {
                id: result.operation_id.clone(),
                resource: "STM".into(),
                action: "Product update".into(),
                status: operation_status(&result.status),
                started_at: started
                    .get(&result.operation_id)
                    .cloned()
                    .unwrap_or_else(|| "unknown".into()),
                owner: "Signed STM product channel".into(),
                detail: result.redacted_detail.clone(),
                receipt: result
                    .receipt
                    .clone()
                    .unwrap_or_else(|| "No receipt".into()),
                details: result
                    .items
                    .iter()
                    .map(|item| {
                        format!(
                            "{} | {:?} | {}",
                            item.label, item.status, item.redacted_detail
                        )
                    })
                    .collect(),
                lifecycle_request: LifecyclePlanRequest {
                    resource_kind: LifecycleResourceKind::Product,
                    action: "product-update".into(),
                    resource_id: "stm".into(),
                    source_analysis_handle: None,
                    item_ids: None,
                },
            })
            .collect::<Vec<_>>();
        views.sort_by(|left, right| right.started_at.cmp(&left.started_at));
        Ok(views)
    }

    pub fn contains_operation(&self, operation_id: &str) -> bool {
        self.operations
            .lock()
            .is_ok_and(|operations| operations.contains_key(operation_id))
    }

    pub fn status(&self, operation_id: &str) -> Result<LifecycleExecutionResult, String> {
        self.operations
            .lock()
            .map_err(|_| "Product update operation state is unavailable".to_string())?
            .get(operation_id)
            .cloned()
            .ok_or_else(|| "Product update operation is unavailable".to_string())
    }
}
fn operation_status(status: &LifecycleExecutionStatus) -> OperationStatus {
    match status {
        LifecycleExecutionStatus::InProgress => OperationStatus::InProgress,
        LifecycleExecutionStatus::Success => OperationStatus::Success,
        LifecycleExecutionStatus::Partial => OperationStatus::Partial,
        LifecycleExecutionStatus::Failed => OperationStatus::Failed,
        LifecycleExecutionStatus::Cancelled => OperationStatus::Cancelled,
        LifecycleExecutionStatus::Recoverable => OperationStatus::Recoverable,
    }
}

fn update_identity(
    update: &tauri_plugin_updater::Update,
    metadata: &VerifiedReleaseMetadata,
) -> ProductUpdateIdentity {
    ProductUpdateIdentity {
        version: update.version.clone(),
        target: update.target.clone(),
        download_url: update.download_url.to_string(),
        signature_sha256: format!("sha256:{:x}", Sha256::digest(update.signature.as_bytes())),
        metadata_sha256: metadata.digest.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_update_exclusion_rejects_replay_until_terminal_release() {
        let active = Arc::new(Mutex::new(None));
        {
            let _guard = ActiveProductGuard::acquire(&active, "plan-1").expect("first");
            assert!(ActiveProductGuard::acquire(&active, "plan-1").is_err());
            assert!(ActiveProductGuard::acquire(&active, "plan-2").is_err());
        }
        assert!(ActiveProductGuard::acquire(&active, "plan-2").is_ok());
    }

    #[test]
    fn reconciled_product_result_is_exposed_through_operation_history() {
        let runtime = ProductUpdateRuntime::default();
        runtime.operations.lock().expect("operations").insert(
            "product-operation-1".into(),
            LifecycleExecutionResult {
                operation_id: "product-operation-1".into(),
                plan_digest: "sha256:fixture".into(),
                status: LifecycleExecutionStatus::Recoverable,
                completed_steps: 0,
                total_steps: 1,
                can_cancel: false,
                receipt: None,
                redacted_detail: "reconcile".into(),
                items: Vec::new(),
                retry_actions: Vec::new(),
                recovery_actions: Vec::new(),
            },
        );
        runtime
            .operation_started_at
            .lock()
            .expect("history")
            .insert("product-operation-1".into(), "2026-08-21T00:00:00Z".into());

        let views = runtime.operation_views().expect("views");

        assert_eq!(views.len(), 1);
        assert_eq!(views[0].id, "product-operation-1");
        assert_eq!(views[0].status, OperationStatus::Recoverable);
        assert_eq!(
            views[0].lifecycle_request.resource_kind,
            LifecycleResourceKind::Product
        );
    }
}
