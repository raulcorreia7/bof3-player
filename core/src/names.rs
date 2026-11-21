// SPDX-License-Identifier: MIT
//! The names file: titles and labels for the catalog, read from one JSON file.
//!
//! The library ships no game text. A names file supplies it. The schema is in `docs/player.md`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::catalog::Naming;

/// The schema version this reader understands.
pub const VERSION: u32 = 1;

/// The names this repository ships, `data/bof3-names.json`, parsed on first use.
#[cfg(feature = "bundled-names")]
#[must_use]
pub fn game_names() -> &'static Names {
    static GAME: std::sync::LazyLock<Names> = std::sync::LazyLock::new(|| Names::from_json(include_str!("../../data/bof3-names.json")).expect("data/bof3-names.json is a valid names file"));
    &GAME
}

/// Where a name comes from, from most to least trusted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// A table or header in the game files.
    Game,
    /// In-game text that names the item.
    Text,
    /// A published data set that carries a reference.
    Data,
    /// A neutral label built from where the item plays; not a title.
    Label,
    /// No source names the item.
    #[default]
    None,
}

/// One named item.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Entry {
    /// The title or label. Absent for a copy that shares its original's title.
    pub title: Option<String>,
    /// Where the title comes from.
    #[serde(default)]
    pub source: Source,
    /// A short pointer to the evidence: a file and offset, or a table and symbol.
    pub evidence: Option<String>,
    /// Where the item plays, when the title is a label.
    pub area: Option<String>,
    /// The id of the item whose content this one repeats.
    pub same_as: Option<String>,
    /// The game's sound ids that select this music.
    #[serde(default)]
    pub sound: Vec<u16>,
    /// The ability indexes a magic bank serves, in the order of the names in `title`.
    #[serde(default)]
    pub abilities: Vec<u16>,
}

/// The names of one file, keyed by catalog id.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Names {
    version: u32,
    #[serde(default)]
    music: BTreeMap<String, Entry>,
    #[serde(default)]
    banks: BTreeMap<String, Entry>,
    #[serde(default)]
    xa: BTreeMap<String, Entry>,
}

impl Names {
    /// Parse a names file.
    ///
    /// # Errors
    /// Returns the reason when the text is not a names file or its version is not [`VERSION`].
    pub fn from_json(text: &str) -> Result<Self, String> {
        let names: Self = serde_json::from_str(text).map_err(|error| error.to_string())?;
        if names.version == VERSION { Ok(names) } else { Err(format!("unsupported names version {} (this player reads version {VERSION})", names.version)) }
    }

    /// Read the names file at `path`.
    ///
    /// # Errors
    /// Returns the reason when the file cannot be read or parsed.
    pub fn read(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        Self::from_json(&text)
    }

    /// The music entries, keyed `music:ARCHIVE#SEQUENCE`.
    #[must_use]
    pub fn music(&self) -> &BTreeMap<String, Entry> {
        &self.music
    }

    /// The bank entries, keyed `bank:ARCHIVE`.
    #[must_use]
    pub fn banks(&self) -> &BTreeMap<String, Entry> {
        &self.banks
    }

    /// The XA entries, keyed by catalog id.
    #[must_use]
    pub fn xa(&self) -> &BTreeMap<String, Entry> {
        &self.xa
    }

    fn music_entry(&self, slot: &str) -> Option<&Entry> {
        self.music.get(&format!("music:{slot}"))
    }

    fn bank_entry(&self, archive: &str) -> Option<&Entry> {
        self.banks.get(&format!("bank:{archive}"))
    }
}

impl Naming for Names {
    fn xa_span(&self, id: &str) -> Option<&str> {
        self.xa.get(id)?.title.as_deref()
    }

    fn music_copy(&self, slot: &str) -> Option<&str> {
        self.music_entry(slot)?.same_as.as_deref()?.strip_prefix("music:")
    }

    fn music_sound(&self, slot: &str) -> Option<u16> {
        self.music_entry(slot)?.sound.first().copied()
    }

    fn bank_name(&self, path: &str) -> Option<&str> {
        let file = path.rsplit('/').next()?;
        let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
        self.bank_entry(stem)?.title.as_deref()
    }

    fn bank_duplicate(&self, archive: &str) -> Option<&str> {
        self.bank_entry(archive)?.same_as.as_deref()?.strip_prefix("bank:")
    }

    fn magic_ability(&self, ability: u16) -> Option<&str> {
        self.banks.values().find_map(|entry| {
            let index = entry.abilities.iter().position(|&value| value == ability)?;
            entry.title.as_deref()?.split(", ").nth(index)
        })
    }

    fn music_title(&self, archive: &str, sequence: u32) -> Option<&str> {
        let mut entry = self.music_entry(&format!("{archive}#{sequence}"))?;
        // The original of a copy is never itself a copy; the bound only guards a bad file.
        for _ in 0..4 {
            if let Some(title) = entry.title.as_deref() {
                return Some(title);
            }
            entry = self.music.get(entry.same_as.as_deref()?)?;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_other_versions_and_bad_text() {
        assert!(Names::from_json(r#"{"version":2}"#).is_err());
        assert!(Names::from_json("A#0\tTitle").is_err());
        assert!(Names::from_json(r#"{"version":1,"music":{"music:A#0":{"source":"web"}}}"#).is_err());
    }

    /// The file the repository ships, `data/bof3-names.json`.
    const SHIPPED: &str = include_str!("../../data/bof3-names.json");

    #[cfg(feature = "bundled-names")]
    #[test]
    fn the_shipped_file_reads_and_every_title_has_a_source() {
        let names = Names::from_json(SHIPPED).unwrap();
        assert!(!names.music().is_empty());
        for (id, entry) in names.music().iter().chain(names.banks()).chain(names.xa()) {
            if entry.title.is_some() {
                assert_ne!(entry.source, Source::None, "{id} has a title and no source");
                assert!(entry.evidence.is_some(), "{id} has a title and no evidence");
            }
        }
    }
}
