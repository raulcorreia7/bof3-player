# Build, test and check the bof3-player workspace. Needs cargo; GNU make.
# Cargo's target dir is target/ here; set CARGO_TARGET_DIR to move it.
# Release binaries are linked into bin/ (not moved with CARGO_TARGET_DIR).

.DEFAULT_GOAL := help

# Passed to `run-tui` and `run-gui`, e.g. make run-tui ARGS='-i "Breath of Fire III.cue"'.
ARGS ?=

BIN_DIR := bin
RELEASE_DIR := $(abspath $(or $(CARGO_TARGET_DIR),target))/release
CLI_BINS := bof3player bof3player-tui
GUI_BINS := bof3player-gui
BINS := $(CLI_BINS) $(GUI_BINS)

.PHONY: help build build-cli build-gui test format lint check run-tui run-gui clean

help:
	@echo "bof3-player: the player, its library and the PlayStation crates (cargo)"
	@echo ""
	@echo "Targets:"
	@echo "  help     List these targets (default)."
	@echo "  build    Release build of all player interfaces; links them into bin/."
	@echo "  build-cli Release build of bof3player and bof3player-tui; links them into bin/."
	@echo "  build-gui Release build of bof3player-gui; links it into bin/."
	@echo "  test     cargo test --workspace --all-features --locked."
	@echo "  format   cargo fmt; CHECK=1 only checks."
	@echo "  lint     cargo clippy -D warnings, then cargo deny."
	@echo "  check    format, lint, test and no-default-features check."
	@echo "  run-tui  Run the terminal browser; pass ARGS='...'."
	@echo "  run-gui  Run the window; pass ARGS='...'."
	@echo "  clean    cargo clean and remove the bin/ links."
	@echo ""
	@echo "Without make, run these cargo lines:"
	@echo "  cargo build --release --locked -p bof3player --features cli --bin bof3player --bin bof3player-tui"
	@echo "  cargo build --release --locked -p bof3player --features gui --bin bof3player-gui"
	@echo "  cargo test --workspace --all-features --locked"

build: build-cli build-gui

build-cli:
	cargo build --release --locked -p bof3player --features cli --bin bof3player --bin bof3player-tui
	@mkdir -p $(BIN_DIR)
	@for b in $(CLI_BINS); do ln -sf "$(RELEASE_DIR)/$$b" "$(BIN_DIR)/$$b"; done

build-gui:
	cargo build --release --locked -p bof3player --features gui --bin bof3player-gui
	@mkdir -p $(BIN_DIR)
	@for b in $(GUI_BINS); do ln -sf "$(RELEASE_DIR)/$$b" "$(BIN_DIR)/$$b"; done

test:
	cargo test --workspace --all-features --locked

# Format all crates; CHECK=1 verifies without writing.
format:
	cargo fmt --all $(if $(CHECK),--check)

lint:
	cargo clippy --workspace --all-features --all-targets --locked -- -D warnings
	@command -v cargo-deny >/dev/null 2>&1 || { echo "cargo-deny missing: cargo install cargo-deny" >&2; exit 1; }
	cargo deny check

check:
	$(MAKE) format CHECK=1
	$(MAKE) lint
	$(MAKE) test
	cargo check -p psx-formats -p emi -p psx-audio -p bof3player --no-default-features --locked

run-tui:
	cargo run --release --locked -p bof3player --features cli --bin bof3player-tui -- $(ARGS)

run-gui:
	cargo run --release --locked -p bof3player --features gui --bin bof3player-gui -- $(ARGS)

clean:
	cargo clean
	@for b in $(BINS); do rm -f "$(BIN_DIR)/$$b"; done
	@rmdir $(BIN_DIR) 2>/dev/null || true
