use thiserror::Error;

#[cfg(any(test, feature = "testing"))]
pub mod fake;

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

/// Abstraction over the host system; later tasks add methods here.
pub trait SystemProbe: Send + Sync {
    fn disks(&self) -> Result<Vec<DiskInfo>, ProbeError>;
}
