// SPDX-License-Identifier: MIT
//! Finding the game data.
//!
//! Probes, in order: the explicit hint and `$BOF3_GAME_DIR` (each as itself
//! and as `<value>/out/extracted`), the working directory and
//! its parents, then the executable's directory and its parents; each of those folders is probed
//! as itself and as `out/extracted`, `decomp/build/disc` (the monorepo's disc), `BIN` and
//! `inputs/external` below it. A candidate is
//! accepted when it is an extracted tree whose `BIN/BGM` holds an `.EMI`
//! archive (also when the path names `BIN`, an asset directory or one archive:
//! discovery walks up to three levels to the root), or a `.cue`/disc image whose
//! ISO 9660 root has a `BIN` directory.

use std::path::{Path, PathBuf};

use bof3player::catalog::Naming;
use bof3player::discover::{SourceKind, open_with};
use bof3player::names::Names;
use bof3player::{DiscSource, Error, FolderSource, Library, Result, Source};

/// How many levels above a hint the tree root may sit.
const ROOT_ASCENT: usize = 3;
/// Parents of the working directory that are probed.
const WORKING_PARENTS: usize = 5;
/// Parents of the executable directory that are probed.
const EXECUTABLE_PARENTS: usize = 3;
/// The file name [`default_names_path`] looks for.
const DEFAULT_NAMES: &str = "bof3-names.json";

/// One probed location.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Why it was probed (`hint`, `BOF3_GAME_DIR`, `cwd/..`, ...).
    pub label: String,
    /// The path tried.
    pub path: PathBuf,
    /// Accepted as the data source.
    pub accepted: bool,
    /// What was found, or why it was rejected.
    pub detail: String,
}

/// The outcome of [`discover`].
#[derive(Debug, Clone)]
pub struct Discovery {
    /// Every probe made, in order, up to and including the accepted one.
    pub candidates: Vec<Candidate>,
    /// The accepted source, if any.
    pub found: Option<(SourceKind, PathBuf)>,
}

impl Discovery {
    /// Open the discovered source as a library with `naming`.
    pub fn open_with(&self, naming: &dyn Naming) -> Result<Library<'static>> {
        match &self.found {
            Some((kind, path)) => open_with(*kind, path, naming),
            None => Err(Error::io("no game data found: pass -i PATH or set BOF3_GAME_DIR")),
        }
    }
}

fn folder_ok(dir: &Path) -> std::result::Result<String, String> {
    let source = FolderSource::open(dir).map_err(|e| e.message().to_string())?;
    let count = source.list().map_err(|e| e.message().to_string())?.iter().filter(|p| p.to_ascii_lowercase().starts_with("bin/bgm/") && p.to_ascii_lowercase().ends_with(".emi")).count();
    if count == 0 { Err("no BIN/BGM/*.EMI archive".into()) } else { Ok(format!("BIN/BGM: {count} EMI archives")) }
}

fn probe(path: &Path) -> (Option<SourceKind>, PathBuf, String) {
    if !path.exists() {
        return (None, path.to_path_buf(), "does not exist".into());
    }
    let mut reason = String::new();
    let is_dir = path.is_dir();
    if is_dir || path.extension().is_some_and(|e| e.eq_ignore_ascii_case("emi")) {
        let mut dir = if is_dir { path.to_path_buf() } else { path.parent().unwrap_or(Path::new(".")).to_path_buf() };
        for ascent in 0..=ROOT_ASCENT {
            match folder_ok(&dir) {
                Ok(found) => {
                    let up = if ascent > 0 { format!(" ({ascent} level(s) up from the hint)") } else { String::new() };
                    return (Some(SourceKind::Folder), dir, format!("{found}{up}"));
                }
                Err(why) if ascent == 0 => reason = why,
                Err(_) => {}
            }
            match dir.parent() {
                Some(parent) if parent != dir && !parent.as_os_str().is_empty() => dir = parent.to_path_buf(),
                _ => break,
            }
        }
    }
    match DiscSource::open(path) {
        Ok(disc) => match disc.list() {
            Ok(files) if files.iter().any(|f| f.to_ascii_lowercase().starts_with("bin/")) => (Some(SourceKind::Disc), path.to_path_buf(), disc.describe()),
            _ => (None, path.to_path_buf(), "ISO9660 root has no BIN directory".into()),
        },
        Err(e) => (None, path.to_path_buf(), if reason.is_empty() { e.message().to_string() } else { reason }),
    }
}

fn probes(hint: Option<&Path>) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut root = |label: &str, value: PathBuf| {
        out.push((label.to_string(), value.clone()));
        out.push((format!("{label}/out/extracted"), value.join("out/extracted")));
    };
    if let Some(hint) = hint {
        root("hint", hint.to_path_buf());
    }
    if let Some(value) = std::env::var_os("BOF3_GAME_DIR").filter(|v| !v.is_empty()) {
        root("BOF3_GAME_DIR", PathBuf::from(value));
    }
    let scan = |out: &mut Vec<(String, PathBuf)>, label: String, base: &Path| {
        for (suffix, sub) in [("/out/extracted", "out/extracted"), ("/decomp/build/disc", "decomp/build/disc"), ("/BIN", "BIN"), ("/inputs/external", "inputs/external"), ("", "")] {
            out.push((format!("{label}{suffix}"), if sub.is_empty() { base.to_path_buf() } else { base.join(sub) }));
        }
    };
    if let Ok(cwd) = std::env::current_dir() {
        for (depth, base) in cwd.ancestors().take(WORKING_PARENTS + 1).enumerate() {
            let label = if depth == 0 { "cwd".into() } else { format!("cwd/{}", vec![".."; depth].join("/")) };
            scan(&mut out, label, base);
        }
    }
    if let Some(exe) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
        for (depth, base) in exe.ancestors().take(EXECUTABLE_PARENTS + 1).enumerate() {
            let label = if depth == 0 { "exe".into() } else { format!("exe/{}", vec![".."; depth].join("/")) };
            scan(&mut out, label, base);
        }
    }
    out
}

