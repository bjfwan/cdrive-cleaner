pub mod detector;

pub use detector::{analyze, invalidate_safety_cache, MigrationSafety, Verdict};
