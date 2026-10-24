#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod elevate;

#[tauri::command]
fn app_version() -> String {
    health_core::version().to_string()
}

fn main() {
    tauri::Builder::default()
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            app_version,
            commands::start_scan,
            commands::cancel_scan,
            commands::get_fix_plan,
            commands::apply_fix_cmd,
            commands::get_session,
            commands::export_report,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
