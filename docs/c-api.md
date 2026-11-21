# C API

The `capi/` crate exports the library crates as C symbols. It builds a shared library
(`libbof3player_capi.so`, `.dylib` or `.dll`) and a static library (`libbof3player_capi.a`). The
command lines, the GUI, the audio device and Ogg are not part of it.

The interface covers the read and playback path: disc images, EMI archives, VAB banks, VRAM
images, the catalog and player, WAVE output, and the machine.

The committed headers are `capi/include/bof3player_capi.h` and `psx-capi/include/psx_capi.h`.
`psx-capi` is the layer shared with the rebof3 C interface: the `psx_status` codes, the last-error
slot, the panic guard, `psx_buffer` and `psx_string` with their free functions, and the EMI handle
(`psx_emi_*`). `bof3player_capi.h` includes `psx_capi.h`, and the library exports both sets of
symbols. `BOF3PLAYER_ABI_VERSION` in the header equals `bof3player_abi_version()`; it is 2.

cbindgen generates the headers from the Rust declarations; each crate's `tests/header.rs` fails
when the committed copy is stale. Regenerate them with:

```sh
UPDATE_HEADER=1 cargo test -p psx-capi -p bof3player-capi --test header
```

The C smoke test in `capi/tests/smoke.c` compiles the header and calls the error paths. It runs in
`cargo test -p bof3player-capi --all-features`; all its input is synthetic.

## Build

```sh
cargo build --release -p bof3player-capi --locked
```

The artifacts are `target/release/libbof3player_capi.so` and `libbof3player_capi.a` on Linux,
`.dylib` and `.a` on macOS, and `.dll` and `.lib` on Windows. No game data is needed to build or to
link.

## Link

```c
#include "bof3player_capi.h"
```

```sh
cc -std=c11 example.c -Icapi/include -Ipsx-capi/include -Ltarget/release -lbof3player_capi -o example
LD_LIBRARY_PATH=target/release ./example "path/to/extracted" out.pcm
```

For a static link, ask rustc for the native libraries the crate needs:

```sh
cargo rustc -p bof3player-capi --release -- --print native-static-libs
```

## Ownership

- An open or parse call returns an opaque handle. Free it with the matching `_free`; NULL is a
  no-op.
- A `bof3player_item_info` owns three strings. Free the record with
  `bof3player_item_info_free`.
- A `psx_string` owns its bytes and has no terminator. Free it with
  `psx_string_free`. A NULL `data` with length 0 is the empty string.
- A player keeps its library alive. Either handle may be freed first.
- An owned result has one `_free` of the same shape: it takes a pointer to the record, accepts NULL
  and zeroes the record. A borrowed pointer (the last-error message) names its lifetime in the
  header.
- Flags are `bool`; out-parameters are named `out_<noun>`.
- Every fallible call returns a `psx_status`. On failure the output is zeroed and
  `psx_last_error_message` returns a NUL-terminated message for the calling thread, valid
  until the next call on that thread. A success clears it and returns NULL.
- A Rust panic is caught at every boundary and reported as `PSX_STATUS_PANIC`. It does not
  cross into C. Builds with `panic = "abort"` abort instead.
- Disc, EMI and player handles are safe to use from several threads; their calls serialize. The
  error is thread-local.

## Disc images

`bof3player_disc_open_path` opens a `.cue` sheet, a raw `.bin`, a cooked `.iso` or a folder that
holds one. It reads the ISO 9660 file table once; the entry calls use the cached table.
`bof3player_disc_entry_path` returns an owned UTF-8 path (a non-UTF-8 path is converted lossily),
and `bof3player_disc_read_entry` returns a copy of the whole file.

## EMI archives

`psx_emi_open_path` reads a file; `psx_emi_parse` takes bytes. Both copy the
container into the handle, so the caller may free its buffer after `parse`, and
`psx_emi_read_entry` always returns a copy. `psx_emi_entry_at` reports the size,
offset, RAM pointer and type of one table entry.

## VAB banks

`bof3player_vab_parse` takes the one-file `.vab` layout: the header followed by its body, which is
what the game's type-6 and type-7 EMI entries form when joined. `bof3player_vab_vag_count` and
`bof3player_vab_vag_at` describe the samples; index 0 is the null sample.
`bof3player_vab_vag_decode` returns mono 44,100 Hz PCM and gives 0 frames for the null sample.

## Images

`bof3player_vram_decode` takes a type-3 VRAM upload and its TOC load argument and returns RGBA in
the upload's own grid. Free the pixels with `bof3player_image_free`.

## Audio output

`bof3player_wav_encode` writes interleaved 16-bit PCM as a RIFF/WAVE file. It borrows the PCM
record and returns owned bytes; free them with `psx_buffer_free`.

## Machine

`bof3player_machine_boot_disc_path` reads `SYSTEM.CNF`, loads the executable and installs the
stand-in kernel. `bof3player_machine_run_frames` runs vertical blanks up to an instruction budget,
and `bof3player_machine_display` copies the displayed frame as RGBA. A halt returns
`PSX_STATUS_HALT`, writes `BOF3PLAYER_STOP_HALT` and puts the halt text in the last error.

## Example

The example lists the catalog of an extracted folder, picks the first playable music item and
writes its PCM as raw 16-bit samples. It needs the user's own disc dump.

```c
#include "bof3player_capi.h"
#include <stdio.h>
#include <string.h>

int main(int argc, char **argv) {
    if (argc != 3) return 2;
    bof3player_library *library = NULL;
    if (bof3player_library_open_folder_path(argv[1], strlen(argv[1]), &library) != PSX_STATUS_OK) {
        fprintf(stderr, "%s\n", psx_last_error_message());
        return 1;
    }
    size_t count = 0, index = 0;
    bof3player_library_item_count(library, &count);
    for (; index < count; index++) {
        bof3player_item_info info;
        if (bof3player_library_item_at(library, index, &info) != PSX_STATUS_OK) continue;
        int music = info.kind == BOF3PLAYER_KIND_MUSIC && info.playable;
        bof3player_item_info_free(&info);
        if (music) break;
    }
    bof3player_player *player = NULL;
    if (index == count || bof3player_library_player_at(library, index, &player) != PSX_STATUS_OK) {
        fprintf(stderr, "%s\n", psx_last_error_message());
        bof3player_library_free(library);
        return 1;
    }
    bof3player_format format = {0};
    bof3player_player_format(player, &format);
    FILE *out = fopen(argv[2], "wb");
    int16_t pcm[4096 * 2];
    size_t frames = 0;
    while (bof3player_player_render_i16(player, pcm, 4096, &frames) == PSX_STATUS_OK && frames > 0) {
        fwrite(pcm, sizeof(int16_t), frames * format.channels, out);
    }
    fclose(out);
    bof3player_player_free(player);
    bof3player_library_free(library);
    return 0;
}
```
