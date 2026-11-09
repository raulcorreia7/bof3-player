# bof3-player

[![CI](https://github.com/raulcorreia7/bof3-player/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/raulcorreia7/bof3-player/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A player for the music and sound of *Breath of Fire III* (PlayStation). It reads the game's sound
banks, sequences, XA streams and CD audio from your own disc image and renders them through a model
of the PlayStation sound chip (the SPU).

**No game data is included.** Dump your own US disc (Redump-verified CUE sheet and BIN tracks), or
use a folder extracted from it. This is a fan project, not affiliated with or endorsed by Capcom.
*Breath of Fire* is Capcom's trademark.

## Requirements

| Need | Version | Get it |
| ---- | ------- | ------ |
| Rust (`rustup`, `cargo`) | 1.88 or later | [rustup.rs](https://rustup.rs); `rust-toolchain.toml` selects stable, and `rustup update stable` updates it |
| Git, GNU make, a C compiler, `pkg-config` | any recent | the distribution's packages (Ogg export builds a bundled libvorbis) |
| `cargo-deny` | any | `cargo install cargo-deny --locked` (the check gate only) |
| System libraries | Linux only | below |
| The disc | US release, serial SLUS-00422 | your own dump: a Redump-verified CUE sheet and its two BIN tracks in one folder, or a folder extracted with `bof3 disc extract` ([bof3-tools](https://github.com/raulcorreia7/bof3-tools)) |

| Distribution | Packages |
| ------------ | -------- |
| Debian, Ubuntu | `build-essential pkg-config libasound2-dev libfontconfig1-dev libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libvulkan-dev` |
| Arch | `base-devel alsa-lib fontconfig wayland libxkbcommon libxkbcommon-x11 libxcb vulkan-icd-loader` |

Audio output needs ALSA (`libasound2-dev`) for all three tools. `bof3player-gui` also needs the other
libraries and a Vulkan driver at run time; `make build-cli` needs only ALSA and the compiler.

Release archives are on [Releases](https://github.com/raulcorreia7/bof3-player/releases). Each holds all
three tools and `data/bof3-names.json`, and has a `.sha256` file to check with `sha256sum -c` (Linux) or
`shasum -a 256 -c` (macOS):

- `bof3player-linux-x86_64.tar.gz` or `bof3player-linux-aarch64.tar.gz`
- `bof3player-macos-arm64.tar.gz` or `bof3player-macos-x86_64.tar.gz`
- `bof3player-windows-x86_64.zip`

No game data is in this repository. Keep the disc dump outside Git.

## Set up

```sh
git clone https://github.com/raulcorreia7/bof3-player.git
cd bof3-player
```

## Build

```sh
make build
```

`make build` builds all three tools in release mode and links them into `bin/`. `make build-cli` builds
`bof3player` and `bof3player-tui`; `make build-gui` builds `bof3player-gui`. `make help` lists every
target and the plain cargo commands (use those on Windows without make).

## Run

Run from the repository root. `Breath of Fire III.cue` stands for your CUE sheet; its BIN tracks sit
next to it.

| Tool             | What it does                                                                      |
| ---------------- | --------------------------------------------------------------------------------- |
| `bof3player`     | Command line: list, search, play, render to WAV, FLAC or Ogg, export MIDI, check. |
| `bof3player-tui` | Terminal browser: search, play with loop band and spectrum, export.               |
| `bof3player-gui` | Window with the same features. Linux, macOS and Windows.                          |

```sh
bin/bof3player -i "Breath of Fire III.cue" doctor
bin/bof3player -i "Breath of Fire III.cue" list --kind music
bin/bof3player -i "Breath of Fire III.cue" render --format flac -o song.flac "music:BGMOPN#0"
bin/bof3player-tui -i "Breath of Fire III.cue"
bin/bof3player-gui -i "Breath of Fire III.cue"
```

All three take `-i PATH` for the disc image or folder. Without it they search the working directory,
the program's folder and `$BOF3_GAME_DIR`; the browser and the window open a chooser when nothing is
found. The commands and switches are in [docs/player.md](docs/player.md) and in `--help`.

## Game data

`-i` takes any of these:

- a `.cue` sheet, with its `.bin` next to it;
- a raw disc image (`.bin` or `.iso`);
- a folder of files extracted from the disc. Use the folder that holds `SLUS_004.22` and `BIN/`,
  or the `BIN` folder itself.

To extract a folder, run `bof3 disc extract` from
[bof3-tools](https://github.com/raulcorreia7/bof3-tools), then pass the folder:

```sh
bin/bof3player -i ~/games/bof3-extracted list --kind music
```

## Supported formats

| Input                                                     | Output                            |
| --------------------------------------------------------- | --------------------------------- |
| CUE and BIN disc image, raw `.bin` or `.iso`, or a folder | Catalog of music, sound and voice |
| VAB sound banks, SEP sequences, XA streams, CD-DA tracks  | WAV, FLAC or Ogg Vorbis audio     |
| SEP sequences                                             | Standard MIDI files               |

## Names file

The disc names no track, so `data/bof3-names.json` ships soundtrack titles, place labels and
duplicate marks, each with its source. The tools find it next to the program, in the working
directory or in `data/`. `--names FILE` or `$BOF3_NAMES` picks another file. See
[data/README.md](data/README.md) and [docs/player.md](docs/player.md#names-file).

## Use it as a library

| Part                                 | What it is                                                                                           | Guide                                        |
| ------------------------------------ | ---------------------------------------------------------------------------------------------------- | -------------------------------------------- |
| [core](core/README.md)               | `bof3player`: catalog, playback and export in Rust, and the `bof3player` and `bof3player-tui` tools. | [docs/rust-api.md](docs/rust-api.md)         |
| [psx-audio](psx-audio/README.md)     | SPU voices, XA and CD-DA playback, sources, WAV, FLAC and Ogg export.                                | [docs/crates.md](docs/crates.md#psx-audio)   |
| [psx-formats](psx-formats/README.md) | VAB, SEP, ADPCM, sector, VRAM, CLUT and TIM formats.                                                 | [docs/crates.md](docs/crates.md#psx-formats) |
| [psx-cd](psx-cd/README.md)           | ISO 9660 and cue-sheet disc images: read, extract, patch, rebuild.                                   | [docs/crates.md](docs/crates.md#psx-cd)      |
| [psx-machine](psx-machine/README.md) | A small PlayStation: R3000A interpreter, memory map, GPU command recording and frame drawing. | [docs/crates.md](docs/crates.md#psx-machine) |
| [emi](emi/README.md)                 | The EMI archive reader.                                                                              | [docs/crates.md](docs/crates.md#emi)         |

The crates are not on crates.io. Use them as path or git dependencies.
[docs/spu-model.md](docs/spu-model.md) lists the hardware documentation the SPU model follows.

## Check

```sh
make check
```

`make check` runs the format check, `cargo clippy`, the tests, a `--no-default-features` check and
`cargo deny`.

## Next

- [docs/player.md](docs/player.md): the tools, switches, keys and names file.
- [docs/rust-api.md](docs/rust-api.md), [docs/crates.md](docs/crates.md) and
  [docs/spu-model.md](docs/spu-model.md): the library and the SPU model.
- [CONTRIBUTING.md](CONTRIBUTING.md), the [code of conduct](CODE_OF_CONDUCT.md) and the
  [security policy](SECURITY.md).
- The [bof3 workspace](https://github.com/raulcorreia7/bof3) links this project with the others.

## License

MIT. See [LICENSE](LICENSE). The SPU model's sources are in [docs/PROVENANCE.md](docs/PROVENANCE.md).
