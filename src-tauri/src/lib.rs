use tauri::Manager;

mod commands;
mod product_update;
mod product_update_contract;
mod product_update_receipt;
mod signed_update_metadata;
mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();
    let updater_enabled = context.config().plugins.0.contains_key("updater");
    let builder =
        tauri::Builder::default().plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }));
    let builder = if updater_enabled {
        builder.plugin(tauri_plugin_updater::Builder::new().build())
    } else {
        builder
    };
    builder
        .manage(state::AppState::new_runtime(env!("CARGO_MANIFEST_DIR")))
        .manage(product_update::ProductUpdateRuntime::new(updater_enabled))
        .setup(|app| {
            app.state::<product_update::ProductUpdateRuntime>()
                .reconcile_startup(app.handle())
                .map_err(std::io::Error::other)?;
            Ok(())
        })
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
        .run(context)
        .expect("error while running STM");
}
