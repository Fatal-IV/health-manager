#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod system;

use setup_logic::logic::{config, decide_mode, Config, Mode};
use serde::{Deserialize, Serialize};
use system::SetupError;
use tauri::{Emitter, State};

struct Ctx {
    config: Config,
    mode: Mode,
    auto_start: bool,
    installed: Option<String>,
}

#[derive(Serialize)]
struct Info {
    config: Config,
    mode: Mode,
    auto_start: bool,
    installed: Option<String>,
    version: &'static str,
    payload_ok: bool,
}

#[derive(Deserialize)]
struct Options {
    desktop_shortcut: bool,
}

#[derive(Serialize, Clone)]
struct Progress {
    percent: u8,
    message: String,
}

#[tauri::command]
fn get_setup_info(ctx: State<'_, Ctx>) -> Info {
    Info {
        config: ctx.config.clone(),
        mode: ctx.mode,
        auto_start: ctx.auto_start,
        installed: ctx.installed.clone(),
        version: env!("CARGO_PKG_VERSION"),
        payload_ok: system::payload_ok(),
    }
}

#[tauri::command]
async fn run_setup(app: tauri::AppHandle, ctx: State<'_, Ctx>, options: Options) -> Result<(), SetupError> {
    let (cfg, mode) = (ctx.config.clone(), ctx.mode);
    tauri::async_runtime::spawn_blocking(move || {
        let progress = |percent: u8, message: &str| {
            let _ = app.emit("setup-progress", Progress { percent, message: message.to_string() });
        };
        match mode {
            Mode::Uninstall => system::uninstall(&cfg, &progress),
            _ if !system::payload_ok() => Err(SetupError { code: "generic", detail: "installer carries no files".into() }),
            _ => system::install(&cfg, options.desktop_shortcut, &progress),
        }
    })
    .await
    .map_err(|e| SetupError { code: "generic", detail: e.to_string() })?
}

#[tauri::command]
fn launch_app(ctx: State<'_, Ctx>) -> Result<(), SetupError> {
    system::launch(&ctx.config.product)
}

fn main() {
    let config = config();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let own_name = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let installed = system::installed_version(&config.product);
    let (mode, auto_start) = decide_mode(&args, &own_name, installed.as_deref(), env!("CARGO_PKG_VERSION"));

    tauri::Builder::default()
        .manage(Ctx { config, mode, auto_start, installed })
        .invoke_handler(tauri::generate_handler![get_setup_info, run_setup, launch_app])
        .run(tauri::generate_context!())
        .expect("error while running the installer");
}
