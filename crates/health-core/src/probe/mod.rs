use std::time::SystemTime;
use thiserror::Error;

#[cfg(any(test, feature = "testing"))]
pub mod fake;

#[cfg(windows)]
pub mod windows;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProbeError {
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskInfo {
    pub mount: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub removable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
    /// CPU usage in percent (may exceed 100 on multi-core hosts).
    pub cpu: f32,
    pub ram_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Impact {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupItem {
    /// Stable identifier used to disable the item later.
    pub id: String,
    pub name: String,
    pub impact: Impact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Running,
    Stopped,
    Disabled,
    Missing,
}

/// Windows Defender state. `third_party_av` is true when another antivirus is
/// registered and active; Defender is then normally switched off on purpose, so
/// `realtime == false` must not be read as unprotected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefenderState {
    pub realtime: bool,
    pub signature_age_days: u32,
    pub third_party_av: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UacLevel {
    /// `EnableLUA` = 0.
    Disabled,
    /// UAC on, but administrators are never prompted (`ConsentPromptBehaviorAdmin` = 0).
    NeverNotify,
    Enabled,
}

/// Abstraction over the host system; later tasks add methods here.
pub trait SystemProbe: Send + Sync {
    fn disks(&self) -> Result<Vec<DiskInfo>, ProbeError>;
    /// Recursive size in bytes of the user temp directory.
    fn temp_bytes(&self) -> Result<u64, ProbeError>;
    /// Size in bytes of the items in the recycle bin.
    fn recycle_bin_bytes(&self) -> Result<u64, ProbeError>;
    /// Size in bytes of Windows Update download leftovers.
    fn update_leftovers_bytes(&self) -> Result<u64, ProbeError>;
    /// Overall CPU usage in percent, measured over a short interval.
    fn cpu_percent(&self) -> Result<f32, ProbeError>;
    /// `(used, total)` physical memory in bytes.
    fn ram_usage(&self) -> Result<(u64, u64), ProbeError>;
    /// The `n` heaviest processes, busiest first.
    fn top_processes(&self, n: usize) -> Result<Vec<ProcInfo>, ProbeError>;
    fn startup_items(&self) -> Result<Vec<StartupItem>, ProbeError>;
    /// Number of pending (not installed, not hidden) Windows updates, from the
    /// local cache only. `Err` when it cannot be determined.
    fn pending_updates(&self) -> Result<u32, ProbeError>;
    /// State of a Windows service, read from the Service Control Manager.
    fn service_state(&self, name: &str) -> Result<ServiceState, ProbeError>;
    /// Time of the last successful Windows update install; `None` = never/unknown.
    fn last_update_success(&self) -> Result<Option<SystemTime>, ProbeError>;
    /// Windows Defender status (WMI); `Err` when it cannot be determined.
    fn defender(&self) -> Result<DefenderState, ProbeError>;
    /// `false` if any firewall profile (standard, public, domain) is disabled.
    fn firewall_enabled(&self) -> Result<bool, ProbeError>;
    fn uac_level(&self) -> Result<UacLevel, ProbeError>;
    /// Current time; a method so tests do not depend on the real clock.
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}
