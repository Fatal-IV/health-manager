#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod elevate;
mod update;

#[tauri::command]
fn app_version() -> String {
    health_core::version().to_string()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            app_version,
            commands::start_scan,
            commands::cancel_scan,
            commands::open_windows_update,
            commands::get_system_summary,
            commands::get_fix_plan,
            commands::apply_fix_cmd,
            commands::get_session,
            commands::export_report,
            update::check_update,
            update::install_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
