use super::{DiskInfo, ProbeError, SystemProbe};

/// Configurable probe for tests, with error injection.
#[derive(Debug, Default, Clone)]
pub struct FakeProbe {
    pub disks: Vec<DiskInfo>,
    pub disks_error: Option<ProbeError>,
}

impl SystemProbe for FakeProbe {
    fn disks(&self) -> Result<Vec<DiskInfo>, ProbeError> {
        match &self.disks_error {
            Some(e) => Err(e.clone()),
            None => Ok(self.disks.clone()),
        }
    }
}
