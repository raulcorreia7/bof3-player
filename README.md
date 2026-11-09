# bof3-player

[![CI](https://github.com/raulcorreia7/bof3-player/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/raulcorreia7/bof3-player/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A player for the music and sound of *Breath of Fire III* (PlayStation). It reads the game's sound
banks, sequences, XA streams and CD audio from your own disc image and renders them through a model
of the PlayStation sound chip (the SPU).

**No game data is included.** Dump your own US disc (Redump-verified CUE sheet and BIN tracks), or
use a folder extracted from it. This is a fan project, not affiliated with or endorsed by Capcom.
*Breath of Fire* is Capcom's trademark.

## Install

From a release: download an archive from
[Releases](https://github.com/raulcorreia7/bof3-player/releases), unpack it and run the tools from the
folder. Each archive contains all three tools and `data/bof3-names.json`:

- `bof3player-linux-x86_64.tar.gz` or `bof3player-linux-aarch64.tar.gz`
- `bof3player-macos-arm64.tar.gz` or `bof3player-macos-x86_64.tar.gz`
- `bof3player-windows-x86_64.zip`

Each archive has a matching `.sha256` file; check it with `sha256sum -c` (Linux) or
`shasum -a 256 -c` (macOS).

From source: you need Rust 1.88 or later and GNU make. On Linux, audio output needs the ALSA headers.
Ogg export builds a bundled libvorbis and needs a C compiler.
The workspace selects stable Rust; run `rustup update stable` to update an installed toolchain.

```sh
git clone https://github.com/raulcorreia7/bof3-player.git
cd bof3-player
make build   # release builds into target/release, linked into bin/
make check   # format, clippy, cargo deny, tests and no-default-features check
```

`make help` lists every target and the plain cargo commands (use those on Windows without make).

## Usage

| Tool             | What it does                                                                      |
| ---------------- | --------------------------------------------------------------------------------- |
| `bof3player`     | Command line: list, search, play, render to WAV, FLAC or Ogg, export MIDI, check. |
| `bof3player-tui` | Terminal browser: search, play with loop band and spectrum, export.               |
| `bof3player-gui` | Window with the same features. Linux, macOS and Windows.                          |

All three take `-i PATH` for the disc image or folder. Without it they search the working directory,
the program's folder and `$BOF3_GAME_DIR`; the browser and the window open a chooser when nothing is
found. The commands and switches are in [docs/player.md](docs/player.md) and in `--help`.

```sh
bof3player -i "Breath of Fire III.cue" doctor
bof3player -i "Breath of Fire III.cue" list --kind music
bof3player -i "Breath of Fire III.cue" render --format flac -o song.flac "music:BGMOPN#0"
bof3player-tui -i "Breath of Fire III.cue"
```

## Game data

`-i` takes any of these:

- a `.cue` sheet, with its `.bin` next to it;
- a raw disc image (`.bin` or `.iso`);
- a folder of files extracted from the disc. Use the folder that holds `SLUS_004.22` and `BIN/`,
  or the `BIN` folder itself.

To extract a folder, run `bof3 disc extract` from
[bof3-tools](https://github.com/raulcorreia7/bof3-tools). Then pass the folder:

```sh
bof3player -i ~/games/bof3-extracted list --kind music
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
| [emi](emi/README.md)                 | The EMI archive reader.                                                                              | [docs/crates.md](docs/crates.md#emi)         |

The crates are not on crates.io. Use them as path or git dependencies.
[docs/spu-model.md](docs/spu-model.md) lists the hardware documentation the SPU model follows. The
[bof3 workspace](https://github.com/raulcorreia7/bof3) links this project with the others.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md), the [code of conduct](CODE_OF_CONDUCT.md) and the
[security policy](SECURITY.md). Run `make check` before a pull request. Do not add game data,
extracted assets or game text.

## License

MIT. See [LICENSE](LICENSE). The SPU model's sources are in [docs/PROVENANCE.md](docs/PROVENANCE.md).
