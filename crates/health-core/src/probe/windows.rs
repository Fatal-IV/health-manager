//! Real Windows implementation of [`SystemProbe`].

use super::{DiskInfo, Impact, ProbeError, ProcInfo, StartupItem, SystemProbe};
use std::path::{Path, PathBuf};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
    KEY_READ, KEY_WOW64_32KEY, REG_SAM_FLAGS,
};
use windows::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};

const DRIVE_UNKNOWN: u32 = 0;
const DRIVE_NO_ROOT_DIR: u32 = 1;
const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_CDROM: u32 = 5;
const DRIVE_REMOTE: u32 = 4;

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
}
