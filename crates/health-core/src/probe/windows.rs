//! Real Windows implementation of [`SystemProbe`].

use super::{DiskInfo, ProbeError, SystemProbe};
use std::path::{Path, PathBuf};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives};
use windows::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};

const DRIVE_UNKNOWN: u32 = 0;
const DRIVE_NO_ROOT_DIR: u32 = 1;
const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_CDROM: u32 = 5;

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
            if kind == DRIVE_UNKNOWN || kind == DRIVE_NO_ROOT_DIR {
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
}
