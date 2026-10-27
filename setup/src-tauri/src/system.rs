//! What the installer does to the machine: files, shortcuts, the startup task, the
//! uninstall entry. The installer runs with administrator rights (its manifest asks for
//! them once), because the program goes to Program Files: a folder only administrators
//! can write, which is what makes the elevated `fix-helper.exe` there trustworthy.
//! Paths come from the config and Windows' own known folders, never from the window.

use setup_logic::logic::{boot_task_xml, uninstall_dir, Config, Product};
use serde::Serialize;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use windows::core::{Interface, GUID, HSTRING};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::SystemInformation::{GetSystemDirectoryW, GetWindowsDirectoryW};
use windows::Win32::UI::Shell::{
    FOLDERID_CommonPrograms, FOLDERID_Desktop, FOLDERID_Programs, FOLDERID_PublicDesktop, IShellLinkW,
    SHGetKnownFolderPath, ShellLink, KF_FLAG_DEFAULT,
};
use winreg::{
    enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
    RegKey,
};

static PAYLOAD_APP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload_app.exe"));
static PAYLOAD_HELPER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload_helper.exe"));

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const UNINSTALLER: &str = "uninstall.exe";
const STATE_DIR: &str = "state";
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

/// The machine-wide install's version, if there is one.
pub fn installed_version(p: &Product) -> Option<String> {
    let key = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(uninstall_key(p)).ok()?;
    key.get_value::<String, _>("DisplayVersion").ok()
}

pub fn install_dir(p: &Product) -> Result<PathBuf, SetupError> {
    let base = std::env::var_os("ProgramFiles").ok_or_else(|| SetupError::new("generic", "ProgramFiles is not set"))?;
    Ok(PathBuf::from(base).join(&p.folder))
}

fn step<'a>(cfg: &'a Config, key: &str) -> &'a str {
    cfg.texts["steps"][key].as_str().unwrap_or("")
}

fn windows_dir() -> Result<PathBuf, SetupError> {
    let mut buf = [0u16; 260];
    let n = unsafe { GetWindowsDirectoryW(Some(&mut buf)) } as usize;
    if n == 0 || n >= buf.len() {
        return Err(SetupError::new("generic", "Windows folder not found"));
    }
    Ok(PathBuf::from(String::from_utf16_lossy(&buf[..n])))
}

