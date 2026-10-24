//! Real Windows implementation of [`SystemProbe`].

use super::{
    DefenderState, DeviceProblem, DiskInfo, Impact, IntegrityResult, ProbeError, ProcInfo,
    ServiceState, StartupItem, SystemProbe, UacLevel,
};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_DevNode_Registry_PropertyW, CM_Get_DevNode_Status, CM_Get_Device_ID_ListW,
    CM_Get_Device_ID_List_SizeW, CM_Locate_DevNodeW, CM_DEVNODE_STATUS_FLAGS, CM_DRP_DEVICEDESC,
    CM_DRP_FRIENDLYNAME, CM_GETIDLIST_FILTER_PRESENT, CM_LOCATE_DEVNODE_NORMAL, CM_PROB,
    CR_BUFFER_SMALL, CR_SUCCESS,
};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SERVICE_DOES_NOT_EXIST, ERROR_SUCCESS,
    VARIANT_FALSE,
};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::EventLog::{
    EvtClose, EvtNext, EvtQuery, EvtQueryChannelPath, EvtQueryReverseDirection, EvtRender,
    EvtRenderEventXml, EVT_HANDLE,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumValueW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_SAM_FLAGS,
    RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
};
use windows::Win32::System::Services::{
    CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceConfigW, QueryServiceStatus,
    QUERY_SERVICE_CONFIGW, SC_HANDLE, SC_MANAGER_CONNECT, SERVICE_DISABLED, SERVICE_QUERY_CONFIG,
    SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_STATUS,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::System::UpdateAgent::{orcSucceeded, IUpdateSession, UpdateSession};
use windows::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};

const DRIVE_UNKNOWN: u32 = 0;
const DRIVE_NO_ROOT_DIR: u32 = 1;
const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_CDROM: u32 = 5;
const DRIVE_REMOTE: u32 = 4;

/// Pairs `CoInitializeEx` with `CoUninitialize`. Declare it first in a function so
/// it drops after every COM object created later. S_OK and S_FALSE both add a
/// reference that must be released; RPC_E_CHANGED_MODE means COM is already
/// usable in another apartment and must not be uninitialized.
pub struct ComGuard(bool);

