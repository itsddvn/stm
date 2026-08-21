use tauri::Manager;

mod commands;
mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .manage(state::AppState::new(env!("CARGO_MANIFEST_DIR")))
        .invoke_handler(tauri::generate_handler![
            commands::refresh_snapshot,
            commands::refresh_status,
            commands::headless_scan,
            commands::list_tools,
            commands::get_tool_detail,
            commands::list_skills,
            commands::get_skill_detail,
            commands::list_mcp_servers,
            commands::get_mcp_detail,
            commands::list_updates,
            commands::list_operations,
            commands::analyze_source,
            commands::prepare_lifecycle_plan,
            commands::start_lifecycle_operation,
            commands::lifecycle_operation_status,
            commands::cancel_lifecycle_operation,
            commands::cancel_operation,
            commands::run_diagnostics,
        ])
        .run(tauri::generate_context!())
        .expect("error while running STM");
}
