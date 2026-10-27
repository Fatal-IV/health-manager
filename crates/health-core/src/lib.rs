pub mod boot_snapshot;
pub mod checks;
pub mod engine;
pub mod fixes;
pub mod model;
pub mod probe;
pub mod report;
pub mod scoring;
pub mod summary;
pub mod verify;

/// Returns the crate version.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_not_empty() {
        assert!(!version().is_empty());
    }
}