impl ComGuard {
    #[allow(clippy::new_without_default)] // initializing COM is a side effect
    pub fn new() -> Self {
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        ComGuard(hr.is_ok())
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

fn unavailable(e: windows::core::Error) -> ProbeError {
    ProbeError::Unavailable(e.to_string())
}

/// Newest successful install in the WUA history (local, no network).
fn wua_last_success() -> Result<Option<SystemTime>, ProbeError> {
    let _com = ComGuard::new();
    let session: IUpdateSession =
        unsafe { CoCreateInstance(&UpdateSession, None, CLSCTX_INPROC_SERVER) }
            .map_err(unavailable)?;
    let searcher = unsafe { session.CreateUpdateSearcher() }.map_err(unavailable)?;
    let history = unsafe { searcher.QueryHistory(0, 200) }.map_err(unavailable)?;
    let count = unsafe { history.Count() }.map_err(unavailable)?;
    let mut newest: Option<SystemTime> = None;
    for i in 0..count {
        let entry = unsafe { history.get_Item(i) }.map_err(unavailable)?;
        if unsafe { entry.ResultCode() }.map_err(unavailable)? != orcSucceeded {
            continue;
        }
        // OLE automation date: days since 1899-12-30 (25569 days before the Unix epoch).
        let date = unsafe { entry.Date() }.map_err(unavailable)?;
        let secs = (date - 25569.0) * 86_400.0;
        if secs.is_finite() && secs >= 0.0 {
            let t = UNIX_EPOCH + Duration::from_secs(secs as u64);
            newest = Some(newest.map_or(t, |n| n.max(t)));
        }
    }
    Ok(newest)
}

/// `Ok(None)` when the key or value does not exist.
fn reg_dword(root: HKEY, subkey: &str, value: &str) -> Result<Option<u32>, ProbeError> {
    let (sub, val) = (HSTRING::from(subkey), HSTRING::from(value));
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let rc = unsafe {
        RegGetValueW(
            root,
            PCWSTR(sub.as_ptr()),
            PCWSTR(val.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    if rc == ERROR_SUCCESS {
        Ok(Some(data))
    } else if rc == ERROR_FILE_NOT_FOUND {
        Ok(None)
    } else {
        Err(ProbeError::Unavailable(format!(
            "registry {subkey}\\{value}: {rc:?}"
        )))
    }
}

fn uac_from_values(enable_lua: u32, consent_admin: Option<u32>) -> UacLevel {
    if enable_lua == 0 {
        UacLevel::Disabled
    } else if consent_admin == Some(0) {
        UacLevel::NeverNotify
    } else {
        UacLevel::Enabled
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct MpComputerStatus {
    real_time_protection_enabled: bool,
    antivirus_signature_age: u32,
}

/// `(real-time protection on, signature age in days)` from Defender's WMI provider.
fn wmi_defender() -> Result<(bool, u32), ProbeError> {
    let wmi_err = |e: wmi::WMIError| ProbeError::Unavailable(format!("wmi defender: {e}"));
    let con = wmi::WMIConnection::with_namespace_path("ROOT\\Microsoft\\Windows\\Defender")
        .map_err(wmi_err)?;
    let rows: Vec<MpComputerStatus> = con
        .raw_query(
            "SELECT RealTimeProtectionEnabled, AntivirusSignatureAge FROM MSFT_MpComputerStatus",
        )
        .map_err(wmi_err)?;
    let s = rows
        .first()
        .ok_or_else(|| ProbeError::Unavailable("MSFT_MpComputerStatus empty".into()))?;
    Ok((s.real_time_protection_enabled, s.antivirus_signature_age))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AvProduct {
    instance_guid: String,
    product_state: u32,
}

/// Windows Defender's fixed product GUID in Security Center.
const DEFENDER_GUID: &str = "{D68DDC3A-831F-4FAE-9E44-DA132C1ACF46}";

/// SecurityCenter2 `productState`: bits 12..15 are the product state; 1 = on.
fn av_state_on(product_state: u32) -> bool {
    (product_state >> 12) & 0xF == 1
}

const WMI_TIMEOUT: Duration = Duration::from_secs(10);

/// Runs `f` on a helper thread and waits at most `dur`. On timeout the thread is
/// leaked (it cannot be cancelled) and keeps running until its call returns.
pub fn run_with_timeout<T: Send + 'static>(
    dur: Duration,
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, ProbeError> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(dur)
        .map_err(|_| ProbeError::Unavailable("WMI timed out".into()))
}

/// True when a non-Defender antivirus is registered and switched on.
fn third_party_av_active() -> Result<bool, ProbeError> {
    let wmi_err = |e: wmi::WMIError| ProbeError::Unavailable(format!("wmi securitycenter: {e}"));
    let con = wmi::WMIConnection::with_namespace_path("ROOT\\SecurityCenter2").map_err(wmi_err)?;
    let rows: Vec<AvProduct> = con
        .raw_query("SELECT instanceGuid, productState FROM AntiVirusProduct")
        .map_err(wmi_err)?;
    Ok(rows.iter().any(|r| {
        !r.instance_guid.eq_ignore_ascii_case(DEFENDER_GUID) && av_state_on(r.product_state)
    }))
}

const CM_PROB_DISABLED: u32 = 22;
const CM_PROB_PHANTOM: u32 = 45;
pub const INTEGRITY_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// Whether a device's problem code means something the user should look at.
/// 0 = fine; CM_PROB_DISABLED (22) = the user switched the device off on
/// purpose; CM_PROB_PHANTOM (45) = not present. Neither is reported.
fn is_reportable_problem(code: u32) -> bool {
    code != 0 && code != CM_PROB_DISABLED && code != CM_PROB_PHANTOM
}

/// Present devices with a reportable Configuration Manager problem code.
fn cm_problem_devices() -> Result<Vec<DeviceProblem>, ProbeError> {
    let cm_err = |what: &str, cr: u32| ProbeError::Unavailable(format!("{what}: CR {cr:#x}"));
    let mut len = 0u32;
    let cr = unsafe {
        CM_Get_Device_ID_List_SizeW(&mut len, PCWSTR::null(), CM_GETIDLIST_FILTER_PRESENT)
    };
    if cr != CR_SUCCESS {
        return Err(cm_err("device id list size", cr.0));
    }
    // The list can grow between the size query and the read: retry once.
    let mut buf;
    let mut attempt = 0;
    loop {
        buf = vec![0u16; len as usize];
        let cr = unsafe {
            CM_Get_Device_ID_ListW(PCWSTR::null(), &mut buf, CM_GETIDLIST_FILTER_PRESENT)
        };
        if cr == CR_SUCCESS {
            break;
        }
        if cr == CR_BUFFER_SMALL && attempt == 0 {
            attempt += 1;
            let cr = unsafe {
                CM_Get_Device_ID_List_SizeW(&mut len, PCWSTR::null(), CM_GETIDLIST_FILTER_PRESENT)
            };
            if cr != CR_SUCCESS {
                return Err(cm_err("device id list size", cr.0));
            }
            continue;
        }
        return Err(cm_err("device id list", cr.0));
    }
    let mut out = Vec::new();
    // Multi-string: ids separated by NUL, ending with an empty string.
    for id in buf.split(|&c| c == 0).filter(|s| !s.is_empty()) {
        let mut wide: Vec<u16> = id.to_vec();
        wide.push(0);
        let mut inst = 0u32;
        let cr = unsafe {
            CM_Locate_DevNodeW(&mut inst, PCWSTR(wide.as_ptr()), CM_LOCATE_DEVNODE_NORMAL)
        };
        if cr != CR_SUCCESS {
            continue; // the device vanished while enumerating
        }
        let mut status = CM_DEVNODE_STATUS_FLAGS(0);
        let mut problem = CM_PROB(0);
        let cr = unsafe { CM_Get_DevNode_Status(&mut status, &mut problem, inst, 0) };
        if cr != CR_SUCCESS || !is_reportable_problem(problem.0) {
            continue;
        }
        let name = cm_device_name(inst).unwrap_or_else(|| String::from_utf16_lossy(id));
        out.push(DeviceProblem {
            name,
            code: problem.0,
        });
    }
    Ok(out)
}

/// Friendly name, else device description, of a devnode.
fn cm_device_name(inst: u32) -> Option<String> {
    for prop in [CM_DRP_FRIENDLYNAME, CM_DRP_DEVICEDESC] {
        let mut buf = vec![0u16; 512];
        let mut bytes = (buf.len() * 2) as u32;
        let cr = unsafe {
            CM_Get_DevNode_Registry_PropertyW(
                inst,
                prop,
                None,
                Some(buf.as_mut_ptr() as *mut _),
                &mut bytes,
                0,
            )
        };
        if cr == CR_SUCCESS {
            let n = (bytes as usize / 2).min(buf.len());
            let s = String::from_utf16_lossy(&buf[..n]);
            let s = s.trim_end_matches('\0').trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

/// Closes an event log handle on drop.
struct EvtGuard(EVT_HANDLE);
impl Drop for EvtGuard {
    fn drop(&mut self) {
        let _ = unsafe { EvtClose(self.0) };
    }
}

/// `BootTime` (ms) from the XML of a Diagnostics-Performance event 100, as seconds.
fn parse_boot_time_secs(xml: &str) -> Option<u32> {
    let i = xml
        .find("Name='BootTime'")
        .or_else(|| xml.find("Name=\"BootTime\""))?;
    let rest = &xml[i..];
    let start = rest.find('>')? + 1;
    let end = rest[start..].find('<')? + start;
    let ms: u64 = rest[start..end].trim().parse().ok()?;
    Some((ms / 1000).min(u32::MAX as u64) as u32)
}

/// Win32 error number of a raw error value: unwrapped only when it is an
/// HRESULT in the Win32 facility (0x8007xxxx) or already fits in 16 bits;
/// any other HRESULT is returned unchanged so its low bits cannot collide.
fn win32_code(raw: u32) -> u32 {
    const FACILITY_WIN32: u32 = 7;
    if (raw >> 16) & 0x1FFF == FACILITY_WIN32 && raw & 0x8000_0000 != 0 {
        raw & 0xFFFF
    } else {
        raw
    }
}

/// Maps a failed `EvtQuery`/`EvtNext` Win32 error code. Access denied (5: the Operational
/// log is not readable without administrator rights) and channel-not-found
/// (15007) both mean "no readable boot event" -> `Ok(None)` (shown as Unknown,
/// never as healthy). Everything else is a real failure.
fn map_evt_error(raw: u32) -> Result<Option<u32>, ProbeError> {
    const ERROR_ACCESS_DENIED: u32 = 5;
    const ERROR_EVT_CHANNEL_NOT_FOUND: u32 = 15007;
    let code = win32_code(raw);
    match code {
        ERROR_ACCESS_DENIED | ERROR_EVT_CHANNEL_NOT_FOUND => Ok(None),
        c => Err(ProbeError::Unavailable(format!("event log error {c}"))),
    }
}

/// Newest boot duration (Event ID 100) in seconds; `None` when there is none.
fn boot_seconds_from_event_log() -> Result<Option<u32>, ProbeError> {
    const ERROR_NO_MORE_ITEMS: u32 = 259;
    let raw = |e: &windows::core::Error| e.code().0 as u32;
    let channel = HSTRING::from("Microsoft-Windows-Diagnostics-Performance/Operational");
    let query = HSTRING::from("*[System[(EventID=100)]]");
    let rs = match unsafe {
        EvtQuery(
            None,
            PCWSTR(channel.as_ptr()),
            PCWSTR(query.as_ptr()),
            EvtQueryChannelPath.0 | EvtQueryReverseDirection.0,
        )
    } {
        Ok(h) => EvtGuard(h),
        Err(e) => return map_evt_error(raw(&e)),
    };
    let mut events = [0isize; 1];
    let mut returned = 0u32;
    if let Err(e) = unsafe { EvtNext(rs.0, &mut events, 5000, 0, &mut returned) } {
        if win32_code(raw(&e)) == ERROR_NO_MORE_ITEMS {
            return Ok(None);
        }
        return map_evt_error(raw(&e));
    }
    if returned == 0 {
        return Ok(None);
    }
    let ev = EvtGuard(EVT_HANDLE(events[0]));
    let (mut used, mut props) = (0u32, 0u32);
    // First call reports the needed buffer size (bytes) via ERROR_INSUFFICIENT_BUFFER.
    let _ = unsafe {
        EvtRender(
            None,
            ev.0,
            EvtRenderEventXml.0,
            0,
            None,
            &mut used,
            &mut props,
        )
    };
    if used == 0 {
        return Err(ProbeError::Unavailable("event render size".into()));
    }
    let mut buf = vec![0u16; (used as usize).div_ceil(2)];
    unsafe {
        EvtRender(
            None,
            ev.0,
            EvtRenderEventXml.0,
            (buf.len() * 2) as u32,
            Some(buf.as_mut_ptr() as *mut _),
            &mut used,
            &mut props,
        )
    }
    .map_err(unavailable)?;
    let xml = String::from_utf16_lossy(&buf);
    Ok(parse_boot_time_secs(xml.trim_end_matches('\0')))
}

/// True when the current process token is elevated (administrator).
pub fn is_elevated() -> Result<bool, ProbeError> {
    struct Tok(HANDLE);
    impl Drop for Tok {
        fn drop(&mut self) {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
    let mut h = HANDLE::default();
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut h) }.map_err(unavailable)?;
    let tok = Tok(h);
    let mut elev = TOKEN_ELEVATION::default();
    let mut ret = 0u32;
    unsafe {
        GetTokenInformation(
            tok.0,
            TokenElevation,
            Some(&mut elev as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret,
        )
    }
    .map_err(unavailable)?;
    Ok(elev.TokenIsElevated != 0)
}

/// Reads a directory from a `GetSystem*DirectoryW`-style API (`len` = UTF-16
/// units written, 0 on failure).
fn api_dir(read: impl Fn(&mut [u16]) -> u32, fallback: &str) -> PathBuf {
    let mut buf = vec![0u16; 520];
    let n = read(&mut buf) as usize;
    if n == 0 || n >= buf.len() {
        return PathBuf::from(fallback);
    }
    PathBuf::from(String::from_utf16_lossy(&buf[..n]))
}

/// `System32` from `GetSystemDirectoryW`. Environment variables are NOT
/// consulted: the elevated helper must not be redirectable through `%SystemRoot%`.
pub fn system32_dir() -> PathBuf {
    use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
    api_dir(
        |b| unsafe { GetSystemDirectoryW(Some(b)) },
        r"C:\Windows\System32",
    )
}

/// The Windows directory from `GetSystemWindowsDirectoryW` (not `%SystemRoot%`).
pub fn windows_dir() -> PathBuf {
    use windows::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
    api_dir(
        |b| unsafe { GetSystemWindowsDirectoryW(Some(b)) },
        r"C:\Windows",
    )
}

/// Why [`run_process`] produced no exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    /// The program could not be started.
    Spawn(String),
    /// It exceeded the timeout and was killed.
    Timeout,
    /// It ended without an exit code, or waiting for it failed.
    Wait(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Spawn(e) => write!(f, "could not start: {e}"),
            RunError::Timeout => f.write_str("timed out"),
            RunError::Wait(e) => write!(f, "wait failed: {e}"),
        }
    }
}

/// Runs `program` (an absolute path) with a fixed argument array: no shell, no
/// window, stdin/stdout/stderr to null, working directory System32 and a
/// minimal environment built from API-resolved directories only (the caller's
/// environment, e.g. a user-planted `PATH` or `PSModulePath`, is not
/// inherited). Kills the child when it exceeds `timeout`.
pub fn run_process(program: &Path, args: &[&str], timeout: Duration) -> Result<i32, RunError> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let (sys, win) = (system32_dir(), windows_dir());
    let drive: String = win.to_string_lossy().chars().take(2).collect();
    let path = format!(
        "{};{};{}",
        sys.display(),
        win.display(),
        sys.join("Wbem").display()
    );
    let mut child = Command::new(program)
        .args(args)
        .env_clear()
        .env("SystemRoot", &win)
        .env("windir", &win)
        .env("SystemDrive", drive)
        .env("TEMP", win.join("Temp"))
        .env("TMP", win.join("Temp"))
        .env("PATH", path)
        .current_dir(&sys)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| RunError::Spawn(format!("{}: {e}", program.display())))?;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return status
                    .code()
                    .ok_or_else(|| RunError::Wait("process ended without exit code".into()))
            }
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::Timeout);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::Wait(e.to_string()));
            }
        }
    }
}

/// [`run_process`] with probe errors: a timeout is `Unavailable`.
fn run_exit_code(program: &Path, args: &[&str], timeout: Duration) -> Result<i32, ProbeError> {
    run_process(program, args, timeout).map_err(|e| match e {
        RunError::Spawn(_) | RunError::Timeout => {
            ProbeError::Unavailable(format!("{}: {e}", program.display()))
        }
        RunError::Wait(m) => ProbeError::Other(m),
    })
}

/// Interprets SFC (`/verifyonly`) and DISM (`/ScanHealth`) exit codes; localized
/// output is never parsed.
/// - SFC 1 = corruption found. SFC exit 1 can also mean the scan could not run
///   (pending repair/reboot); that rare false-Critical is accepted because
///   parsing localized output is forbidden.
/// - Clean = SFC found no integrity violations and DISM ran without failing;
///   DISM exit 0 does not prove the component store is healthy (`/ScanHealth`
///   exits 0 even for a repairable store).
/// - Anything else cannot be trusted and is an error, never "clean".
fn interpret_integrity(sfc: i32, dism: i32) -> Result<IntegrityResult, ProbeError> {
    match (sfc, dism) {
        (1, _) => Ok(IntegrityResult::CorruptionFound),
        (0, 0) => Ok(IntegrityResult::Clean),
        _ => Err(ProbeError::Other(format!(
            "integrity scan inconclusive (sfc={sfc}, dism={dism})"
        ))),
    }
}

/// Runs SFC, then DISM only when SFC did not already find corruption (DISM takes
/// minutes and its code is ignored in that case).
fn run_integrity(
    sfc: impl FnOnce() -> Result<i32, ProbeError>,
    dism: impl FnOnce() -> Result<i32, ProbeError>,
) -> Result<IntegrityResult, ProbeError> {
    match sfc()? {
        1 => Ok(IntegrityResult::CorruptionFound),
        0 => interpret_integrity(0, dism()?),
        // Unknown SFC code: inconclusive, do not spend minutes on DISM.
        other => interpret_integrity(other, 0),
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsProbe;

impl WindowsProbe {
    pub fn new() -> Self {
        Self
    }
}

/// Recursive size of a directory; unreadable entries are ignored. Symlinks
/// are not followed. A missing directory counts as 0.
fn dir_size(path: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in rd.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            total = total.saturating_add(dir_size(&entry.path()));
        } else if let Ok(md) = entry.metadata() {
            total = total.saturating_add(md.len());
        }
    }
    total
}

/// Values under a registry key as `(name, first data byte)`; a missing key yields
/// an empty list.
fn registry_values(root: HKEY, subkey: &str, view: REG_SAM_FLAGS) -> Vec<(String, Option<u8>)> {
    let sub = HSTRING::from(subkey);
    let mut key = HKEY::default();
    if unsafe { RegOpenKeyExW(root, PCWSTR(sub.as_ptr()), 0, KEY_READ | view, &mut key) }
        != ERROR_SUCCESS
    {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut buf = vec![0u16; 16384];
    let mut data = [0u8; 16];
    for index in 0u32.. {
        let mut len = buf.len() as u32;
        let mut dlen = data.len() as u32;
        let rc = unsafe {
            RegEnumValueW(
                key,
                index,
                windows::core::PWSTR(buf.as_mut_ptr()),
                &mut len,
                None,
                None,
                Some(data.as_mut_ptr()),
                Some(&mut dlen),
            )
        };
        if rc == ERROR_MORE_DATA {
            // Data longer than our small buffer: the name and first byte are still valid.
            out.push((
                String::from_utf16_lossy(&buf[..len as usize]),
                Some(data[0]),
            ));
            continue;
        }
        if rc != ERROR_SUCCESS {
            break;
        }
        let first = (dlen > 0).then_some(data[0]);
        out.push((String::from_utf16_lossy(&buf[..len as usize]), first));
    }
    let _ = unsafe { RegCloseKey(key) };
    out
}

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_KEY_32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
/// StartupApproved key for 64-bit (and all HKCU) Run entries.
pub const STARTUP_APPROVED_RUN: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
/// StartupApproved key for HKLM 32-bit Run entries.
pub const STARTUP_APPROVED_RUN32: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32";

/// Every registry Run value (enabled and disabled), with the StartupApproved
/// key that controls it. HKLM is read in the 64-bit view and the 32-bit
/// (WOW6432Node) view; HKCU `Software` is shared between views, so it is read
/// once (reading it again "as 32-bit" would pair the same values with `Run32`
/// and make a disabled item look enabled). StartupApproved is always read in
/// the 64-bit view, where Explorer and Task Manager keep it.
pub fn run_entries() -> Vec<super::RunEntry> {
    let mut out = Vec::new();
    for (root, hive) in [(HKEY_CURRENT_USER, "HKCU"), (HKEY_LOCAL_MACHINE, "HKLM")] {
        // StartupApproved: first byte even (0x02, 0x06) = enabled, odd = disabled;
        // a missing value means enabled.
        let approved = registry_values(root, STARTUP_APPROVED_RUN, KEY_WOW64_64KEY);
        let approved_32 = registry_values(root, STARTUP_APPROVED_RUN32, KEY_WOW64_64KEY);
        let views: &[(&str, REG_SAM_FLAGS, &'static str)] = if hive == "HKCU" {
            &[(RUN_KEY, REG_SAM_FLAGS(0), STARTUP_APPROVED_RUN)]
        } else {
            // HKLM 32-bit programs live in the WOW6432Node view; read it explicitly
            // with KEY_WOW64_32KEY so it works regardless of process bitness.
            &[
                (RUN_KEY, KEY_WOW64_64KEY, STARTUP_APPROVED_RUN),
                (RUN_KEY, KEY_WOW64_32KEY, STARTUP_APPROVED_RUN32),
                (RUN_KEY_32, KEY_WOW64_64KEY, STARTUP_APPROVED_RUN32),
            ]
        };
        for &(key, view, approved_key) in views {
            let appr = if approved_key == STARTUP_APPROVED_RUN {
                &approved
            } else {
                &approved_32
            };
            for (name, _) in registry_values(root, key, view) {
                let disabled = appr
                    .iter()
                    .any(|(n, b)| n.eq_ignore_ascii_case(&name) && b.is_some_and(|b| b & 1 == 1));
                let entry = super::RunEntry {
                    hive,
                    name,
                    approved_key,
                    disabled,
                };
                if !out.contains(&entry) {
                    out.push(entry);
                }
            }
        }
    }
    out
}

impl SystemProbe for WindowsProbe {
    fn disks(&self) -> Result<Vec<DiskInfo>, ProbeError> {
        let mask = unsafe { GetLogicalDrives() };
        if mask == 0 {
            return Err(ProbeError::Unavailable("GetLogicalDrives failed".into()));
        }
        let mut out = Vec::new();
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let mount = format!("{}:", (b'A' + i as u8) as char);
            let root = HSTRING::from(format!("{mount}\\"));
            let kind = unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) };
            // A disconnected network share can block GetDiskFreeSpaceExW, so skip remote drives.
            if kind == DRIVE_UNKNOWN || kind == DRIVE_NO_ROOT_DIR || kind == DRIVE_REMOTE {
                continue;
            }
            let (mut free, mut total) = (0u64, 0u64);
            let ok = unsafe {
                GetDiskFreeSpaceExW(
                    PCWSTR(root.as_ptr()),
                    Some(&mut free),
                    Some(&mut total),
                    None,
                )
            };
            if ok.is_err() {
                // Empty card reader or unavailable volume: skip.
                continue;
            }
            out.push(DiskInfo {
                mount,
                total_bytes: total,
                free_bytes: free,
                removable: kind == DRIVE_REMOVABLE || kind == DRIVE_CDROM,
            });
        }
        Ok(out)
    }