fn system_dir() -> Result<PathBuf, SetupError> {
    let mut buf = [0u16; 260];
    let n = unsafe { GetSystemDirectoryW(Some(&mut buf)) } as usize;
    if n == 0 || n >= buf.len() {
        return Err(SetupError::new("generic", "System folder not found"));
    }
    Ok(PathBuf::from(String::from_utf16_lossy(&buf[..n])))
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

/// Shortcuts for everybody on this computer: (Start menu, desktop).
fn shortcut_paths(p: &Product) -> Result<(PathBuf, PathBuf), SetupError> {
    let file = format!("{}.lnk", p.name);
    Ok((
        known_folder(&FOLDERID_CommonPrograms)?.join(&file),
        known_folder(&FOLDERID_PublicDesktop)?.join(file),
    ))
}

fn write_registry(p: &Product, dir: &Path) -> Result<(), SetupError> {
    let (key, _) = RegKey::predef(HKEY_LOCAL_MACHINE)
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

fn run_tool(exe: &Path, args: &[&str]) -> Result<(), SetupError> {
    let status = std::process::Command::new(exe)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(SetupError::io)?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| SetupError::new("generic", format!("{} {} failed ({status})", exe.display(), args.join(" "))))
}

/// Registers the startup task (administrator, no prompt later) and runs it once now so a
/// first reading exists straight away.
fn register_boot_task(p: &Product, dir: &Path) -> Result<(), SetupError> {
    let schtasks = system_dir()?.join("schtasks.exe");
    let state = dir.join(STATE_DIR);
    std::fs::create_dir_all(&state).map_err(SetupError::io)?;
    let xml_path = state.join("boot-task.xml");
    // schtasks wants UTF-16 with a byte order mark.
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(boot_task_xml(&dir.join(&p.helper)).encode_utf16().flat_map(u16::to_le_bytes));
    std::fs::write(&xml_path, bytes).map_err(SetupError::io)?;
    let created = run_tool(&schtasks, &["/Create", "/TN", &p.task, "/XML", &xml_path.to_string_lossy(), "/F"]);
    let _ = std::fs::remove_file(&xml_path);
    created?;
    // A first reading now; the task itself keeps it fresh after every start.
    let _ = run_tool(&schtasks, &["/Run", "/TN", &p.task]);
    Ok(())
}

fn delete_boot_task(p: &Product) {
    if let Ok(dir) = system_dir() {
        let _ = run_tool(&dir.join("schtasks.exe"), &["/Delete", "/TN", &p.task, "/F"]);
    }
}

/// Removes an earlier per-user install (versions before the move to Program Files).
/// Best effort: anything in use or missing is left as it is.
fn migrate_user_install(p: &Product) {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey(uninstall_key(p)) else {
        return;
    };
    let old = std::env::var_os("LOCALAPPDATA").map(|b| PathBuf::from(b).join("Programs").join(&p.folder));
    let location: Option<String> = key.get_value("InstallLocation").ok();
    // Only the folder the old installer chose itself; never a path the registry merely names.
    if let (Some(old), Some(loc)) = (old, location) {
        if Path::new(&loc) == old && old.join(&p.exe).is_file() {
            let _ = std::fs::remove_dir_all(&old);
        }
    }
    if let Ok(programs) = known_folder(&FOLDERID_Programs) {
        let _ = std::fs::remove_file(programs.join(format!("{}.lnk", p.name)));
    }
    if let Ok(desktop) = known_folder(&FOLDERID_Desktop) {
        let _ = std::fs::remove_file(desktop.join(format!("{}.lnk", p.name)));
    }
    let _ = hkcu.delete_subkey_all(uninstall_key(p));
}

/// Installs or updates: the two program files, a copy of this installer as the uninstaller,
/// the startup task, shortcuts for all users (desktop optional) and the Apps & features entry.
pub fn install(cfg: &Config, desktop: bool, progress: &dyn Fn(u8, &str)) -> Result<(), SetupError> {
    let p = &cfg.product;
    let dir = install_dir(p)?;
    progress(5, step(cfg, "prepare"));
    std::fs::create_dir_all(&dir).map_err(SetupError::io)?;

    progress(10, step(cfg, "migrate"));
    migrate_user_install(p);

    progress(20, step(cfg, "app"));
    write_replace(&dir.join(&p.exe), PAYLOAD_APP)?;
    progress(42, step(cfg, "helper"));
    write_replace(&dir.join(&p.helper), PAYLOAD_HELPER)?;

    progress(58, step(cfg, "uninstaller"));
    let me = std::env::current_exe().map_err(SetupError::io)?;
    let dest = dir.join(UNINSTALLER);
    if me != dest {
        write_replace(&dest, &std::fs::read(&me).map_err(SetupError::io)?)?;
    }

    progress(70, step(cfg, "task"));
    register_boot_task(p, &dir)?;

    progress(84, step(cfg, "shortcuts"));
    let (start_menu, on_desktop) = shortcut_paths(p)?;
    let target = dir.join(&p.exe);
    create_shortcut(&start_menu, &target, &p.description)?;
    if desktop {
        create_shortcut(&on_desktop, &target, &p.description)?;
    }

    progress(94, step(cfg, "register"));
    write_registry(p, &dir)?;
    progress(100, "");
    Ok(())
}

/// Removes the program, its task, shortcuts and entry. The folder itself (which still holds
/// this running copy) is deleted by a short-lived helper process once this one has exited.
pub fn uninstall(cfg: &Config, progress: &dyn Fn(u8, &str)) -> Result<(), SetupError> {
    let p = &cfg.product;
    let me = std::env::current_exe().map_err(SetupError::io)?;
    let dir = uninstall_dir(&me, p).ok_or_else(|| SetupError::new("generic", "not an installed copy"))?;

    progress(10, step(cfg, "remove_task"));
    delete_boot_task(p);

    progress(30, step(cfg, "remove_files"));
    remove_waiting(&dir.join(&p.exe))?;
    remove_waiting(&dir.join(&p.helper))?;

    progress(60, step(cfg, "remove_shortcuts"));
    let (start_menu, on_desktop) = shortcut_paths(p)?;
    for lnk in [start_menu, on_desktop] {
        let _ = std::fs::remove_file(lnk);
    }

    progress(80, step(cfg, "remove_register"));
    let _ = RegKey::predef(HKEY_LOCAL_MACHINE).delete_subkey_all(uninstall_key(p));

    // `dir` was checked by `uninstall_dir` (our own folder name, our own file next to it).
    let cmd = system_dir()?.join("cmd.exe");
    std::process::Command::new(cmd)
        .raw_arg(format!("/C ping -n 3 127.0.0.1 >nul & rmdir /S /Q \"{}\"", dir.display()))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(SetupError::io)?;
    progress(100, "");
    Ok(())
}

/// Starts the program as the signed-in user. This installer is elevated, and a program started
/// directly from it would inherit that; going through Explorer starts it at normal level.
pub fn launch(p: &Product) -> Result<(), SetupError> {
    let exe = install_dir(p)?.join(&p.exe);
    std::process::Command::new(windows_dir()?.join("explorer.exe"))
        .arg(exe)
        .spawn()
        .map(|_| ())
        .map_err(SetupError::io)
}