/// Probe for game data. An explicit `hint` that does not exist is an error: no
/// automatic probes run then, so a typo is never silently replaced.
#[must_use]
pub fn discover(hint: Option<&Path>) -> Discovery {
    let mut candidates = Vec::new();
    let mut list = probes(hint);
    if hint.is_some_and(|h| !h.exists()) {
        list.truncate(2);
    }
    for (label, path) in list {
        let (kind, path, detail) = probe(&path);
        candidates.push(Candidate { label, path: path.clone(), accepted: kind.is_some(), detail });
        if let Some(kind) = kind {
            return Discovery { candidates, found: Some((kind, path)) };
        }
    }
    Discovery { candidates, found: None }
}

/// One entry of a folder listing for choosing game data.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The file or folder name.
    pub name: String,
    /// The full path.
    pub path: PathBuf,
    /// Whether the entry is a folder. Files are disc images or cue sheets.
    pub is_dir: bool,
}

/// Where to start choosing game data: the home folder, else the working directory.
#[must_use]
pub fn start_dir() -> PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).filter(|v| !v.is_empty());
    home.map(PathBuf::from).or_else(|| std::env::current_dir().ok()).unwrap_or_else(|| PathBuf::from("."))
}

/// The folders and the supported files (`.cue`, `.bin`, `.iso`, `.img`) in `dir`, folders first,
/// each group by name. Hidden entries are left out.
pub fn browse(dir: &Path) -> std::io::Result<Vec<Entry>> {
    let mut entries: Vec<Entry> = std::fs::read_dir(dir)?
        .filter_map(std::result::Result::ok)
        .filter_map(|item| {
            let name = item.file_name().to_string_lossy().into_owned();
            let path = item.path();
            let is_dir = path.is_dir();
            let supported = path.extension().is_some_and(|ext| ["cue", "bin", "iso", "img"].iter().any(|wanted| ext.eq_ignore_ascii_case(wanted)));
            (!name.starts_with('.') && (is_dir || supported)).then_some(Entry { name, path, is_dir })
        })
        .collect();
    entries.sort_by_key(|entry| (!entry.is_dir, entry.name.to_lowercase()));
    Ok(entries)
}

/// The names file to use when `--names` is absent: `$BOF3_NAMES`, then `bof3-names.json`,
/// `build/bof3-names.json` or `data/bof3-names.json` in the working directory and its parents,
/// then in the executable's directory and its parents. `None` when none exists.
#[must_use]
pub fn default_names_path() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("BOF3_NAMES").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(value));
    }
    let cwd = std::env::current_dir().ok();
    let exe = std::env::current_exe().ok().and_then(|path| path.parent().map(Path::to_path_buf));
    let bases = cwd.iter().flat_map(|dir| dir.ancestors().take(6)).chain(exe.iter().flat_map(|dir| dir.ancestors().take(4)));
    bases.flat_map(|base| [base.join(DEFAULT_NAMES), base.join("build").join(DEFAULT_NAMES), base.join("data").join(DEFAULT_NAMES)]).find(|path| path.is_file())
}

/// The names file `--names` selects, or else the default one. A file that was asked for must
/// read; a default file that does not read gives the reason as the second value and no names.
#[must_use]
pub fn load_names(explicit: Option<&Path>) -> (Names, Option<String>) {
    let (path, asked) = match explicit {
        Some(path) => (Some(path.to_path_buf()), true),
        None => (default_names_path(), false),
    };
    let Some(path) = path else { return (Names::default(), None) };
    match Names::read(&path) {
        Ok(names) => (names, None),
        Err(error) => {
            let kind = if asked { "names file" } else { "default names file" };
            (Names::default(), Some(format!("cannot read the {kind} {}: {error}", path.display())))
        }
    }
}

/// The folder exports go into by default: the user's Music folder below `Breath of Fire III`,
/// else `./exports` when no home is known.
pub fn default_export_dir() -> PathBuf {
    let folder = Path::new("Breath of Fire III");
    #[cfg(target_os = "linux")]
    if let Some(music) = std::env::var_os("XDG_MUSIC_DIR").filter(|value| !value.is_empty()) {
        return PathBuf::from(music).join(folder);
    }
    if let Some(home) = home_dir() {
        return home.join("Music").join(folder);
    }
    PathBuf::from("exports")
}

#[cfg(windows)]
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").filter(|value| !value.is_empty()).map(PathBuf::from)
}

#[cfg(not(windows))]
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").filter(|value| !value.is_empty()).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bof3player::catalog::Naming;

    #[test]
    fn an_explicit_file_overrides_the_default_and_a_bad_one_reports_why() {
        let dir = std::env::temp_dir().join(format!("bof3player-names-override-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("mine.json");
        std::fs::write(&file, r#"{"version":1,"music":{"music:A#0":{"title":"Mine","source":"data","evidence":"e"}}}"#).unwrap();
        let (names, failure) = load_names(Some(&file));
        assert!(failure.is_none());
        assert_eq!(names.music_title("A", 0), Some("Mine"));

        let missing = dir.join("absent.json");
        let (names, failure) = load_names(Some(&missing));
        let message = failure.expect("a file that was asked for must read");
        assert!(message.contains("names file") && message.contains("absent.json"), "{message}");
        assert!(names.music().is_empty());

        std::fs::write(&file, r#"{"version":9}"#).unwrap();
        let message = load_names(Some(&file)).1.expect("an unsupported version is reported");
        assert!(message.contains("unsupported names version 9"), "{message}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