    fn temp_bytes(&self) -> Result<u64, ProbeError> {
        Ok(dir_size(&std::env::temp_dir()))
    }

    fn recycle_bin_bytes(&self) -> Result<u64, ProbeError> {
        let mut info = SHQUERYRBINFO {
            cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
            ..Default::default()
        };
        // Null root path queries the recycle bins of all drives.
        unsafe { SHQueryRecycleBinW(PCWSTR::null(), &mut info) }
            .map_err(|e| ProbeError::Other(format!("SHQueryRecycleBinW: {e}")))?;
        Ok(info.i64Size.max(0) as u64)
    }

    fn update_leftovers_bytes(&self) -> Result<u64, ProbeError> {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        let p: PathBuf = PathBuf::from(root)
            .join("SoftwareDistribution")
            .join("Download");
        Ok(dir_size(&p))
    }

    fn cpu_percent(&self) -> Result<f32, ProbeError> {
        // CPU usage needs two samples; the first refresh only sets a baseline.
        let mut sys = sysinfo::System::new();
        sys.refresh_cpu_usage();
        std::thread::sleep(std::time::Duration::from_millis(500));
        sys.refresh_cpu_usage();
        Ok(sys.global_cpu_usage())
    }

    fn ram_usage(&self) -> Result<(u64, u64), ProbeError> {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        Ok((sys.used_memory(), sys.total_memory()))
    }

