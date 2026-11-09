// SPDX-License-Identifier: MIT
//! Runs programs from JaCzekanski's MIT-licensed `ps1-tests` suite that report their own results,
//! when `PS1_TESTS_DIR` names an unpacked release (the folder holding `dma/`, `cpu/` and the rest).
//! Skips otherwise; the suite is not part of the repository.

use std::path::PathBuf;

use psx_machine::Machine;

/// Runs `program` for `frames` and returns what it printed.
fn run(program: &str, frames: u64) -> Option<String> {
    let root = PathBuf::from(std::env::var_os("PS1_TESTS_DIR")?);
    let bytes = std::fs::read(root.join(program)).expect("the suite has the program");
    let mut machine = Machine::with_kernel();
    machine.load_executable(&bytes, None).expect("a PS-X EXE");
    machine.run_frames(frames, frames * 2_000_000).expect("the program runs without a halt");
    Some(machine.kernel().tty.clone())
}

fn assert_passes(program: &str, passes: usize) {
    let Some(tty) = run(program, 120) else {
        eprintln!("PS1_TESTS_DIR is not set; skipping {program}");
        return;
    };
    let failures: Vec<&str> = tty.lines().filter(|line| line.starts_with("fail")).collect();
    assert!(failures.is_empty(), "{program}: {failures:?}");
    assert_eq!(tty.lines().filter(|line| line.starts_with("pass")).count(), passes, "{program}:\n{tty}");
}

#[test]
fn ordering_table_clear_suite_passes() {
    assert_passes("dma/otc-test/otc-test.exe", 15);
}

#[test]
fn dma_priority_control_suite_passes() {
    assert_passes("dma/dpcr/dpcr.exe", 2);
}

#[test]
fn gte_suite_passes() {
    let Some(tty) = run("gte/test-all/test-all.exe", 600) else {
        eprintln!("PS1_TESTS_DIR is not set; skipping");
        return;
    };
    assert!(tty.contains("Passed tests: 1150\nFailed tests: 0\n"), "{tty}");
}
