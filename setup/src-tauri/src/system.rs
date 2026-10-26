//! What the installer does to the machine: files, shortcuts, the uninstall entry.
//! Everything is per user (no administrator rights). Paths come from the config and
//! the user's own known folders, never from the window.

use crate::logic::{uninstall_dir, Config, Product};
use serde::Serialize;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use windows::core::{Interface, GUID, HSTRING};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Programs, IShellLinkW, SHGetKnownFolderPath, ShellLink, KF_FLAG_DEFAULT,
};
use winreg::{enums::HKEY_CURRENT_USER, RegKey};

static PAYLOAD_APP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload_app.exe"));
static PAYLOAD_HELPER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload_helper.exe"));

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const UNINSTALLER: &str = "uninstall.exe";
/// A running program keeps its file locked; a closing one is given this long.
const LOCK_RETRIES: u32 = 30;
const LOCK_WAIT: Duration = Duration::from_millis(500);

/// `code` picks the text shown in the window (`texts.errors.<code>`); `detail` is technical.
#[derive(Serialize, Debug)]
pub struct SetupError {
    pub code: &'static str,
    pub detail: String,
}

impl SetupError {
    fn new(code: &'static str, detail: impl ToString) -> Self {
        SetupError { code, detail: detail.to_string() }
    }
    fn io(e: std::io::Error) -> Self {
        SetupError::new("generic", e)
    }
}

pub fn payload_ok() -> bool {
    !PAYLOAD_APP.is_empty() && !PAYLOAD_HELPER.is_empty()
}

fn uninstall_key(p: &Product) -> String {
    format!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}", p.slug)
}

pub fn installed_version(p: &Product) -> Option<String> {
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey(uninstall_key(p)).ok()?;
    key.get_value::<String, _>("DisplayVersion").ok()
}

pub fn install_dir(p: &Product) -> Result<PathBuf, SetupError> {
    let base = std::env::var_os("LOCALAPPDATA").ok_or_else(|| SetupError::new("generic", "LOCALAPPDATA is not set"))?;
    Ok(PathBuf::from(base).join("Programs").join(&p.folder))
}

fn step<'a>(cfg: &'a Config, key: &str) -> &'a str {
    cfg.texts["steps"][key].as_str().unwrap_or("")
}

/// Writes next to the target, then swaps it in; waits while the old file is still in use.
fn write_replace(path: &Path, bytes: &[u8]) -> Result<(), SetupError> {
    let tmp = path.with_extension("new");
    std::fs::write(&tmp, bytes).map_err(SetupError::io)?;
    let mut last = None;
    for _ in 0..LOCK_RETRIES {
        match std::fs::rename(&tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(LOCK_WAIT);
            }
        }
    }
    let _ = std::fs::remove_file(&tmp);
    Err(SetupError::new("locked", last.map(|e| e.to_string()).unwrap_or_default()))
}

fn remove_waiting(path: &Path) -> Result<(), SetupError> {
    let mut last = None;
    for _ in 0..LOCK_RETRIES {
        match std::fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(LOCK_WAIT);
            }
        }
    }
    Err(SetupError::new("locked", last.map(|e| e.to_string()).unwrap_or_default()))
}

fn known_folder(id: &GUID) -> Result<PathBuf, SetupError> {
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).map_err(|e| SetupError::new("generic", e))?;
        let s = p.to_string().map_err(|e| SetupError::new("generic", e));
        CoTaskMemFree(Some(p.0 as _));
        Ok(PathBuf::from(s?))
    }
}

fn create_shortcut(lnk: &Path, target: &Path, description: &str) -> Result<(), SetupError> {
    let e = |e: windows::core::Error| SetupError::new("generic", e);
    unsafe {
        // S_FALSE (already initialised on this thread) is fine; the pairing call is harmless.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let result = (|| {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(e)?;
            link.SetPath(&HSTRING::from(target.as_os_str())).map_err(e)?;
            if let Some(dir) = target.parent() {
                link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str())).map_err(e)?;
            }
            link.SetDescription(&HSTRING::from(description)).map_err(e)?;
            link.SetIconLocation(&HSTRING::from(target.as_os_str()), 0).map_err(e)?;
            let file: IPersistFile = link.cast().map_err(e)?;
            file.Save(&HSTRING::from(lnk.as_os_str()), true).map_err(e)
        })();
        CoUninitialize();
        result
    }
}

