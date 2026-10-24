pub use crate::engine::Check;

/// Registry of all checks; later tasks register theirs here.
pub fn all_checks() -> Vec<Box<dyn Check>> {
    Vec::new()
}
