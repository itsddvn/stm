use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static PORTABLE_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;
#[cfg(target_os = "windows")]
use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

use stm_core::capabilities::{InstallProviderPreference, QuickSetupView};
use stm_core::domain::migration::MigrationCandidate;
use stm_core::domain::portable::PortableImportResult;
use stm_core::domain::provider::{PreferenceSnapshot, ProviderKind};
use stm_core::{
    application::{
        dto::{
            AppViewModelDto, McpServerViewModelDto, OperationViewModelDto, RefreshStatusDto,
            SkillViewModelDto, SourceAnalysisViewModelDto, ToolViewModelDto, UpdateViewModelDto,
        },
        service::{DiagnosticsReport, HeadlessScanResult},
    },
    domain::{
        application_update::ApplicationUpdateKind,
        lifecycle::{
            LifecycleConsentAuthorization, LifecycleExecutionResult, LifecyclePlan,
            LifecyclePlanRequest, LifecycleResourceKind,
        },
        source::SourceKind,
    },
    CoreError,
};
use stm_runtime::{
    default_data_dir, detect_provider_inventory, download_and_verify_homebrew_pkg,
    prepare_bun_binary,
};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::{product_update::ProductUpdateRuntime, state::AppState};

type CommandResult<T> = Result<T, String>;

