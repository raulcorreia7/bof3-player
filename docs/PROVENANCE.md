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
| `GP0` command formats     | GPU Render Polygon, Line and Rectangle Commands; GPU Memory Transfer Commands |
| Page, CLUT, window bits   | GPU Rendering Attributes                               |
| `GP1`, `GPUSTAT`          | GPU Display Control Commands; GPU Status Register      |
| DMA modes, `DPCR`, `DICR` | DMA Channels                                           |
