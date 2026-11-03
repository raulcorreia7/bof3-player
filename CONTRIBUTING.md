# Contributing

Issues and pull requests are welcome. All contributions follow the
[code of conduct](CODE_OF_CONDUCT.md). Ask questions in
[Discussions](https://github.com/raulcorreia7/bof3-player/discussions). `AGENTS.md` is optional
guidance for coding agents; people can ignore it.

## Set up

You need Git, GNU make and Rust 1.88 or later (edition 2024). On Linux, audio output needs the
ALSA headers. The [README](README.md) shows how to build and run the tools.

```sh
git clone https://github.com/raulcorreia7/bof3-player.git
cd bof3-player
make check
```

## Checks

`make check` runs the checks CI runs:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo test --workspace --all-features
cargo deny check
```

Install `cargo-deny` once with `cargo install cargo-deny --locked`. CI also builds with the minimum
supported Rust version (`rust-version` in `Cargo.toml`).

`psx-formats`, `psx-audio` and `emi` are `no_std` plus `alloc` without their `std` feature. If you
change them, also run `cargo check -p psx-formats --no-default-features`.

## Test data

Tests build their inputs from constants, generated fixtures (the `fixtures` feature of
`psx-formats`) or files written to a temporary folder. A test that writes files for you to inspect
puts them in `target/tmp/<test>`.

Do not add game data, extracted assets, game text or paths outside the repository. The only titles
are in `data/bof3-names.json`, each with a published source; library callers can also provide names
through a `Naming` implementation.

## Submit a change

Create a branch and keep each change focused. Use commit subjects such as
`fix(spu): clamp the reverb sum` or `docs(player): add a render example`. Describe the behavior and
the checks in the pull request.

## License

Contributions are under the repository's [MIT license](LICENSE). Do not submit code you did not
write or cannot license.
