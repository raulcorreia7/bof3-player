// SPDX-License-Identifier: MIT
//! Compiles the C smoke test when the `smoke` feature is on.

fn main() {
    println!("cargo:rerun-if-changed=tests/smoke.c");
    println!("cargo:rerun-if-changed=include/bof3player_capi.h");
    #[cfg(feature = "smoke")]
    {
        cc::Build::new().file("tests/smoke.c").include("include").warnings(true).compile("bof3player_capi_smoke");
    }
}