#[tauri::command]
pub fn refresh_snapshot(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<AppViewModelDto> {
    Ok(state.start_refresh(app))
}

#[tauri::command]
pub fn refresh_status(state: State<'_, AppState>) -> CommandResult<RefreshStatusDto> {
    Ok(state.refresh_status())
}

#[tauri::command]
pub fn headless_scan(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<HeadlessScanResult> {
    let result = state.service().headless_scan().map_err(render_error)?;
    for event in &result.events {
        app.emit(crate::state::REFRESH_EVENT_NAME, event)
            .map_err(|error| error.to_string())?;
    }
    Ok(result)
}

#[tauri::command]
pub fn list_tools(state: State<'_, AppState>) -> CommandResult<Vec<ToolViewModelDto>> {
    state.service().list_tools().map_err(render_error)
}

#[tauri::command]
pub fn get_tool_detail(
    id: String,
    state: State<'_, AppState>,
) -> CommandResult<Option<ToolViewModelDto>> {
    state.service().get_tool_detail(&id).map_err(render_error)
}

#[tauri::command]
pub fn list_skills(state: State<'_, AppState>) -> CommandResult<Vec<SkillViewModelDto>> {
    state.service().list_skills().map_err(render_error)
}

#[tauri::command]
pub fn get_skill_detail(
    id: String,
    state: State<'_, AppState>,
) -> CommandResult<Option<SkillViewModelDto>> {
    state.service().get_skill_detail(&id).map_err(render_error)
}

#[tauri::command]
pub fn list_mcp_servers(state: State<'_, AppState>) -> CommandResult<Vec<McpServerViewModelDto>> {
    state.service().list_mcp_servers().map_err(render_error)
}

#[tauri::command]
pub fn get_mcp_detail(
    id: String,
    state: State<'_, AppState>,
) -> CommandResult<Option<McpServerViewModelDto>> {
    state.service().get_mcp_detail(&id).map_err(render_error)
}

#[tauri::command]
pub async fn list_updates(
    app: AppHandle,
    state: State<'_, AppState>,
    product_updates: State<'_, ProductUpdateRuntime>,
) -> CommandResult<Vec<UpdateViewModelDto>> {
    let mut updates = state.service().list_updates().map_err(render_error)?;
    let product = UpdateViewModelDto::from(&product_updates.availability(&app).await);
    upsert_product_update(&mut updates, product);
    Ok(updates)
}

#[tauri::command]
pub fn list_operations(
    state: State<'_, AppState>,
    product_updates: State<'_, ProductUpdateRuntime>,
) -> CommandResult<Vec<OperationViewModelDto>> {
    let mut operations = state.service().list_operations().map_err(render_error)?;
    operations.extend(product_updates.operation_views()?);
    operations.sort_by(|left, right| right.started_at.cmp(&left.started_at));
    Ok(operations)
}

#[tauri::command]
pub fn analyze_source(
    kind: SourceKind,
    url: String,
    state: State<'_, AppState>,
) -> CommandResult<SourceAnalysisViewModelDto> {
    state
        .service()
        .analyze_source(kind, &url)
        .map_err(render_error)
}
#[tauri::command]
pub async fn prepare_lifecycle_plan(
    app: AppHandle,
    request: LifecyclePlanRequest,
    state: State<'_, AppState>,
    product_updates: State<'_, ProductUpdateRuntime>,
) -> CommandResult<LifecyclePlan> {
    if request.resource_kind == LifecycleResourceKind::Product {
        return product_updates.prepare(&app, request).await;
    }
    let providers = detect_provider_inventory();
    let requirements = state
        .service()
        .setup_queue_bootstrap_requirements(&request, &providers)
        .map_err(render_error)?;
    let requires_homebrew = requirements.contains(&ProviderKind::Homebrew);
    let requires_bun = requirements.contains(&ProviderKind::Bun);
    if requires_homebrew || requires_bun {
        let homebrew = if requires_homebrew {
            Some(
                download_and_verify_homebrew_pkg(&default_data_dir().join("bootstrap"))
                    .map_err(|error| error.to_string())?
                    .into_core(),
            )
        } else {
            None
        };
        let bun = if requires_bun {
            let data_dir = default_data_dir();
            Some(
                prepare_bun_binary(&data_dir.join("bootstrap"), &data_dir.join("providers/bun"))
                    .map_err(|error| error.to_string())?
                    .into_core(),
            )
        } else {
            None
        };
        return state
            .service()
            .prepare_lifecycle_with_provider_bootstraps(
                request,
                providers,
                homebrew.as_ref(),
                bun.as_ref(),
            )
            .map_err(render_error);
    }
    state
        .service()
        .prepare_lifecycle_with_providers(request, providers)
        .map_err(render_error)
}

#[tauri::command]
pub async fn start_lifecycle_operation(
    app: AppHandle,
    plan_id: String,
    authorization: LifecycleConsentAuthorization,
    locale: Option<String>,
    state: State<'_, AppState>,
    product_updates: State<'_, ProductUpdateRuntime>,
) -> CommandResult<LifecycleExecutionResult> {
    let locale = locale.unwrap_or_else(|| "vi".to_string());
    let summary = if product_updates.contains_plan(&plan_id) {
        product_updates.native_confirmation_summary(&plan_id, &locale)?
    } else {
        state
            .service()
            .native_confirmation_summary(&plan_id, &locale)
            .map_err(render_error)?
    };
    let confirmed = app
        .dialog()
        .message(summary)
        .title(if locale.starts_with("en") {
            "Confirm STM changes"
        } else {
            "Xác nhận thay đổi của STM"
        })
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancel)
        .blocking_show();
    if !confirmed {
        return Err(render_error(CoreError::LifecycleConsentDenied(
            "native confirmation denied".to_string(),
        )));
    }
    if product_updates.contains_plan(&plan_id) {
        product_updates.start(&app, &plan_id, authorization).await
    } else {
        state
            .service()
            .start_lifecycle(&plan_id, authorization)
            .map_err(render_error)
    }
}

#[tauri::command]
pub fn lifecycle_operation_status(
    operation_id: String,
    state: State<'_, AppState>,
    product_updates: State<'_, ProductUpdateRuntime>,
) -> CommandResult<LifecycleExecutionResult> {
    if product_updates.contains_operation(&operation_id) {
        product_updates.status(&operation_id)
    } else {
        state
            .service()
            .lifecycle_status(&operation_id)
            .map_err(render_error)
    }
}

#[tauri::command]
pub fn cancel_lifecycle_operation(
    operation_id: String,
    state: State<'_, AppState>,
    product_updates: State<'_, ProductUpdateRuntime>,
) -> CommandResult<LifecycleExecutionResult> {
    if product_updates.contains_operation(&operation_id) {
        product_updates.status(&operation_id)
    } else {
        state
            .service()
            .cancel_lifecycle(&operation_id)
            .map_err(render_error)
    }
}

#[tauri::command]
pub fn cancel_operation(operation_id: String, state: State<'_, AppState>) -> bool {
    state.cancel_operation(&operation_id)
}

#[tauri::command]
pub fn run_diagnostics(state: State<'_, AppState>) -> CommandResult<DiagnosticsReport> {
    state.service().diagnostics().map_err(render_error)
}

#[tauri::command]
pub fn get_quick_setup(state: State<'_, AppState>) -> CommandResult<QuickSetupView> {
    state
        .service()
        .quick_setup(detect_provider_inventory())
        .map_err(render_error)
}

#[tauri::command]
pub fn get_migration_candidates(
    state: State<'_, AppState>,
) -> CommandResult<Vec<MigrationCandidate>> {
    state
        .service()
        .migration_candidates(&detect_provider_inventory())
        .map_err(render_error)
}
#[tauri::command]
pub fn set_provider_preference(
    preference: InstallProviderPreference,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    state
        .service()
        .set_provider_preference(preference)
        .map_err(render_error)
}

#[tauri::command]
pub fn dismiss_quick_setup(state: State<'_, AppState>) -> CommandResult<()> {
    state.service().dismiss_quick_setup().map_err(render_error)
}

#[tauri::command]
pub fn get_setup_preferences(state: State<'_, AppState>) -> CommandResult<PreferenceSnapshot> {
    Ok(state.service().setup_preferences())
}
#[tauri::command]
pub fn validate_portable_setup(
    bytes: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<String>> {
    state
        .service()
        .import_portable_bytes(bytes.as_bytes())
        .map(|result| result.warnings)
        .map_err(render_error)
}

#[tauri::command]
pub fn import_portable_setup(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<Option<PortableImportResult>> {
    let Some(selected) = app
        .dialog()
        .file()
        .add_filter("STM setup", &["json"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = selected.into_path().map_err(|error| error.to_string())?;
    let file = OpenOptions::new()
        .read(true)
        .open(&path)
        .map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("portable setup must be a regular file".to_string());
    }
    let mut bytes = Vec::with_capacity(
        (metadata.len() as usize).min(stm_core::domain::portable::MAX_PORTABLE_BYTES + 1),
    );
    file.take((stm_core::domain::portable::MAX_PORTABLE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > stm_core::domain::portable::MAX_PORTABLE_BYTES {
        return Err("portable setup exceeds 64 KiB".to_string());
    }
    state
        .service()
        .import_portable_bytes(&bytes)
        .map(Some)
        .map_err(render_error)
}

#[tauri::command]
pub fn export_portable_setup(
    app: AppHandle,
    target: String,
    state: State<'_, AppState>,
) -> CommandResult<Option<String>> {
    let bytes = state
        .service()
        .export_portable_setup(&target)
        .map_err(render_error)?;
    let Some(selected) = app
        .dialog()
        .file()
        .set_file_name(format!("stm-setup-{target}.json"))
        .add_filter("STM setup", &["json"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = selected.into_path().map_err(|error| error.to_string())?;
    atomic_write(&path, &bytes)?;
    Ok(Some(
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("stm-setup.json")
            .to_string(),
    ))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> CommandResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| "export path has no parent".to_string())?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("stm-setup.json");
    let (temp, mut file) = create_random_temp(parent, name)?;
    let result = (|| -> CommandResult<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|error| error.to_string())?;
        }
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        replace_export_file(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn create_random_temp(parent: &Path, name: &str) -> CommandResult<(PathBuf, fs::File)> {
    for _ in 0..16 {
        let sequence = PORTABLE_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temp = parent.join(format!(".{name}.stm-{}-{sequence}.tmp", std::process::id()));
        match OpenOptions::new().create_new(true).write(true).open(&temp) {
            Ok(file) => return Ok((temp, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("could not create a private export temporary file".to_string())
}

#[cfg(not(target_os = "windows"))]
fn replace_export_file(temp: &Path, path: &Path) -> CommandResult<()> {
    fs::rename(temp, path).map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
fn replace_export_file(temp: &Path, path: &Path) -> CommandResult<()> {
    if !path.exists() {
        return fs::rename(temp, path).map_err(|error| error.to_string());
    }
    let destination = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replacement = temp
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replaced = unsafe {
        ReplaceFileW(
            destination.as_ptr(),
            replacement.as_ptr(),
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if replaced == 0 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}

fn upsert_product_update(updates: &mut Vec<UpdateViewModelDto>, product: UpdateViewModelDto) {
    if let Some(existing) = updates
        .iter_mut()
        .find(|update| update.resource_type == ApplicationUpdateKind::Product)
    {
        *existing = product;
    } else {
        updates.push(product);
    }
}

fn render_error(error: CoreError) -> String {
    error.to_string()
}
#[cfg(test)]
mod tests {
    use stm_core::domain::application_update::UpdateExecutionMode;

    use super::*;

    fn update(id: &str, resource_type: ApplicationUpdateKind) -> UpdateViewModelDto {
        UpdateViewModelDto {
            id: id.into(),
            resource_type,
            name: id.into(),
            current: "1.0.0".into(),
            target: "2.0.0".into(),
            execution_mode: UpdateExecutionMode::ManagedExecute,
            selected: false,
            risk: String::new(),
            selection_action: None,
            review_action: None,
        }
    }

    #[test]
    fn product_availability_upsert_preserves_tool_and_skill_updates() {
        let mut updates = vec![
            update("update-tool", ApplicationUpdateKind::Tool),
            update("update-skill", ApplicationUpdateKind::Skill),
            update("stale-product", ApplicationUpdateKind::Product),
        ];
        let mut product = update("update-product", ApplicationUpdateKind::Product);
        product.execution_mode = UpdateExecutionMode::DetectOnly;

        upsert_product_update(&mut updates, product);

        assert_eq!(updates.len(), 3);
        assert_eq!(updates[0].id, "update-tool");
        assert_eq!(updates[1].id, "update-skill");
        assert_eq!(updates[2].id, "update-product");
        assert_eq!(updates[2].execution_mode, UpdateExecutionMode::DetectOnly);
    }
}
