// SPDX-License-Identifier: MIT
//! The command line: list, search, play, render, export and check the game's audio.

use std::process::ExitCode;

fn main() -> ExitCode {
    let (names, failure) = bof3player::names::load(None);
    if let Some(failure) = failure {
        eprintln!("bof3player: {failure}; titles are not shown");
    }
    bof3player::cli::main("bof3player", std::env::args_os().skip(1).collect(), &names)
}
