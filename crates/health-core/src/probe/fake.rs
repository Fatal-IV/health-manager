use super::{DiskInfo, ProbeError, ProcInfo, StartupItem, SystemProbe};

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
}
