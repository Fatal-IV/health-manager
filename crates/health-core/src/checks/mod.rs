pub mod performance;
pub mod security;
pub mod storage;
pub mod system;
pub mod update;

pub use crate::engine::Check;

/// Registry of all checks; later tasks register theirs here.
pub fn all_checks() -> Vec<Box<dyn Check>> {
    vec![
        Box::new(storage::DiskSpaceCheck),
        Box::new(storage::TempFilesCheck),
        Box::new(storage::RecycleBinCheck),
        Box::new(storage::UpdateLeftoversCheck),
        Box::new(performance::CpuRamCheck),
        Box::new(performance::TopProcessesCheck),
        Box::new(performance::StartupItemsCheck),
        Box::new(update::PendingUpdatesCheck),
        Box::new(update::UpdateServiceCheck),
        Box::new(update::LastUpdateCheck),
        Box::new(security::DefenderCheck),
        Box::new(security::FirewallCheck),
        Box::new(security::UacCheck),
        Box::new(system::ProblemDevicesCheck),
        Box::new(system::BootTimeCheck),
        Box::new(system::IntegrityCheck),
    ]
}
