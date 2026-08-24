use std::{fs, fs::OpenOptions, io::Write, path::PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tools_manager_core::domain::lifecycle::{LifecycleExecutionResult, LifecyclePlan};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProductUpdateReceipt<'a> {
    schema_version: u64,
    operation_id: &'a str,
    plan_digest: &'a str,
    version_receipt: Option<&'a str>,
    status: String,
    completed_at: String,
    redacted_detail: &'a str,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PendingProductInstall {
    pub operation_id: String,
    pub plan_digest: String,
    pub current_version: String,
    pub target_version: String,
    pub started_at: String,
}

pub(super) fn persist_pending_install(
    app: &AppHandle,
    plan: &LifecyclePlan,
    operation_id: &str,
) -> Result<(), String> {
    let path = pending_path(app)?;
    if path.exists() {
        return Err("A pending product install already requires reconciliation".into());
    }
    write_private(
        &path,
        &serde_json::to_vec_pretty(&PendingProductInstall {
            operation_id: operation_id.to_string(),
            plan_digest: plan.digest.clone(),
            current_version: plan.current_version.clone(),
            target_version: plan.target_version.clone(),
            started_at: OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .map_err(|_| "Pending product install timestamp failed".to_string())?,
        })
        .map_err(|_| "Pending product install serialization failed".to_string())?,
    )
}

pub(super) fn load_pending_install(
    app: &AppHandle,
) -> Result<Option<PendingProductInstall>, String> {
    let path = pending_path(app)?;
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| "Pending product install metadata failed".to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 64 * 1024 {
        return Err("Pending product install record rejected".into());
    }
    let value = serde_json::from_slice(
        &fs::read(path).map_err(|_| "Pending product install read failed".to_string())?,
    )
    .map_err(|_| "Pending product install record is malformed".to_string())?;
    Ok(Some(value))
}

pub(super) fn clear_pending_install(app: &AppHandle) -> Result<(), String> {
    let path = pending_path(app)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Pending product install record could not be removed".into()),
    }
}

pub(super) fn persist_product_receipt(
    app: &AppHandle,
    result: &LifecycleExecutionResult,
) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|_| "Product update receipt directory is unavailable".to_string())?
        .join("product-updates");
    fs::create_dir_all(&root)
        .map_err(|_| "Product update receipt directory could not be created".to_string())?;
    let status = format!("{:?}", result.status).to_ascii_lowercase();
    let receipt_path = root.join(format!("{}-{status}.json", result.operation_id));
    if receipt_path.exists()
        || fs::symlink_metadata(&root).is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return Err("Product update receipt path is not a fresh regular target".into());
    }
    let payload = serde_json::to_vec_pretty(&ProductUpdateReceipt {
        schema_version: 1,
        operation_id: &result.operation_id,
        plan_digest: &result.plan_digest,
        version_receipt: result.receipt.as_deref(),
        status,
        completed_at: OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|_| "Product update receipt timestamp failed".to_string())?,
        redacted_detail: &result.redacted_detail,
    })
    .map_err(|_| "Product update receipt serialization failed".to_string())?;
    if payload.len() > 64 * 1024 {
        return Err("Product update receipt exceeds its storage bound".into());
    }
    write_private(&receipt_path, &payload)?;
    Ok(receipt_path)
}
fn pending_path(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|_| "Product update receipt directory is unavailable".to_string())?
        .join("product-updates");
    fs::create_dir_all(&root)
        .map_err(|_| "Product update receipt directory could not be created".to_string())?;
    if fs::symlink_metadata(&root).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("Product update receipt directory symlink rejected".into());
    }
    Ok(root.join("pending-install.json"))
}

fn write_private(path: &PathBuf, payload: &[u8]) -> Result<(), String> {
    if payload.len() > 64 * 1024 || path.exists() {
        return Err("Product update receipt path or size rejected".into());
    }
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|_| "Product update receipt could not be opened".to_string())?;
    file.write_all(payload)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|_| "Product update receipt could not be persisted".to_string())
}
