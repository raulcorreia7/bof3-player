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
| GTE  | every register and command, flags and saturation, the hardware's reciprocal for the perspective division; no cycle counts |
| Bus  | 2 MB RAM with its mirrors, 1 KB scratchpad, I/O ports, an empty BIOS region; KUSEG, KSEG0 and KSEG1           |
| GPU  | `GP0`, `GP1`, `GPUSTAT`, `GPUREAD`; a 1024×512 16-bit VRAM and a log of every command                       |
| DMA  | seven channels in block, request and linked-list modes, `DPCR`, `DICR` and its interrupt                     |
| Timers | the NTSC beam (263 lines, vertical blank at line 240, `IRQ0`) and the three root counters                 |
| CD-ROM | the controller ports, the commands libcd sends, responses and interrupts, sectors to the CPU or DMA 3     |
| MDEC | quantization and scale tables, macroblock decoding to 4-, 8-, 15- and 24-bit pixels, DMA 0 in and DMA 1 out |
| SPU  | registers, 512 KB sound RAM through the transfer port and DMA 4, a log of key-ons and sound RAM writes        |
| Kernel | stand-ins for the `A0`, `B0` and `C0` functions the game calls, exceptions, events, threads, pads, `Exec`  |

The GPU log is the point of the crate. Each draw records its shape, vertices (before the drawing
offset), colours, UVs, texture page, CLUT, depth, texture window, the drawing offset and the
drawing area. Uploads record their rectangle and pixels; fills, copies, downloads and display
commands are logged too. Uploads, fills and copies change VRAM; draws are recorded but not
rasterized, so VRAM holds what the game loaded, not what it drew. A renderer, if one is added,
goes behind a cargo feature (OpenGL ES through `glow` is the preferred backend), so the log, VRAM
readback, headless tests and CI never need a GPU.

The SPU log records each key-on with the voice's start address, loop address, pitch, ADSR and
volume, the fields `psx_audio::spu::VoiceSetup` takes; together with `Spu::ram` it is enough to
render the sounds with `psx-audio`. The machine itself mixes no audio, and CD-DA and XA sectors are
counted, not played.

Unmapped reads return 0 and unmapped writes are dropped; the machine raises no bus errors. A DMA
transfer runs to its end the moment it starts; MDEC output waits until the decoder holds the whole
transfer. The MDEC decodes a command as soon as its last parameter arrives.

## Running code

`Machine::boot_image(path)` opens a disc image, reads `SYSTEM.CNF`, loads the boot executable and
points the CPU at its entry. `run_frames`, `run_until_pc` and `run_until` (any condition on the
machine) run it; a `Halt` names what stopped it, such as a kernel function with no stand-in.
`gpu_log`, `vram`, `ram`, `key_ons` and `kernel().tty` read the results.
`Machine::new` gives a bare machine with no kernel, for tests on synthetic programs.

With `BOF3_DISC_DIR` naming a *Breath of Fire III* (USA) image, `cargo run --release -p
psx-machine --example boot -- 600` prints the frames run, GPU command counts and the VRAM
rectangles written, and `tests/retail.rs` checks that the game's first 32×32 texture upload equals
the type-3 EMI entry it came from. The game boots through its logo movie to the title screen and,
with Start pressed, to the title menu. The tty shows one `VSync: timeout`, at frame 481 just after
the logo returns: `VSync` gives its wait for the next vertical blank 32768 polls, about one frame
at the machine's flat two cycles per instruction, and the run carries on.

`PS1_TESTS_DIR`, naming an unpacked release of the MIT-licensed
[ps1-tests](https://github.com/JaCzekanski/ps1-tests) suite, turns on `tests/ps1_tests.rs`, which
runs the suite's self-checking DMA and GTE programs.

## No BIOS image

The kernel is a set of stand-ins written from psx-spx, added as the game calls them. Its tables and
control blocks sit in RAM where the BIOS keeps them (function tables at 0x200, 0x674 and 0x874, the
table of tables at 0x100, thread control blocks), because PsyQ library code reads and patches them:
`_patch_pad` derives the pad enable and disable routines from the `ChangeClearPAD` entry, and the
game's thread scheduler presets `SR` in the control blocks. Fetches from kernel RAM are traps the
machine handles; a fetch it has no routine for stops the run with `Halt::KernelPc`.

## The CD drive is hardware, not the libcd boundary

The plan was to serve the disc at the libcd call boundary. The game's code argued against it:

- the EMI loader reads through `CdControl(CdlReadN)` with a ready callback that pulls each sector
  with `CdGetSector`, not through `CdRead`;
- the logo program streams its movie through `StCdInterrupt`, which moves sectors with its own DMA
  calls (`dma_execute`) below the public functions;
- the logo program links its own copy of libcd at other addresses, so a symbol map per executable
  would be needed.

So the machine models the controller instead: its ports, the commands libcd sends, the response
and interrupt order, and sector reads at single or double speed. It needs no game addresses and no
symbol map, and both executables boot through it unchanged.

## Why the crate writes its own core

The machine needs hooks no packaged emulator offers: a bus that records GPU and SPU traffic, and
a BIOS replaced by stubs. These were checked first (November 2026):

| Candidate                | Licence | Why it is not used                                                                                                 |
| ------------------------ | ------- | ------------------------------------------------------------------------------------------------------------------ |
| `trapezoid-core` 0.3.0   | MIT     | The bus trait and the GTE are in private modules, so another crate cannot plug in its own bus; `Psx::new` requires a BIOS file. |
| `trapezoid-core` GTE     | MIT (Amjad Alsharafi) | Considered on its own for the GTE. Its history records that the author consulted DuckStation's (GPL) code for the RTPS/RTPT flag behaviour (upstream commit 72d2d2d), so the GTE is written here from psx-spx instead. |
| `rsemu` 0.0.8            | MIT     | Needs Rust 1.91 (the workspace supports 1.88); a pre-1.0 framework with no GTE and no PlayStation board.             |
| `playastation` 0.1.0     | WTFPL   | Not on the `cargo deny` licence list.                                                                              |
| rustation, Mednafen/Beetle, PCSX-Redux, Avocado, DuckStation | GPL, unclear or non-commercial | Not MIT-compatible; nothing was copied from them. |

> A fan project, not affiliated with or endorsed by Capcom. *Breath of Fire* is Capcom's trademark. No game data is included; bring your own disc.
