// SPDX-License-Identifier: MIT
//! Opening a known game-data source.

use std::path::Path;

use crate::Library;
use crate::catalog::Naming;
use crate::error::Result;
use crate::source::{DiscSource, FolderSource};

/// What kind of data a source is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// An extracted folder.
    Folder,
    /// A disc image.
    Disc,
}

/// Open a known source with `naming`.
pub fn open_with(kind: SourceKind, path: &Path, naming: &dyn Naming) -> Result<Library<'static>> {
    match kind {
        SourceKind::Folder => Library::open_with(FolderSource::open(path)?, naming),
        SourceKind::Disc => Library::open_with(DiscSource::open(path)?, naming),
    }
}
