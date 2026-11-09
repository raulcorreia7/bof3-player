# Contributing

Issues and pull requests are welcome. All contributions follow the
[code of conduct](CODE_OF_CONDUCT.md). Ask questions in
[Discussions](https://github.com/raulcorreia7/bof3-player/discussions). `AGENTS.md` is optional
guidance for coding agents; people can ignore it.

## Set up

Install the tools in the [README requirements](README.md#requirements): Rust 1.88 or later (edition
2024), GNU make, `cargo-deny` and the Linux libraries. Then:

```sh
git clone https://github.com/raulcorreia7/bof3-player.git
cd bof3-player
git switch -c fix/short-name
```

## Checks

Run before every push:

```sh
make check
```

`make check` runs the checks CI runs:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo test --workspace --all-features
cargo check -p psx-formats -p emi -p psx-audio -p bof3player --no-default-features
cargo deny check
```

Install `cargo-deny` once with `cargo install cargo-deny --locked`. CI also builds with the minimum
supported Rust version (`rust-version` in `Cargo.toml`) and builds the release CLI, TUI and GUI. A
pull request that changes only docs runs the format checks only.

`psx-formats`, `psx-audio` and `emi` are `no_std` plus `alloc` without their `std` feature; the
`cargo check` line covers that.

## Test data

Tests build their inputs from constants, generated fixtures (the `fixtures` feature of
`psx-formats`) or files written to a temporary folder. A test that writes files for you to inspect
puts them in `target/tmp/<test>`.

Do not add game data, extracted assets, game text, build output (`target/`, `bin/`) or paths outside the repository. The only titles
are in `data/bof3-names.json`, each with a published source; library callers can also provide names
through a `Naming` implementation.

## Submit a change

1. Branch from `main` and keep each change focused.
2. Commit as `type(scope): subject`, imperative and lowercase, for example
   `fix(spu): clamp the reverb sum` or `docs(player): add a render example`.
3. Run `make check`, then push and open a pull request that describes the behavior and the checks.

Docs, comments and commit messages follow the workspace
[writing standard](https://github.com/raulcorreia7/bof3/blob/main/docs/standard.md#writing).

## License

Contributions are under the repository's [MIT license](LICENSE). Do not submit code you did not
write or cannot license.
