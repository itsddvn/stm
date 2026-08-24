use tauri::{AppHandle, Emitter, State};
use tools_manager_core::{
    application::{
        dto::{
            AppViewModelDto, McpServerViewModelDto, OperationViewModelDto, RefreshStatusDto,
            SkillViewModelDto, SourceAnalysisViewModelDto, ToolViewModelDto, UpdateViewModelDto,
        },
        service::{DiagnosticsReport, HeadlessScanResult},
    },
    domain::{
        lifecycle::{
            LifecycleConsentAuthorization, LifecycleExecutionResult, LifecyclePlan,
            LifecyclePlanRequest, LifecycleResourceKind,
        },
        source::SourceKind,
    },
    CoreError,
};

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
pub fn list_updates(state: State<'_, AppState>) -> CommandResult<Vec<UpdateViewModelDto>> {
    state.service().list_updates().map_err(render_error)
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
        product_updates.prepare(&app, request).await
    } else {
        state
            .service()
            .prepare_lifecycle(request)
            .map_err(render_error)
    }
}

#[tauri::command]
pub async fn start_lifecycle_operation(
    app: AppHandle,
    plan_id: String,
    authorization: LifecycleConsentAuthorization,
    state: State<'_, AppState>,
    product_updates: State<'_, ProductUpdateRuntime>,
) -> CommandResult<LifecycleExecutionResult> {
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

fn render_error(error: CoreError) -> String {
    error.to_string()
}
