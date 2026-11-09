// SPDX-License-Identifier: MIT
//! Rendering catalog items to audio files without a terminal or an audio device.

use std::collections::HashSet;
use std::path::Path;

use crate::catalog::{Filter, Item};
use crate::save::{ExportFormat, ExportOptions, Save, Step, absolute, file_stem};
use crate::{Library, PlayerOptions};

/// How [`render`] and [`render_item`] write an item.
#[derive(Clone, Copy, Debug)]
pub struct RenderStyle {
    /// The file format.
    pub format: ExportFormat,
    /// The Ogg Vorbis quality, 0 to 10.
    pub quality: f32,
    /// How many times a looping song plays; `None` is once.
    pub loops: Option<u32>,
    /// Stop after this many seconds; `None` renders the whole item.
    pub seconds: Option<f64>,
    /// Fade-out length in seconds.
    pub fade: f64,
    /// Settings for the player that renders the item.
    pub player: PlayerOptions,
}

impl Default for RenderStyle {
    /// WAV, the default Ogg quality, no loop count, no limit, no fade.
    fn default() -> Self {
        Self {
            format: ExportFormat::Wav,
            quality: crate::export::DEFAULT_OGG_QUALITY,
            loops: None,
            seconds: None,
            fade: 0.0,
            player: PlayerOptions::default(),
        }
    }
}

/// Render `item` into the file `path`, creating its folder, and report the result on standard error.
pub fn render_item(
    library: &'static Library<'static>,
    item: &Item,
    path: &Path,
    style: &RenderStyle,
) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let options = ExportOptions {
        loops: style.loops,
        fade: style.fade,
        format: style.format,
        quality: style.quality,
        seconds: style.seconds,
    };
    let mut save = Save::start_to(library, item, style.player, path.to_path_buf(), options)
        .map_err(|e| format!("{}: {e}", item.id))?;
    loop {
        match save.step() {
            Step::Progress => {}
            Step::Done => break,
            Step::Failed(error) => {
                return Err(format!("{} ({}): {error}", item.id, style.format.extension()));
            }
        }
    }
    eprintln!(
        "wrote {}: {:.1}s, {} Hz, {} ch",
        absolute(path).display(),
        save.rendered(),
        save.sample_rate(),
        save.channels()
    );
    Ok(())
}

/// Render every item that matches `query` and `filter` into `dir`, one file per item named after
/// its id; returns how many items failed. Unless `with_duplicates`, an item that repeats another
/// one's audio is skipped and only the canonical copy is written.
pub fn render(
    library: &'static Library<'static>,
    query: &str,
    dir: &Path,
    filter: &Filter<'_>,
    style: &RenderStyle,
    with_duplicates: bool,
) -> usize {
    let catalog = library.catalog();
    let groups = catalog.groups();
    let mut written: HashSet<String> = HashSet::new();
    let mut failed = 0;
    for hit in catalog.search(query, filter) {
        let repeat = !with_duplicates
            && groups.canonical_of(&hit.id).is_some_and(|canon| canon != hit.id || !written.insert(canon));
        if repeat {
            continue;
        }
        let path = dir.join(format!("{}.{}", file_stem(&hit.id), style.format.extension()));
        if let Err(error) = render_item(library, hit, &path, style) {
            eprintln!("player: {error}");
            failed += 1;
        }
    }
    failed
}
