# Provenance

The SPU model is written from public hardware documentation. It does not copy code from any
emulator.

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
