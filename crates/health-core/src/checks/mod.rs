pub mod storage;

pub use crate::engine::Check;

/// Registry of all checks; later tasks register theirs here.
pub fn all_checks() -> Vec<Box<dyn Check>> {
    vec![
        Box::new(storage::DiskSpaceCheck),
        Box::new(storage::TempFilesCheck),
        Box::new(storage::RecycleBinCheck),
        Box::new(storage::UpdateLeftoversCheck),
    ]
}
