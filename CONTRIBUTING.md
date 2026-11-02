# Contributing

Issues and pull requests are welcome. All contributions follow the
[code of conduct](CODE_OF_CONDUCT.md). `AGENTS.md` is optional guidance for coding agents; people can
ignore it.

## Before a pull request

Run the checks CI runs, from the repository root:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo test --workspace --all-features
cargo deny check
```

`psx-formats`, `psx-audio` and `emi` are `no_std` plus `alloc` without their `std` feature. If you
change them, also run `cargo check -p psx-formats --no-default-features`.

Rust 1.88 or newer, edition 2024.

## Test data

Tests build their inputs from constants, generated fixtures (the `fixtures` feature of
`psx-formats`) or files written to a temporary folder. A test that writes files for you to inspect
puts them in `target/tmp/<test>`.

Do not add game data, extracted assets, game text or paths outside the repository. The only titles
are in `data/bof3-names.json`, each with a published source; library callers can also provide names
through a `Naming` implementation.

## License

Contributions are under the repository's [MIT license](LICENSE).
