// SPDX-License-Identifier: MIT
//! Checks that the committed header is the one cbindgen generates.

use std::path::Path;

#[test]
fn header_is_current() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let bindings = cbindgen::generate(root).expect("cbindgen generates the header");
    let mut output = Vec::new();
    bindings.write(&mut output);
    let generated = String::from_utf8(output).expect("cbindgen writes UTF-8");
    let header = root.join("include/bof3player_capi.h");
    if std::env::var_os("UPDATE_HEADER").is_some() {
        std::fs::create_dir_all(header.parent().unwrap()).unwrap();
        std::fs::write(&header, &generated).unwrap();
    }
    // A Windows checkout may hold the text file with CRLF; the generator always
    // writes LF.
    let committed = std::fs::read_to_string(&header).expect("the committed header exists").replace("\r\n", "\n");
    assert_eq!(generated, committed, "the header is stale; regenerate it with UPDATE_HEADER=1 cargo test -p bof3player-capi --test header");
}