    fn top_processes(&self, n: usize) -> Result<Vec<ProcInfo>, ProbeError> {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        std::thread::sleep(std::time::Duration::from_millis(500));
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let mut v: Vec<ProcInfo> = sys
            .processes()
            .values()
            // pid 0 is the System Idle Process, not a real process.
            .filter(|p| p.pid().as_u32() != 0)
            .map(|p| ProcInfo {
                pid: p.pid().as_u32(),
                name: p.name().to_string_lossy().into_owned(),
                cpu: p.cpu_usage(),
                ram_bytes: p.memory(),
            })
            .collect();
        v.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.ram_bytes.cmp(&a.ram_bytes)));
        v.truncate(n);
        Ok(v)
    }

    fn startup_items(&self) -> Result<Vec<StartupItem>, ProbeError> {
        let mut out: Vec<StartupItem> = Vec::new();
        for e in run_entries() {
            let id = e.id();
            if e.disabled || out.iter().any(|i| i.id == id) {
                continue;
            }
            out.push(StartupItem {
                id,
                // No cheap reliable impact measure exists (it would need boot
                // trace data), so every registry Run entry is rated Medium.
                impact: Impact::Medium,
                name: e.name,
            });
        }
        Ok(out)
    }

    /// Pending updates via the Windows Update Agent COM API (`IUpdateSearcher`)
    /// with `Online = false`: it only consults the local cache, so it needs no
    /// network and no admin rights. If the cache cannot answer (never scanned,
    /// COM failure) the result is `Unavailable`, never "0 pending".
    fn pending_updates(&self) -> Result<u32, ProbeError> {
        let _com = ComGuard::new(); // dropped last, after the COM objects below
        let session: IUpdateSession =
            unsafe { CoCreateInstance(&UpdateSession, None, CLSCTX_INPROC_SERVER) }
                .map_err(unavailable)?;
        let searcher = unsafe { session.CreateUpdateSearcher() }.map_err(unavailable)?;
        unsafe { searcher.SetOnline(VARIANT_FALSE) }.map_err(unavailable)?;
        let result = unsafe { searcher.Search(&"IsInstalled=0 and IsHidden=0".into()) }
            .map_err(unavailable)?;
        let count = unsafe { result.Updates() }
            .and_then(|u| unsafe { u.Count() })
            .map_err(unavailable)?;
        Ok(count.max(0) as u32)
    }

    fn service_state(&self, name: &str) -> Result<ServiceState, ProbeError> {
        /// Closes the handle on every path.
        struct Handle(SC_HANDLE);
        impl Drop for Handle {
            fn drop(&mut self) {
                let _ = unsafe { CloseServiceHandle(self.0) };
            }
        }
        let err =
            |what: &str, e: windows::core::Error| ProbeError::Unavailable(format!("{what}: {e}"));
        let scm = Handle(
            unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) }
                .map_err(|e| err("OpenSCManager", e))?,
        );
        let wide = HSTRING::from(name);
        let svc = match unsafe {
            OpenServiceW(
                scm.0,
                PCWSTR(wide.as_ptr()),
                SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG,
            )
        } {
            Ok(h) => Handle(h),
            Err(e) if e.code() == ERROR_SERVICE_DOES_NOT_EXIST.to_hresult() => {
                return Ok(ServiceState::Missing)
            }
            Err(e) => return Err(err("OpenService", e)),
        };
        // Start type: first call sizes the buffer, second fills it.
        let mut needed = 0u32;
        let _ = unsafe { QueryServiceConfigW(svc.0, None, 0, &mut needed) };
        if needed == 0 {
            return Err(ProbeError::Unavailable("QueryServiceConfig".into()));
        }
        // u64 buffer keeps the QUERY_SERVICE_CONFIGW header aligned.
        let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
        let cfg = buf.as_mut_ptr() as *mut QUERY_SERVICE_CONFIGW;
        unsafe { QueryServiceConfigW(svc.0, Some(cfg), needed, &mut needed) }
            .map_err(|e| err("QueryServiceConfig", e))?;
        if unsafe { (*cfg).dwStartType } == SERVICE_DISABLED {
            return Ok(ServiceState::Disabled);
        }
        let mut status = SERVICE_STATUS::default();
        unsafe { QueryServiceStatus(svc.0, &mut status) }
            .map_err(|e| err("QueryServiceStatus", e))?;
        Ok(if status.dwCurrentState == SERVICE_RUNNING {
            ServiceState::Running
        } else {
            ServiceState::Stopped
        })
    }

    fn defender(&self) -> Result<DefenderState, ProbeError> {
        // WMI can hang (infinite enumerator timeout), so run all of it off-thread.
        run_with_timeout(WMI_TIMEOUT, || {
            let _com = ComGuard::new(); // COM is per-thread: init inside this thread
            match wmi_defender() {
                Ok((realtime, signature_age_days)) => Ok(DefenderState {
                    realtime,
                    signature_age_days,
                    // Only relevant when Defender is off; `None` = query failed.
                    third_party_av: if realtime {
                        Some(false)
                    } else {
                        third_party_av_active().ok()
                    },
                }),
                // Defender's WMI provider is absent when another AV replaced it.
                Err(e) => match third_party_av_active() {
                    Ok(true) => Ok(DefenderState {
                        realtime: false,
                        signature_age_days: 0,
                        third_party_av: Some(true),
                    }),
                    _ => Err(e),
                },
            }
        })?
    }

    fn firewall_enabled(&self) -> Result<bool, ProbeError> {
        const BASE: &str =
            "SYSTEM\\CurrentControlSet\\Services\\SharedAccess\\Parameters\\FirewallPolicy";
        let mut read_any = false;
        for profile in ["StandardProfile", "PublicProfile", "DomainProfile"] {
            match reg_dword(
                HKEY_LOCAL_MACHINE,
                &format!("{BASE}\\{profile}"),
                "EnableFirewall",
            )? {
                Some(0) => return Ok(false),
                Some(_) => read_any = true,
                // Value absent: Windows treats a missing EnableFirewall as enabled.
                None => {}
            }
        }
        // Not a single profile could be read: say so rather than claim "enabled".
        if read_any {
            Ok(true)
        } else {
            Err(ProbeError::Unavailable("no firewall profile found".into()))
        }
    }

    fn uac_level(&self) -> Result<UacLevel, ProbeError> {
        const KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies\\System";
        let lua = reg_dword(HKEY_LOCAL_MACHINE, KEY, "EnableLUA")?
            .ok_or_else(|| ProbeError::Unavailable("EnableLUA not found".into()))?;
        // Absent ConsentPromptBehaviorAdmin means the default (prompt).
        let consent = reg_dword(HKEY_LOCAL_MACHINE, KEY, "ConsentPromptBehaviorAdmin")?;
        Ok(uac_from_values(lua, consent))
    }

    /// Last successful update. Order: registry `LastSuccessTime` (UTC string;
    /// often absent or stale on Windows 10/11), then the WUA update history.
    fn last_update_success(&self) -> Result<Option<SystemTime>, ProbeError> {
        choose_last_update(registry_last_success(), wua_last_success)
    }
    fn problem_devices(&self) -> Result<Vec<DeviceProblem>, ProbeError> {
        cm_problem_devices()
    }

    fn boot_seconds(&self) -> Result<Option<u32>, ProbeError> {
        boot_seconds_from_event_log()
    }

    /// Needs administrator (SFC/DISM refuse to run otherwise). The app starts
    /// unelevated, so until Tasks 10/11 route the deep integrity scan through the
    /// elevated helper this returns `Unavailable("needs administrator")` and the
    /// check shows an error card (never "healthy").
    fn integrity_scan(&self) -> Result<IntegrityResult, ProbeError> {
        if !is_elevated()? {
            return Err(ProbeError::Unavailable("needs administrator".into()));
        }
        let sys = system32_dir();
        run_integrity(
            || run_exit_code(&sys.join("sfc.exe"), &["/verifyonly"], INTEGRITY_TIMEOUT),
            || {
                run_exit_code(
                    &sys.join("dism.exe"),
                    &["/Online", "/Cleanup-Image", "/ScanHealth"],
                    INTEGRITY_TIMEOUT,
                )
            },
        )
    }
}

