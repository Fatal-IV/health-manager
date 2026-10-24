use super::{DiskInfo, ProbeError, SystemProbe};

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
}
