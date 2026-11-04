# Agent guide

bof3-player plays the music and sound of *Breath of Fire III* (PlayStation) from the user's own disc image.
It is a Cargo workspace (`core`, `emi`, `psx-audio`, `psx-cd`, `psx-formats`) with a CLI and a
Rust library. Read [README.md](README.md) first, then [CONTRIBUTING.md](CONTRIBUTING.md)
and the page in [docs/](docs/) for the area you change.

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

## Writing

Docs, comments, commit messages and pull requests follow one standard.

- **Current.** Present tense, no history.
- **What, not how.** Say what a thing does and why. The code shows how.
- **ASD-STE100** is the basis for technical text: short sentences, one instruction per sentence,
  active voice, one term for one concept, numbered steps for a procedure. Exact technical terms
  stay.
- **Orwell's six rules** ("Politics and the English Language", 1946):
  1. Never use a metaphor, simile or other figure of speech that you are used to seeing in print.
  2. Never use a long word where a short one will do.
  3. If it is possible to cut a word out, always cut it out.
  4. Never use the passive where you can use the active.
  5. Never use a foreign phrase, a scientific word or a jargon word if you can think of an everyday
     English equivalent.
  6. Break any of these rules sooner than say anything outright barbarous.
