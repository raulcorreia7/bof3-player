#include "bof3player_capi.h"

/* No game data: the success paths run in the Rust FFI test with synthetic bytes. */
int bof3player_capi_smoke(void) {
    if (bof3player_abi_version() != BOF3PLAYER_ABI_VERSION) return 1;
    if (psx_last_error_message() != 0) return 2;

    bof3player_library *library = 0;
    if (bof3player_library_open_disc_path(0, 0, &library) != PSX_STATUS_INVALID_ARGUMENT) return 3;
    if (library != 0) return 4;
    if (psx_last_error_message() == 0) return 5;

    bof3player_player *player = 0;
    if (bof3player_library_player_at(library, 0, &player) != PSX_STATUS_INVALID_ARGUMENT) return 6;
    if (player != 0) return 7;

    bof3player_item_info info = {0};
    if (bof3player_library_item_at(library, 0, &info) != PSX_STATUS_INVALID_ARGUMENT) return 8;
    bof3player_item_info_free(&info);
    psx_string_free(&info.id);

    psx_string string = {0};
    psx_string_free(&string);

    if (bof3player_player_set_loop(0, BOF3PLAYER_LOOP_OFF, 0) != PSX_STATUS_INVALID_ARGUMENT) return 9;
    if (bof3player_player_render_i16(0, 0, 0, 0) != PSX_STATUS_INVALID_ARGUMENT) return 10;

    bof3player_disc *disc = 0;
    if (bof3player_disc_open_path(0, 0, &disc) != PSX_STATUS_INVALID_ARGUMENT) return 11;
    if (disc != 0) return 12;

    psx_emi *archive = 0;
    if (psx_emi_parse(0, 0, &archive) != PSX_STATUS_FORMAT) return 13;
    if (archive != 0) return 14;

    bof3player_vab *bank = 0;
    if (bof3player_vab_parse(0, 0, &bank) != PSX_STATUS_FORMAT) return 15;
    if (bank != 0) return 16;

    psx_buffer buffer = {0};
    psx_buffer_free(&buffer);
    bof3player_pcm pcm = {0};
    bof3player_pcm_free(&pcm);

    bof3player_image image = {0};
    bof3player_image_free(&image);
    if (image.rgba.data != 0 || image.rgba.len != 0) return 18;

    if (bof3player_wav_encode(0, &buffer) != PSX_STATUS_INVALID_ARGUMENT) return 19;

    bof3player_machine *machine = 0;
    if (bof3player_machine_boot_disc_path(0, 0, &machine) != PSX_STATUS_INVALID_ARGUMENT) return 20;
    if (machine != 0) return 21;
    bof3player_stop stop = BOF3PLAYER_STOP_PC;
    if (bof3player_machine_run_frames(0, 1, 1, &stop) != PSX_STATUS_INVALID_ARGUMENT) return 22;
    if (bof3player_machine_display(0, &image) != PSX_STATUS_INVALID_ARGUMENT) return 23;

    bof3player_machine_free(machine);
    bof3player_image_free(0);
    bof3player_machine_free(0);

    bof3player_disc_free(disc);
    psx_emi_free(archive);
    bof3player_vab_free(bank);
    bof3player_disc_free(0);
    psx_emi_free(0);
    bof3player_vab_free(0);
    psx_buffer_free(0);
    bof3player_pcm_free(0);

    bof3player_player_free(player);
    bof3player_library_free(library);
    bof3player_library_free(0);
    bof3player_player_free(0);
    return 0;
}
