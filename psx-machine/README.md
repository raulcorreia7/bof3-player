# psx-machine

A small PlayStation that runs game code from your own disc and records what the code tells the
hardware. The crate knows no game: it has an R3000A interpreter and its memory map, and no BIOS
image.

```sh
# from the repository root
cargo test -p psx-machine
```

Guide: [psx-machine](../docs/crates.md#psx-machine). The hardware sources are in
[PROVENANCE.md](../docs/PROVENANCE.md#psx-machine).

## What it models

| Part | Model                                                                                                       |
| ---- | ----------------------------------------------------------------------------------------------------------- |
| CPU  | R3000A: every MIPS I instruction, branch and load delay slots, coprocessor 0, exceptions and the interrupt line |
| GTE  | the coprocessor 2 registers; a GTE command stops the run with `Halt::Gte`                                   |
| Bus  | 2 MB RAM with its mirrors, 1 KB scratchpad, I/O ports, an empty BIOS region; KUSEG, KSEG0 and KSEG1           |
| GPU  | `GP0`, `GP1`, `GPUSTAT`, `GPUREAD`; a 1024×512 16-bit VRAM and a log of every command                       |
| DMA  | seven channels in block, request and linked-list modes, `DPCR`, `DICR` and its interrupt                     |

The GPU log is the point of the crate. Each draw records its shape, vertices (before the drawing
offset), colours, UVs, texture page, CLUT, depth, texture window, the drawing offset and the
drawing area. Uploads record their rectangle and pixels; fills, copies, downloads and display
commands are logged too. Uploads, fills and copies change VRAM; draws are recorded but not
rasterized, so VRAM holds what the game loaded, not what it drew. A renderer can come later behind
a feature; the log and VRAM do not depend on one.

Unmapped reads return 0 and unmapped writes are dropped; the machine raises no bus errors. A DMA
transfer runs to its end the moment it starts.

## Why the crate writes its own core

The machine needs hooks no packaged emulator offers: a bus that records GPU and SPU traffic, and
a BIOS replaced by stubs. These were checked first (November 2026):

| Candidate                | Licence | Why it is not used                                                                                                 |
| ------------------------ | ------- | ------------------------------------------------------------------------------------------------------------------ |
| `trapezoid-core` 0.3.0   | MIT     | The bus trait and the GTE are in private modules, so another crate cannot plug in its own bus; `Psx::new` requires a BIOS file. |
| `rsemu` 0.0.8            | MIT     | Needs Rust 1.91 (the workspace supports 1.88); a pre-1.0 framework with no GTE and no PlayStation board.             |
| `playastation` 0.1.0     | WTFPL   | Not on the `cargo deny` licence list.                                                                              |
| rustation, Mednafen/Beetle, PCSX-Redux, Avocado, DuckStation | GPL, unclear or non-commercial | Not MIT-compatible; nothing was copied from them. |

> A fan project, not affiliated with or endorsed by Capcom. *Breath of Fire* is Capcom's trademark. No game data is included; bring your own disc.
