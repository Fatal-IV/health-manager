//! The short "how does this computer look right now" summary shown on the start
//! screen. It reads a few facts only; nothing is scanned or judged here, and a
//! value that cannot be read is simply left out.

use crate::probe::{DiskInfo, SystemProbe};
use serde::Serialize;

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct DiskSpace {
    pub mount: String,
    pub free_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct MemoryUse {
    pub used_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemSummary {
    /// e.g. `Windows 11 Home`; `None` when the system does not say.
    pub os: Option<String>,
    pub uptime_secs: Option<u64>,
    pub disk: Option<DiskSpace>,
    pub memory: Option<MemoryUse>,
}

/// The drive Windows runs from (`SystemDrive`, normally `C:`), else the first fixed drive.
fn system_disk(disks: &[DiskInfo], system_drive: Option<&str>) -> Option<DiskSpace> {
    let pick = system_drive
        .and_then(|d| disks.iter().find(|x| x.mount.eq_ignore_ascii_case(d.trim_end_matches('\\'))))
        .or_else(|| disks.iter().find(|x| !x.removable))?;
    (pick.total_bytes > 0).then(|| DiskSpace {
        mount: pick.mount.clone(),
        free_bytes: pick.free_bytes,
        total_bytes: pick.total_bytes,
    })
}

/// Builds the summary from the probe; `os` and `uptime` come from the host (see [`host_facts`]).
pub fn summarize(
    p: &dyn SystemProbe,
    os: Option<String>,
    uptime_secs: Option<u64>,
    system_drive: Option<&str>,
) -> SystemSummary {
    SystemSummary {
        os,
        uptime_secs,
        disk: p.disks().ok().and_then(|d| system_disk(&d, system_drive)),
        memory: p
            .ram_usage()
            .ok()
            .filter(|(_, total)| *total > 0)
            .map(|(used, total)| MemoryUse { used_bytes: used, total_bytes: total }),
    }
}

/// Operating system name and time since the last start, as the host reports them.
pub fn host_facts() -> (Option<String>, Option<u64>) {
    (sysinfo::System::long_os_version(), Some(sysinfo::System::uptime()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::fake::FakeProbe;
    use crate::probe::ProbeError;

    const GB: u64 = 1024 * 1024 * 1024;

    fn disk(mount: &str, free: u64, total: u64, removable: bool) -> DiskInfo {
        DiskInfo { mount: mount.into(), total_bytes: total, free_bytes: free, removable }
    }

    #[test]
    fn reports_the_system_drive_and_memory() {
        let p = FakeProbe {
            disks: vec![disk("D:", 10 * GB, 100 * GB, false), disk("C:", 142 * GB, 237 * GB, false)],
            ram_used: 9 * GB,
            ram_total: 16 * GB,
            ..Default::default()
        };
        let s = summarize(&p, Some("Windows 11 Home".into()), Some(3600), Some("C:"));
        assert_eq!(s.disk, Some(DiskSpace { mount: "C:".into(), free_bytes: 142 * GB, total_bytes: 237 * GB }));
        assert_eq!(s.memory, Some(MemoryUse { used_bytes: 9 * GB, total_bytes: 16 * GB }));
        assert_eq!(s.os.as_deref(), Some("Windows 11 Home"));
        assert_eq!(s.uptime_secs, Some(3600));
    }

    #[test]
    fn without_a_known_system_drive_the_first_fixed_one_is_used() {
        let p = FakeProbe {
            disks: vec![disk("E:", GB, 8 * GB, true), disk("D:", 10 * GB, 100 * GB, false)],
            ..Default::default()
        };
        assert_eq!(summarize(&p, None, None, None).disk.map(|d| d.mount), Some("D:".to_string()));
        // `SystemDrive` may come with a trailing backslash.
        assert_eq!(summarize(&p, None, None, Some("D:\\")).disk.map(|d| d.mount), Some("D:".to_string()));
    }

    #[test]
    fn what_cannot_be_read_is_left_out() {
        let p = FakeProbe {
            disks_error: Some(ProbeError::Unavailable("x".into())),
            ram_error: Some(ProbeError::Unavailable("x".into())),
            ..Default::default()
        };
        assert_eq!(summarize(&p, None, None, None), SystemSummary::default());
        // A zero-sized disk or memory is not a reading.
        let zero = FakeProbe { disks: vec![disk("C:", 0, 0, false)], ..Default::default() };
        let s = summarize(&zero, None, None, Some("C:"));
        assert!(s.disk.is_none() && s.memory.is_none());
    }
}
