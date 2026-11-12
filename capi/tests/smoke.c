#include "bof3player_capi.h"

/* No game data: the success paths run in the Rust FFI test with synthetic bytes. */
int bof3player_capi_smoke(void) {
    if (bof3player_abi_version() != 1) return 1;
    if (bof3player_last_error_message() != 0) return 2;

    bof3player_library *library = 0;
    if (bof3player_library_open_disc_path(0, 0, &library) != BOF3PLAYER_STATUS_INVALID_ARGUMENT) return 3;
    if (library != 0) return 4;
    if (bof3player_last_error_message() == 0) return 5;

    bof3player_player *player = 0;
    if (bof3player_library_player_at(library, 0, &player) != BOF3PLAYER_STATUS_INVALID_ARGUMENT) return 6;
    if (player != 0) return 7;

    bof3player_item_info info = {0};
    if (bof3player_library_item_at(library, 0, &info) != BOF3PLAYER_STATUS_INVALID_ARGUMENT) return 8;
    bof3player_item_info_free(&info);
    bof3player_string_free(&info.id);

    bof3player_string string = {0};
    bof3player_string_free(&string);

    if (bof3player_player_set_loop(0, BOF3PLAYER_LOOP_OFF, 0) != BOF3PLAYER_STATUS_INVALID_ARGUMENT) return 9;
    if (bof3player_player_render_i16(0, 0, 0, 0) != BOF3PLAYER_STATUS_INVALID_ARGUMENT) return 10;

    bof3player_disc *disc = 0;
    if (bof3player_disc_open_path(0, 0, &disc) != BOF3PLAYER_STATUS_INVALID_ARGUMENT) return 11;
    if (disc != 0) return 12;

    bof3player_emi *archive = 0;
    if (bof3player_emi_parse(0, 0, &archive) != BOF3PLAYER_STATUS_FORMAT) return 13;
    if (archive != 0) return 14;

    bof3player_vab *bank = 0;
    if (bof3player_vab_parse(0, 0, &bank) != BOF3PLAYER_STATUS_FORMAT) return 15;
    if (bank != 0) return 16;

    bof3player_buffer buffer = {0};
    bof3player_buffer_free(&buffer);
    bof3player_pcm pcm = {0};
    bof3player_pcm_free(&pcm);

    bof3player_disc_free(disc);
    bof3player_emi_free(archive);
    bof3player_vab_free(bank);
    bof3player_disc_free(0);
    bof3player_emi_free(0);
    bof3player_vab_free(0);
    bof3player_buffer_free(0);
    bof3player_pcm_free(0);

    bof3player_player_free(player);
    bof3player_library_free(library);
    bof3player_library_free(0);
    bof3player_player_free(0);
    return 0;
}
