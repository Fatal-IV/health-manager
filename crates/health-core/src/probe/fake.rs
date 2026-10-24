use super::{
    DefenderState, DeviceProblem, DiskInfo, IntegrityResult, ProbeError, ProcInfo, ServiceState,
    StartupItem, SystemProbe, UacLevel,
};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

/// Configurable probe for tests, with error injection.
#[derive(Debug, Default, Clone)]
pub struct FakeProbe {
    pub disks: Vec<DiskInfo>,
    pub disks_error: Option<ProbeError>,
    pub temp_bytes: u64,
    pub temp_error: Option<ProbeError>,
    pub recycle_bin_bytes: u64,
    pub recycle_bin_error: Option<ProbeError>,
    pub update_leftovers_bytes: u64,
    pub update_leftovers_error: Option<ProbeError>,
    pub cpu_percent: f32,
    pub cpu_error: Option<ProbeError>,
    pub ram_used: u64,
    pub ram_total: u64,
    pub ram_error: Option<ProbeError>,
    pub processes: Vec<ProcInfo>,
    pub processes_error: Option<ProbeError>,
    pub startup_items: Vec<StartupItem>,
    pub startup_error: Option<ProbeError>,
    pub pending_updates: u32,
    pub pending_updates_error: Option<ProbeError>,
    /// Per service name; an unlisted service is `Missing`.
    pub services: BTreeMap<String, ServiceState>,
    pub services_error: Option<ProbeError>,
    pub last_update: Option<SystemTime>,
    pub last_update_error: Option<ProbeError>,
    /// `None` = healthy, up-to-date Defender.
    pub defender: Option<DefenderState>,
    pub defender_error: Option<ProbeError>,
    /// `None` = enabled.
    pub firewall: Option<bool>,
    pub firewall_error: Option<ProbeError>,
    /// `None` = `UacLevel::Enabled`.
    pub uac: Option<UacLevel>,
    pub uac_error: Option<ProbeError>,
    pub problem_devices: Vec<DeviceProblem>,
    pub problem_devices_error: Option<ProbeError>,
    /// `None` = no boot event.
    pub boot_seconds: Option<u32>,
    pub boot_error: Option<ProbeError>,
    /// `None` = `IntegrityResult::Clean`.
    pub integrity: Option<IntegrityResult>,
    pub integrity_error: Option<ProbeError>,
    /// Counts `integrity_scan` calls (shared across clones).
    pub integrity_calls: Arc<AtomicUsize>,
    /// Fixed clock; `None` uses the real time.
    pub now: Option<SystemTime>,
}

fn pick<T: Clone>(err: &Option<ProbeError>, v: T) -> Result<T, ProbeError> {
    match err {
        Some(e) => Err(e.clone()),
        None => Ok(v),
    }
}

impl SystemProbe for FakeProbe {
    fn disks(&self) -> Result<Vec<DiskInfo>, ProbeError> {
        pick(&self.disks_error, self.disks.clone())
    }
    fn temp_bytes(&self) -> Result<u64, ProbeError> {
        pick(&self.temp_error, self.temp_bytes)
    }
    fn recycle_bin_bytes(&self) -> Result<u64, ProbeError> {
        pick(&self.recycle_bin_error, self.recycle_bin_bytes)
    }
    fn update_leftovers_bytes(&self) -> Result<u64, ProbeError> {
        pick(&self.update_leftovers_error, self.update_leftovers_bytes)
    }
    fn cpu_percent(&self) -> Result<f32, ProbeError> {
        pick(&self.cpu_error, self.cpu_percent)
    }
    fn ram_usage(&self) -> Result<(u64, u64), ProbeError> {
        pick(&self.ram_error, (self.ram_used, self.ram_total))
    }
    fn top_processes(&self, n: usize) -> Result<Vec<ProcInfo>, ProbeError> {
        pick(
            &self.processes_error,
            self.processes.iter().take(n).cloned().collect(),
        )
    }
    fn startup_items(&self) -> Result<Vec<StartupItem>, ProbeError> {
        pick(&self.startup_error, self.startup_items.clone())
    }
    fn pending_updates(&self) -> Result<u32, ProbeError> {
        pick(&self.pending_updates_error, self.pending_updates)
    }
    fn service_state(&self, name: &str) -> Result<ServiceState, ProbeError> {
        pick(
            &self.services_error,
            self.services
                .get(name)
                .copied()
                .unwrap_or(ServiceState::Missing),
        )
    }
    fn last_update_success(&self) -> Result<Option<SystemTime>, ProbeError> {
        pick(&self.last_update_error, self.last_update)
    }
    fn defender(&self) -> Result<DefenderState, ProbeError> {
        pick(
            &self.defender_error,
            self.defender.unwrap_or(DefenderState {
                realtime: true,
                signature_age_days: 0,
                third_party_av: Some(false),
            }),
        )
    }
    fn firewall_enabled(&self) -> Result<bool, ProbeError> {
        pick(&self.firewall_error, self.firewall.unwrap_or(true))
    }
    fn uac_level(&self) -> Result<UacLevel, ProbeError> {
        pick(&self.uac_error, self.uac.unwrap_or(UacLevel::Enabled))
    }
    fn problem_devices(&self) -> Result<Vec<DeviceProblem>, ProbeError> {
        pick(&self.problem_devices_error, self.problem_devices.clone())
    }
    fn boot_seconds(&self) -> Result<Option<u32>, ProbeError> {
        pick(&self.boot_error, self.boot_seconds)
    }
    fn integrity_scan(&self) -> Result<IntegrityResult, ProbeError> {
        self.integrity_calls.fetch_add(1, Ordering::SeqCst);
        pick(
            &self.integrity_error,
            self.integrity.unwrap_or(IntegrityResult::Clean),
        )
    }
    fn now(&self) -> SystemTime {
        self.now.unwrap_or_else(SystemTime::now)
    }
}