fn shortcut_paths(p: &Product) -> Result<(PathBuf, PathBuf), SetupError> {
    let file = format!("{}.lnk", p.name);
    Ok((known_folder(&FOLDERID_Programs)?.join(&file), known_folder(&FOLDERID_Desktop)?.join(file)))
}

fn write_registry(p: &Product, dir: &Path) -> Result<(), SetupError> {
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(uninstall_key(p))
        .map_err(SetupError::io)?;
    let exe = dir.join(&p.exe);
    let set = |name: &str, value: String| key.set_value(name, &value).map_err(SetupError::io);
    set("DisplayName", p.name.clone())?;
    set("DisplayVersion", env!("CARGO_PKG_VERSION").to_string())?;
    set("Publisher", p.publisher.clone())?;
    set("InstallLocation", dir.display().to_string())?;
    set("DisplayIcon", exe.display().to_string())?;
    set("UninstallString", format!("\"{}\" --uninstall", dir.join(UNINSTALLER).display()))?;
    key.set_value("NoModify", &1u32).map_err(SetupError::io)?;
    key.set_value("NoRepair", &1u32).map_err(SetupError::io)?;
    let kb = ((PAYLOAD_APP.len() + PAYLOAD_HELPER.len()) / 1024) as u32;
    key.set_value("EstimatedSize", &kb).map_err(SetupError::io)
}

/// Installs or updates: the two program files, a copy of this installer as the uninstaller,
/// a Start menu shortcut (desktop optional) and the Apps & features entry.
pub fn install(cfg: &Config, desktop: bool, progress: &dyn Fn(u8, &str)) -> Result<(), SetupError> {
    let p = &cfg.product;
    let dir = install_dir(p)?;
    progress(5, step(cfg, "prepare"));
    std::fs::create_dir_all(&dir).map_err(SetupError::io)?;

    progress(15, step(cfg, "app"));
    write_replace(&dir.join(&p.exe), PAYLOAD_APP)?;
    progress(45, step(cfg, "helper"));
    write_replace(&dir.join(&p.helper), PAYLOAD_HELPER)?;

    progress(65, step(cfg, "uninstaller"));
    let me = std::env::current_exe().map_err(SetupError::io)?;
    let dest = dir.join(UNINSTALLER);
    if me != dest {
        write_replace(&dest, &std::fs::read(&me).map_err(SetupError::io)?)?;
    }

    progress(80, step(cfg, "shortcuts"));
    let (start_menu, on_desktop) = shortcut_paths(p)?;
    let target = dir.join(&p.exe);
    create_shortcut(&start_menu, &target, &p.description)?;
    if desktop {
        create_shortcut(&on_desktop, &target, &p.description)?;
    }

    progress(92, step(cfg, "register"));
    write_registry(p, &dir)?;
    progress(100, "");
    Ok(())
}

/// Removes the program, its shortcuts and its entry. The folder itself (which still holds this
/// running copy) is deleted by a short-lived helper process once this one has exited.
pub fn uninstall(cfg: &Config, progress: &dyn Fn(u8, &str)) -> Result<(), SetupError> {
    let p = &cfg.product;
    let me = std::env::current_exe().map_err(SetupError::io)?;
    let dir = uninstall_dir(&me, p).ok_or_else(|| SetupError::new("generic", "not an installed copy"))?;

    progress(15, step(cfg, "remove_files"));
    remove_waiting(&dir.join(&p.exe))?;
    remove_waiting(&dir.join(&p.helper))?;

    progress(55, step(cfg, "remove_shortcuts"));
    let (start_menu, on_desktop) = shortcut_paths(p)?;
    for lnk in [start_menu, on_desktop] {
        let _ = std::fs::remove_file(lnk);
    }

    progress(80, step(cfg, "remove_register"));
    let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(uninstall_key(p));

    // `dir` was checked by `uninstall_dir` (our own folder name, our own file next to it).
    std::process::Command::new("cmd.exe")
        .raw_arg(format!("/C ping -n 3 127.0.0.1 >nul & rmdir /S /Q \"{}\"", dir.display()))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(SetupError::io)?;
    progress(100, "");
    Ok(())
}

pub fn launch(p: &Product) -> Result<(), SetupError> {
    let dir = install_dir(p)?;
    std::process::Command::new(dir.join(&p.exe))
        .current_dir(&dir)
        .spawn()
        .map(|_| ())
        .map_err(SetupError::io)
}