/// `LastSuccessTime` ("YYYY-MM-DD HH:MM:SS", UTC) from the Auto Update results.
/// Absent, unreadable or unparseable all yield `None` (the caller falls back).
fn registry_last_success() -> Option<SystemTime> {
    let key = HSTRING::from(
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\WindowsUpdate\\Auto Update\\Results\\Install",
    );
    let value = HSTRING::from("LastSuccessTime");
    let mut buf = [0u16; 64];
    let mut size = std::mem::size_of_val(&buf) as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut size),
        )
    };
    if rc != ERROR_SUCCESS {
        return None;
    }
    let chars = (size as usize / 2).min(buf.len());
    let text = String::from_utf16_lossy(&buf[..chars]);
    parse_utc_timestamp(text.trim_end_matches('\0'))
}

/// Source order for the last successful update: the registry value wins; if it
/// is absent or unparseable, fall back to the WUA history. `None` only when
/// both are empty; a failing history query is an error (Unknown), not "never".
fn choose_last_update(
    registry: Option<SystemTime>,
    history: impl FnOnce() -> Result<Option<SystemTime>, ProbeError>,
) -> Result<Option<SystemTime>, ProbeError> {
    match registry {
        Some(t) => Ok(Some(t)),
        None => history(),
    }
}

/// Parses "YYYY-MM-DD HH:MM:SS" (UTC). Unparseable text yields `None`.
fn parse_utc_timestamp(s: &str) -> Option<SystemTime> {
    let (date, time) = s.trim().split_once(' ')?;
    let mut d = date.split('-');
    let y: i64 = d.next()?.parse().ok()?;
    let m: i64 = d.next()?.parse().ok()?;
    let day: i64 = d.next()?.parse().ok()?;
    if d.next().is_some() {
        return None;
    }
    let mut t = time.split(':');
    let hh: u64 = t.next()?.parse().ok()?;
    let mm: u64 = t.next()?.parse().ok()?;
    let ss: u64 = t.next()?.parse().ok()?;
    if t.next().is_some() || hh > 23 || mm > 59 || ss > 60 || !(1..=12).contains(&m) {
        return None;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let dim = match m {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if !(1..=dim).contains(&day) {
        return None;
    }
    // Days from civil date (Hinnant's algorithm), all steps checked.
    let y = if m <= 2 { y.checked_sub(1)? } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era
        .checked_mul(146_097)?
        .checked_add(doe)?
        .checked_sub(719_468)?;
    let secs = u64::try_from(days)
        .ok()?
        .checked_mul(86_400)?
        .checked_add(hh * 3600 + mm * 60 + ss)?;
    UNIX_EPOCH.checked_add(Duration::from_secs(secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_probe_smoke() {
        let p = WindowsProbe::new();
        let disks = p.disks().unwrap();
        assert!(disks.iter().any(|d| d.total_bytes > 0));
        p.temp_bytes().unwrap();
        p.update_leftovers_bytes().unwrap();
    }

    #[test]
    fn real_probe_performance_smoke() {
        let p = WindowsProbe::new();
        assert!(p.ram_usage().unwrap().1 > 0);
        let cpu = p.cpu_percent().unwrap();
        assert!((0.0..=100.0).contains(&cpu));
        assert!(!p.top_processes(3).unwrap().is_empty());
        assert!(p.startup_items().is_ok());
    }

    #[test]
    fn parses_registry_timestamp() {
        assert_eq!(
            parse_utc_timestamp("2024-03-01 12:30:15"),
            Some(UNIX_EPOCH + Duration::from_secs(1_709_296_215))
        );
        assert_eq!(parse_utc_timestamp("garbage"), None);
    }

    fn ts(secs: u64) -> Option<SystemTime> {
        Some(UNIX_EPOCH + Duration::from_secs(secs))
    }

    #[test]
    fn timestamp_edge_cases() {
        assert_eq!(parse_utc_timestamp("1970-01-01 00:00:00"), ts(0));
        assert_eq!(
            parse_utc_timestamp("2024-02-29 00:00:00"),
            ts(1_709_164_800)
        );
        assert_eq!(parse_utc_timestamp("2023-02-29 00:00:00"), None);
        assert_eq!(parse_utc_timestamp("2024-02-31 00:00:00"), None);
        assert_eq!(parse_utc_timestamp("2024-04-31 00:00:00"), None);
        assert_eq!(
            parse_utc_timestamp("9223372036854775807-01-01 00:00:00"),
            None
        );
        assert_eq!(
            parse_utc_timestamp("-9223372036854775807-01-01 00:00:00"),
            None
        );
        assert_eq!(parse_utc_timestamp("1969-12-31 23:59:59"), None);
    }

    #[test]
    fn registry_value_wins_over_history() {
        let r = choose_last_update(ts(5), || panic!("history must not be queried"));
        assert_eq!(r, Ok(ts(5)));
    }

    #[test]
    fn missing_registry_falls_back_to_history() {
        assert_eq!(choose_last_update(None, || Ok(ts(9))), Ok(ts(9)));
    }

    #[test]
    fn both_empty_is_none() {
        assert_eq!(choose_last_update(None, || Ok(None)), Ok(None));
    }

    #[test]
    fn history_error_is_error() {
        let e = ProbeError::Unavailable("x".into());
        assert_eq!(choose_last_update(None, || Err(e.clone())), Err(e));
    }

    #[test]
    fn real_probe_update_smoke() {
        let p = WindowsProbe::new();
        assert!(p.service_state("wuauserv").is_ok());
        assert_eq!(
            p.service_state("definitely_not_a_service_xyz").unwrap(),
            ServiceState::Missing
        );
        match p.pending_updates() {
            Ok(_) | Err(ProbeError::Unavailable(_)) => {}
            Err(e) => panic!("unexpected: {e}"),
        }
        // None (never) and Some are both fine; it must not panic.
        let _ = p.last_update_success();
    }

    #[test]
    fn uac_and_av_decoding() {
        assert_eq!(uac_from_values(0, Some(5)), UacLevel::Disabled);
        assert_eq!(uac_from_values(1, Some(0)), UacLevel::NeverNotify);
        assert_eq!(uac_from_values(1, Some(5)), UacLevel::Enabled);
        assert_eq!(uac_from_values(1, None), UacLevel::Enabled);
        assert!(av_state_on(0x1000));
        assert!(av_state_on(0x11000));
        assert!(!av_state_on(0x0000));
        assert!(!av_state_on(0x2000)); // snoozed
    }

    #[test]
    fn real_probe_security_smoke() {
        let p = WindowsProbe::new();
        assert!(p.firewall_enabled().is_ok());
        assert!(p.uac_level().is_ok());
        match p.defender() {
            Ok(_) | Err(ProbeError::Unavailable(_)) => {}
            Err(e) => panic!("unexpected: {e}"),
        }
    }

    #[test]
    fn problem_code_filtering() {
        assert!(!is_reportable_problem(0));
        assert!(!is_reportable_problem(CM_PROB_DISABLED));
        assert!(!is_reportable_problem(CM_PROB_PHANTOM));
        assert!(is_reportable_problem(28)); // driver not installed
        assert!(is_reportable_problem(10)); // cannot start
    }

    #[test]
    fn evt_error_mapping() {
        assert_eq!(map_evt_error(5), Ok(None)); // access denied
        assert_eq!(map_evt_error(15007), Ok(None)); // channel not found
        assert!(map_evt_error(1223).is_err());
        assert!(map_evt_error(0).is_err());
        assert_eq!(map_evt_error(0x8007_0005), Ok(None)); // HRESULT_FROM_WIN32(5)
        assert_eq!(map_evt_error(0x8007_3A9F), Ok(None)); // HRESULT_FROM_WIN32(15007)
        assert!(map_evt_error(0x8004_0005).is_err()); // not the Win32 facility
        assert!(map_evt_error(0x8004_3A9F).is_err());
    }

    #[test]
    fn sfc_unknown_code_does_not_run_dism() {
        let calls = std::cell::Cell::new(0);
        let dism = || {
            calls.set(calls.get() + 1);
            Ok(0)
        };
        assert!(run_integrity(|| Ok(2), dism).is_err());
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn sfc_corruption_skips_dism() {
        let calls = std::cell::Cell::new(0);
        let dism = || {
            calls.set(calls.get() + 1);
            Ok(0)
        };
        assert_eq!(
            run_integrity(|| Ok(1), dism),
            Ok(IntegrityResult::CorruptionFound)
        );
        assert_eq!(calls.get(), 0);
        assert_eq!(run_integrity(|| Ok(0), dism), Ok(IntegrityResult::Clean));
        assert_eq!(calls.get(), 1);
        assert!(run_integrity(|| Err(ProbeError::Other("x".into())), dism).is_err());
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn integrity_exit_code_interpretation() {
        assert_eq!(interpret_integrity(0, 0), Ok(IntegrityResult::Clean));
        assert_eq!(
            interpret_integrity(1, 0),
            Ok(IntegrityResult::CorruptionFound)
        );
        assert_eq!(
            interpret_integrity(1, 5),
            Ok(IntegrityResult::CorruptionFound)
        );
        assert!(interpret_integrity(0, 87).is_err());
        assert!(interpret_integrity(2, 0).is_err());
        assert!(interpret_integrity(-1, -1).is_err());
    }

    #[test]
    fn boot_time_xml_parsing() {
        let xml = "<Event><EventData><Data Name='BootTime'>45678</Data></EventData></Event>";
        assert_eq!(parse_boot_time_secs(xml), Some(45));
        let xml = "<Data Name=\"BootTime\">91000</Data>";
        assert_eq!(parse_boot_time_secs(xml), Some(91));
        assert_eq!(parse_boot_time_secs("<Event/>"), None);
        assert_eq!(parse_boot_time_secs("<Data Name='BootTime'>x</Data>"), None);
    }

    #[test]
    fn real_probe_system_smoke() {
        let p = WindowsProbe::new();
        // integrity_scan is deliberately NOT run: it takes minutes.
        assert!(is_elevated().is_ok());
        match p.problem_devices() {
            Ok(_) | Err(ProbeError::Unavailable(_)) => {}
            Err(e) => panic!("unexpected: {e}"),
        }
        match p.boot_seconds() {
            Ok(_) | Err(ProbeError::Unavailable(_)) => {}
            Err(e) => panic!("unexpected: {e}"),
        }
    }

    #[test]
    fn run_exit_code_reports_code_and_kills_on_timeout() {
        let cmd = system32_dir().join("cmd.exe");
        assert_eq!(
            run_exit_code(&cmd, &["/c", "exit 3"], Duration::from_secs(10)),
            Ok(3)
        );
        let slow = run_exit_code(
            &cmd,
            &["/c", "ping -n 30 127.0.0.1 >nul"],
            Duration::from_millis(300),
        );
        assert!(matches!(slow, Err(ProbeError::Unavailable(_))));
    }

    #[test]
    fn system_dirs_come_from_the_api_and_exist() {
        let sys = system32_dir();
        assert!(sys.join("cmd.exe").is_file(), "{}", sys.display());
        assert!(windows_dir().join("System32").is_dir());
    }

    #[test]
    fn run_process_does_not_inherit_the_environment() {
        let cmd = system32_dir().join("cmd.exe");
        std::env::set_var("SAGLIK_TEST_LEAK", "1");
        // `if defined` exits 7 only when the variable reached the child.
        let code = run_process(
            &cmd,
            &["/c", "if defined SAGLIK_TEST_LEAK (exit 7) else (exit 0)"],
            Duration::from_secs(10),
        );
        assert_eq!(code, Ok(0));
        let slow = run_process(
            &cmd,
            &["/c", "ping -n 30 127.0.0.1 >nul"],
            Duration::from_millis(300),
        );
        assert_eq!(slow, Err(RunError::Timeout));
        let missing = run_process(&sys_missing(), &[], Duration::from_secs(1));
        assert!(matches!(missing, Err(RunError::Spawn(_))));
    }

    fn sys_missing() -> PathBuf {
        system32_dir().join("definitely-not-here-xyz.exe")
    }

    #[test]
    fn run_entries_have_well_formed_ids() {
        for e in run_entries() {
            assert!(e.hive == "HKCU" || e.hive == "HKLM");
            assert_eq!(e.id(), format!("{}|{}", e.hive, e.name));
            assert!(
                e.approved_key == STARTUP_APPROVED_RUN || e.approved_key == STARTUP_APPROVED_RUN32
            );
            if e.hive == "HKCU" {
                assert_eq!(e.approved_key, STARTUP_APPROVED_RUN);
            }
        }
    }

    #[test]
    fn run_with_timeout_returns_value_or_times_out() {
        let d = Duration::from_millis(50);
        assert_eq!(run_with_timeout(d, || 7), Ok(7));
        let slow = run_with_timeout(d, || {
            std::thread::sleep(Duration::from_millis(500));
            1
        });
        assert_eq!(slow, Err(ProbeError::Unavailable("WMI timed out".into())));
    }
}
