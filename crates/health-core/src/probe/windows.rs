//! Real Windows implementation of [`SystemProbe`].

use super::{DiskInfo, Impact, ProbeError, ProcInfo, ServiceState, StartupItem, SystemProbe};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{
    ERROR_MORE_DATA, ERROR_SERVICE_DOES_NOT_EXIST, ERROR_SUCCESS, VARIANT_FALSE,
};
use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumValueW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, REG_SAM_FLAGS, RRF_RT_REG_SZ,
};
use windows::Win32::System::Services::{
    CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceConfigW, QueryServiceStatus,
    QUERY_SERVICE_CONFIGW, SC_HANDLE, SC_MANAGER_CONNECT, SERVICE_DISABLED, SERVICE_QUERY_CONFIG,
    SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_STATUS,
};
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
struct ComGuard(bool);

impl ComGuard {
    fn new() -> Self {
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
        const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
        const RUN_32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
        const APPROVED: &str =
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
        const APPROVED_32: &str =
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32";
        let mut out: Vec<StartupItem> = Vec::new();
        for (root, label) in [(HKEY_CURRENT_USER, "HKCU"), (HKEY_LOCAL_MACHINE, "HKLM")] {
            // StartupApproved: first byte even (0x02, 0x06) = enabled, odd = disabled;
            // a missing value means enabled.
            let approved = registry_values(root, APPROVED, REG_SAM_FLAGS(0));
            let approved_32 = registry_values(root, APPROVED_32, REG_SAM_FLAGS(0));
            // HKLM 32-bit programs live in the WOW6432Node view; read it explicitly
            // with KEY_WOW64_32KEY so it works regardless of process bitness.
            let views = [
                (RUN, REG_SAM_FLAGS(0), &approved),
                (RUN, KEY_WOW64_32KEY, &approved_32),
                (RUN_32, REG_SAM_FLAGS(0), &approved_32),
            ];
            for (key, view, appr) in views {
                for (name, _) in registry_values(root, key, view) {
                    let disabled = appr.iter().any(|(n, b)| {
                        n.eq_ignore_ascii_case(&name) && b.is_some_and(|b| b & 1 == 1)
                    });
                    let id = format!("{label}|{name}");
                    if disabled || out.iter().any(|i| i.id == id) {
                        continue;
                    }
                    out.push(StartupItem {
                        id,
                        // No cheap reliable impact measure exists (it would need boot
                        // trace data), so every registry Run entry is rated Medium.
                        impact: Impact::Medium,
                        name,
                    });
                }
            }
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

    /// Last successful update. Order: registry `LastSuccessTime` (UTC string;
    /// often absent or stale on Windows 10/11), then the WUA update history.
    fn last_update_success(&self) -> Result<Option<SystemTime>, ProbeError> {
        choose_last_update(registry_last_success(), wua_last_success)
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
}
