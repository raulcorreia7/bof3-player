# Provenance

The SPU model and the `psx-machine` hardware model are written from public hardware
documentation. They do not copy code from any emulator.

## Sources

The [psx-spx](https://psx-spx.consoledev.net/soundprocessingunitspu/) SPU specification is the
source for the hardware tables and formulas:

| Item                               | psx-spx section                |
| ---------------------------------- | ------------------------------ |
| Gaussian interpolation table (512) | 4-Point Gaussian Interpolation |
| ADPCM filter coefficients          | Pos/neg Tables                 |
| ADSR envelope and volume formulas  | SPU Volume and ADSR Generator  |
| Reverb presets                     | SPU Reverb Examples            |

`psx-audio/src/spu/gauss.rs` holds the Gaussian table as constants taken from psx-spx. The pitch
table is computed from `floor(0x1000 * 2^(i/192))`. [spu-model.md](spu-model.md) lists each table
and how it was checked.

## Clean-room rule

Write SPU code from the specification and from tests of the specification's values. Do not read,
copy or translate source code from an emulator, and do not paste an emulator's tables. A change that
adds a constant or a formula cites the psx-spx section it comes from.

## psx-machine

`psx-machine` follows the [psx-spx](https://psx-spx.consoledev.net/) hardware specification. No
emulator source was copied; GPL and non-commercial emulators (Mednafen/Beetle, PCSX-Redux,
Avocado, DuckStation) and rustation, whose licence is unclear, were not used as code.

| Part                      | psx-spx section                                        |
| ------------------------- | ------------------------------------------------------ |
| Instructions, delay slots | CPU Specifications; CPU Load/Store, Jump/Branch Opcodes |
| Exceptions, `SR`, `CAUSE` | COP0 - Exception Handling; COP0 - Register Summary     |
| Memory map and mirrors    | Memory Map                                             |
| `I_STAT`, `I_MASK`        | Interrupts                                             |
| GTE registers, commands, `FLAG` | Geometry Transformation Engine (GTE): Registers, Saturation, Coordinate, General Purpose and Color Calculation Commands |
| GTE perspective division  | GTE Division Inaccuracy (the UNR table is generated from its formula) |
| `GP0` command formats     | GPU Render Polygon, Line and Rectangle Commands; GPU Memory Transfer Commands |
| Page, CLUT, window bits   | GPU Rendering Attributes                               |
| Rasterizing draws         | GPU Render Polygon, Line and Rectangle Commands (fill rules, line end points, flips); GPU Rendering Attributes (size restriction, texture window, mask bit, drawing area and offset); GPU (MISC) (dithering table, semi-transparency, modulation, draw to display) |
| Texels and the CLUT cache | GPU Video Memory (VRAM); GPU Texture Caching           |
| `GP1`, `GPUSTAT`          | GPU Display Control Commands; GPU Status Register      |
| Displayed frame           | GPU Display Control Commands (`GP1(05h)`-`GP1(08h)`, the displayed width); GPU Video Memory (VRAM) (24-bit pixels); GPU Timings (dotclocks) |
| DMA modes, `DPCR`, `DICR` | DMA Channels                                           |
| Beam and root counters    | Timers; GPU Timings (263 lines of 3413 video cycles at 53.693175 MHz) |
| CD-ROM ports and commands | CDROM Controller I/O Ports; CDROM Controller Command Summary; CDROM - Response Timings |
| MDEC commands, decoding   | MDEC I/O Ports; MDEC Commands; MDEC Decompression (zigzag order, run-length blocks, reference IDCT, YUV to RGB) |
| SPU registers, transfers  | SPU Voice Registers; SPU Control and Status Register; SPU Memory Access |
| Kernel functions          | BIOS Function Summary; BIOS Event, Thread and Joypad Functions |
| Exception dispatch, chains | BIOS Interrupt/Exception Handling                     |
| Kernel RAM layout         | BIOS Memory Map; BIOS Patches (`_patch_pad` offsets)   |
| PS-X EXE, `SYSTEM.CNF`    | CDROM File Formats                                     |

The MIT-licensed [ps1-tests](https://github.com/JaCzekanski/ps1-tests) suite (release build 158)
checks the model from outside the repository; none of it is committed. `tests/ps1_tests.rs` runs
the DMA, GTE (`gte/test-all`, 1150 register and command checks) and GPU programs that report
their own results, and the GPU programs that ship a reference VRAM dump, when `PS1_TESTS_DIR`
names an unpacked release. The
MDEC programs' console dumps (`psx.log`) set the dequantization rounding (an arithmetic shift) and
the 4-bit output rounding, and their 15- and 24-bit frames match the reference VRAM images by eye.

psx-spx does not give the GPU's interpolation and line-stepping precision, the x-flip start, how
an overlapping VRAM copy reads, or exactly when the CLUT cache reloads. These were chosen by
matching the GPU programs' reference VRAM dumps (`vram.png`, from a console) pixel for pixel;
the [crate README](../psx-machine/README.md#rasterizer) lists each choice, its evidence and the
results. The general shape of the fixed-point line stepping and polygon planes is the one
Mednafen's GPU is known for, used as an idea only; no Mednafen code was read or copied. The
`gpu/gp0-e1` program's checks set when `GP0(E1h)` bit 11 can be written (only while `GP1(09h)`
bit 0 is set).
