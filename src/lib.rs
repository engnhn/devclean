pub mod artifact;
pub mod cleanup;
pub mod error;
pub mod fs;
pub mod ui;

// Public API re-exports
pub use artifact::{ArtifactKind, FileIdentity, Finding};
pub use error::{DevcleanError, Result};
pub use fs::{ScanOptions, ScanResult, ScanStats, scan_path, scan_path_with_options};
