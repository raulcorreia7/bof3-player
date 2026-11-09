// SPDX-License-Identifier: MIT
//! Boots the disc in `$BOF3_DISC_DIR` (a folder holding the CUE sheet, or the image itself) and
//! prints what the game told the GPU.
//!
//! ```sh
//! cargo build --release -p psx-machine --example boot
//! BOF3_DISC_DIR=/path/to/disc target/release/examples/boot 600
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use psx_machine::Machine;
use psx_machine::gpu::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = std::env::var_os("BOF3_DISC_DIR").map(PathBuf::from) else {
        eprintln!("set BOF3_DISC_DIR to the folder that holds the disc image");
        std::process::exit(2);
    };
    let image = if dir.is_dir() { psx_cd::discover_image(&dir)? } else { dir };
    let frames: u64 = std::env::args().nth(1).and_then(|value| value.parse().ok()).unwrap_or(600);
    let mut machine = Machine::boot_image(&image)?;
    let result = machine.run_frames(frames, frames * 400_000);
    println!("frames run: {}", machine.frame());
    println!("instructions: {}", machine.instructions());
    println!("stopped: {result:?} at pc {:#010x}", machine.cpu().pc());
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for entry in machine.gpu_log() {
        let name = match &entry.command {
            Command::Draw(draw) => format!("draw {:#04x}", draw.opcode),
            Command::Fill { .. } => "fill".into(),
            Command::Upload { .. } => "upload".into(),
            Command::Download { .. } => "download".into(),
            Command::Copy { .. } => "copy".into(),
            Command::Display { command, .. } => format!("display {command:#04x}"),
        };
        *counts.entry(name).or_default() += 1;
    }
    println!("GPU commands:");
    for (name, count) in counts {
        println!("  {name}: {count}");
    }
    println!("VRAM rects written:");
    for entry in machine.gpu_log() {
        if let Command::Upload { rect, .. } = &entry.command {
            println!("  frame {}: {}x{} at ({}, {})", entry.frame, rect.width, rect.height, rect.x, rect.y);
        }
    }
    println!("kernel calls: {:?}", machine.kernel().calls);
    println!("tty:\n{}", machine.kernel().tty);
    Ok(())
}
