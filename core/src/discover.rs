// SPDX-License-Identifier: MIT
//! Opening a known game-data source.

use std::path::Path;

use crate::Library;
use crate::catalog::Naming;
use crate::error::Result;
use crate::source::{DiscSource, FolderSource};

/// What type of data a source is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType {
    /// An extracted folder.
    Folder,
    /// A disc image.
    Disc,
}

/// Open a known source with `naming`.
pub fn open_with(media_type: SourceType, path: &Path, naming: &dyn Naming) -> Result<Library<'static>> {
    match media_type {
        SourceType::Folder => Library::open_with(FolderSource::open(path)?, naming),
        SourceType::Disc => Library::open_with(DiscSource::open(path)?, naming),
    }
}
