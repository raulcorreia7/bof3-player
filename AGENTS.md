# Agent guide

bof3-player plays the music and sound of *Breath of Fire III* (PlayStation) from the user's own disc image.
It is a Cargo workspace (`core`, `emi`, `psx-audio`, `psx-cd`, `psx-formats`) with a CLI and a
Rust library. Read [README.md](README.md) first, then [CONTRIBUTING.md](CONTRIBUTING.md),
then the doc for the area you change.

## Where things are

| Path                   | What it holds                                                          | Doc                                                     |
| ---------------------- | ---------------------------------------------------------------------- | ------------------------------------------------------- |
| `core/`                | The `bof3player` crate and CLI: catalog, sources, playback, rendering  | [docs/rust-api.md](docs/rust-api.md), [core/README.md](core/README.md) |
| `core/` binaries       | `bof3player`, `bof3player-tui`, `bof3player-gui` and how to run them  | [docs/player.md](docs/player.md)                        |
| `psx-audio/`           | SPU voice engine, XA and CD-DA playback, WAV, FLAC and Ogg output      | [docs/spu-model.md](docs/spu-model.md), [psx-audio/README.md](psx-audio/README.md) |
| `psx-formats/`         | SPU and XA ADPCM, VAB, SEP and other PlayStation file formats          | [psx-formats/README.md](psx-formats/README.md)          |
| `psx-cd/`              | ISO 9660, Mode 2 XA sectors, CUE/BIN images                            | [psx-cd/README.md](psx-cd/README.md)                    |
| `emi/`                 | The EMI archive reader                                                 | [emi/README.md](emi/README.md)                          |
| `data/`                | The bundled catalog names and their sources                            | [data/README.md](data/README.md)                        |
| all crates             | How the crates depend on each other                                    | [docs/crates.md](docs/crates.md)                        |
| SPU evidence           | Where the SPU model's behaviour comes from                             | [docs/PROVENANCE.md](docs/PROVENANCE.md)                |

## Build and test

Rust 1.88 or later, edition 2024. Run from this folder:

```sh
cargo build --release -p bof3player --features cli --bin bof3player
cargo test --workspace --all-features
```

## Gate

Run before every report; CI runs the same checks:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo test --workspace --all-features
cargo check -p psx-formats -p emi -p psx-audio -p bof3player --no-default-features
cargo deny check
```

## Rules

- The repository holds no game data, extracted assets or game text. The only titles are in
  `data/bof3-names.json`, each with a source. Tests use constants, generated fixtures or a
  temporary folder (`target/tmp/<test>`).
- A task's `writes` list is the grant. Anything else is a `needs:` line in the report.
- Commit as `type(scope): subject`, imperative and lowercase. Add no AI attribution. Never amend,
  force-push or rewrite history.

## Index

- Skills: none; this guide is the entry point.
- Tools: `bof3player --help` and [docs/player.md](docs/player.md).
- Docs: [docs/](docs/) (table above) and each crate's `README.md`.
- CI: [.github/workflows/ci.yml](.github/workflows/ci.yml) runs the gate above;
  [release.yml](.github/workflows/release.yml) builds the release archives.
- Wiki (game and format facts): [site](https://raulcorreia7.github.io/bof3-wiki/). For a model: [llms.txt](https://raulcorreia7.github.io/bof3-wiki/llms.txt) (index), [llms-small.txt](https://raulcorreia7.github.io/bof3-wiki/llms-small.txt) (compact, for a small context), [llms-full.txt](https://raulcorreia7.github.io/bof3-wiki/llms-full.txt) (everything).
- Project standard: [docs/standard.md](https://github.com/raulcorreia7/bof3/blob/main/docs/standard.md).

## Writing

Docs, comments, commit messages and pull requests follow the [project standard](https://github.com/raulcorreia7/bof3/blob/main/docs/standard.md#writing): present tense, what and why instead of how, ASD-STE100 and Orwell's six rules.
Comments say what, never how: one line, only where a name cannot say it ([standard](https://github.com/raulcorreia7/bof3/blob/main/docs/standard.md#comments)).
