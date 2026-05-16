use std::path::Path;

use crate::winfs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanBackendKind {
    Native,
    MftUsn,
}

impl ScanBackendKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::MftUsn => "mft_usn",
        }
    }
}

pub fn select_backend(path: &Path) -> ScanBackendKind {
    if winfs::supports_mft_scan(path) {
        ScanBackendKind::MftUsn
    } else {
        ScanBackendKind::Native
    }
}
