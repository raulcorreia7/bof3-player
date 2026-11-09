# psx-machine

A small PlayStation that runs game code from your own disc, records what the code tells the
hardware, and draws the frames the game draws. The crate knows no game: it has an R3000A interpreter and its memory map, and no BIOS
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
| GPU  | `GP0`, `GP1`, `GPUSTAT`, `GPUREAD`; a 1024×512 16-bit VRAM that draws are rasterized into, the CLUT cache, the displayed frame, and a log of every command |
| DMA  | seven channels in block, request and linked-list modes, `DPCR`, `DICR` and its interrupt                     |
| Timers | the NTSC beam (263 lines, vertical blank at line 240, `IRQ0`) and the three root counters                 |
| CD-ROM | the controller ports, the commands libcd sends, responses and interrupts, sectors to the CPU or DMA 3     |
| MDEC | quantization and scale tables, macroblock decoding to 4-, 8-, 15- and 24-bit pixels, DMA 0 in and DMA 1 out |
| SPU  | registers, 512 KB sound RAM through the transfer port and DMA 4, a log of key-ons and sound RAM writes        |
| Kernel | stand-ins for the `A0`, `B0` and `C0` functions the game calls, exceptions, events, threads, pads, `Exec`  |

The GPU log is the point of the crate. Each draw records its shape, vertices (before the drawing
offset), colours, UVs, texture page, CLUT, depth, texture window, the drawing offset and the
drawing area. Uploads record their rectangle and pixels; fills, copies, downloads and display
commands are logged too.

Uploads, fills and copies change VRAM, and so do draws: the GPU rasterizes every polygon, line
and rectangle into VRAM on the CPU, so VRAM after a frame holds what the console's GPU would
hold. `Machine::set_rasterize(false)` (or `Gpu::set_rasterize`) turns that off for a run that
only reads the log; the log is the same either way. `Machine::display` returns the picture the
GPU shows as RGBA: the display area of VRAM from `GP1(05h)`, as wide as psx-spx computes from
the `GP1(06h)` range and the dotclock, as tall as the `GP1(07h)` range (twice that in 480-line
interlace, both fields woven), in 15- or 24-bit colour, black while `GP1(03h)` has the display
off. Where the range places the picture on a television, and its border, are not part of the
frame.

The CPU frame is the contract. Nothing in this crate presents it: a front end uploads the RGBA
frame (or VRAM) as a texture each frame, through OpenGL ES (preferred), OpenGL or Vulkan, behind
its own cargo feature, so the log, VRAM readback, the frame, headless tests and CI never need a
GPU.

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
`gpu_log`, `vram`, `display`, `ram`, `key_ons` and `kernel().tty` read the results.
`Machine::new` gives a bare machine with no kernel, for tests on synthetic programs.

With `BOF3_DISC_DIR` naming a *Breath of Fire III* (USA) image, `cargo build --release -p
psx-machine --example boot`, then `target/release/examples/boot 600`, prints the frames run, GPU command counts and the VRAM
rectangles written. `tests/retail.rs` checks that the game's first 32×32 texture upload equals
the type-3 EMI entry it came from, and plays a new game with scripted buttons: Start through the
title screen and the opening, then Cross every second. The game loads the first area
(`WORLD00/AREA024`), fights the first boss battle (`BOSS/BOSS001`) and is back in the field at
frame 8054, where it waits for the player to walk. The run is deterministic, so the same buttons
give the same frames. The tty shows one `VSync: timeout`, at frame 481 just after
the logo returns: `VSync` gives its wait for the next vertical blank 32768 polls, about one frame
at the machine's flat two cycles per instruction, and the run carries on.

`tests/retail.rs` also renders the first area once the scripted game is back in it, on two runs
at once, and checks that the displayed frame has more than 64 colours and is the same on both
runs; it writes the frame to `target/tmp/retail/first-area.png` to look at, and commits nothing.

`PS1_TESTS_DIR`, naming an unpacked release of the MIT-licensed
[ps1-tests](https://github.com/JaCzekanski/ps1-tests) suite, turns on `tests/ps1_tests.rs`, which
runs the suite's self-checking DMA, GTE and GPU programs and compares the VRAM that its GPU
programs draw with the `vram.png` dump each ships, taken from a console.

## Rasterizer

The rasterizer is written from psx-spx: the render commands, the rendering attributes, the size
restriction, semi-transparency, modulation, dithering, texture windows, the mask bit, the drawing
area and offset, draw-to-display in 480-line interlace, and the CLUT cache. psx-spx does not give
the precision of the GPU's interpolation and line stepping, so those were fitted to the
ps1-tests reference images:

| What | Form | Evidence |
| ---- | ---- | -------- |
| Polygon colour and UV | a plane in fixed point with 12 fraction bits, gradients truncated toward zero, based at the leftmost vertex, rounded to the nearest value there | `gpu/triangle`, `gpu/uv-interpolation` match exactly; 32 or 16 bits, other roundings and the top vertex as base miss by one in places |
| Line position | 32 fraction bits, steps rounded away from zero, the start a little before the pixel centre on x, and on y when the line goes up | `gpu/lines`: both of its tie cases, steep and shallow |
| Line colour | 12 or more fraction bits, steps truncated toward zero | `gpu/lines` |
| X-flipped rectangle | the first column samples U+1, then U, U-1, … | `gpu/texture-flip`, measured with an even U only |
| VRAM copy | each source row is read before it is written | `gpu/vram-to-vram-overlap`, rows up to 16 pixels |
| CLUT cache | reloaded for a new CLUT position, for more entries (16 for 4-bit, 256 for 8-bit), and after `GP0(01h)` or `GP1(01h)` | `gpu/clut-cache` |

Results with release 158 (16 programs, all pass):

| Program | Checks | Result |
| ------- | ------ | ------ |
| `gpu/clipping`, `gpu/clut-cache`, `gpu/quad`, `gpu/rectangles`, `gpu/texture-flip`, `gpu/texture-overflow`, `gpu/triangle`, `gpu/uv-interpolation`, `gpu/vram-to-vram-overlap` | all of VRAM against `vram.png` | equal |
| `gpu/transparency` | its 320×240 screen; the rest of the dump is VRAM the console held before the program ran | equal |
| `gpu/lines` | all of VRAM | 64 pixels differ: the closing segments of its two Gouraud poly-lines, whose last colour the program never sets (it calls `setRGB2` twice), so the console drew whatever its stack held |
| `gpu/gp0-e1`, `gpu/mask-bit` | their own checks | 10 and 5 pass |

Not compared: `gpu/benchmark` (random triangles and live timings on screen), `gpu/bandwidth`
(timings), `gpu/animated-triangle` and `gpu/version-detect` (no reference).

Not modelled: the 2 KB texture cache (a draw that samples texels it has just overwritten can
differ from the console), drawing time (a draw lands in VRAM the moment its last word arrives),
2 MB VRAM, the interlace field per scanline (draw-to-display uses the field the machine flips
each vertical blank), and the v0 GPU. A vertex plus the drawing offset is not wrapped to 11 bits;
psx-spx does not say whether the hardware does.

Rasterizing costs about 12% more CPU cycles over the scripted retail run (8100 frames, 1.35
million draws, counted with `perf stat`); the interpreter takes the rest, and
`set_rasterize(false)` saves that share for a run that only reads the log.

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
