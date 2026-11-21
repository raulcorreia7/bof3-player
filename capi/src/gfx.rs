// SPDX-License-Identifier: MIT
//! Image output: a type-3 VRAM upload as RGBA.

use std::ptr;

use psx_formats::gfx::vram;

use crate::error::{fail, guard, map_formats};
use crate::{bof3player_image, owned_buffer, psx_buffer, psx_status};
use psx_capi::input;

/// Decode a type-3 VRAM upload into RGBA.
///
/// `load_argument` is the entry's TOC load argument; it gives the upload's
/// column, row and rectangles per row. Free the pixels with
/// `bof3player_image_free`.
///
/// # Safety
///
/// `payload` points to `payload_len` readable bytes (or is NULL with length 0),
/// and `out_image` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bof3player_vram_decode(load_argument: u32, payload: *const u8, payload_len: usize, out_image: *mut bof3player_image) -> psx_status {
    if out_image.is_null() {
        return fail(psx_status::InvalidArgument, "out_image is NULL");
    }
    unsafe { *out_image = bof3player_image { width: 0, height: 0, rgba: psx_buffer { data: ptr::null_mut(), len: 0 } } };
    guard(|| {
        let payload = unsafe { input(payload, payload_len, "payload")? };
        let descriptor = emi::upload::descriptor(load_argument);
        let image = vram::upload_image(payload, descriptor).map_err(map_formats)?;
        unsafe { *out_image = bof3player_image { width: image.width, height: image.height, rgba: owned_buffer(image.rgba) } };
        Ok(())
    })
}
