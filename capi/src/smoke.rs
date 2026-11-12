// SPDX-License-Identifier: MIT
//! Runs the C smoke test built by `build.rs`.
//!
//! A unit test, not an integration test: the C object is linked into the
//! library's test binary, which holds every exported symbol. An integration
//! test links the rlib and reaches the C symbols only through its own objects.

#[cfg(feature = "smoke")]
#[link(name = "bof3player_capi_smoke", kind = "static")]
unsafe extern "C" {
    fn bof3player_capi_smoke() -> i32;
}

#[test]
fn c_smoke() {
    #[cfg(feature = "smoke")]
    assert_eq!(unsafe { bof3player_capi_smoke() }, 0, "the C smoke test failed");
}
