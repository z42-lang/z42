//! Unit tests for `stack_guard`. The overflow path end to end is covered by
//! `tests/stack_overflow_e2e.rs` (z42vm in a child process) and the host test
//! `invoke_deep_recursion_is_fatal`.

use super::*;

#[test]
fn margin_is_an_eighth_clamped() {
    assert_eq!(margin(512 * 1024), 256 * 1024);
    assert_eq!(margin(4 * 1024 * 1024), 512 * 1024);
    assert_eq!(margin(64 * 1024 * 1024), 1024 * 1024);
}

#[test]
fn limit_is_below_the_current_frame() {
    let l = limit();
    assert!(l == 1 || l < current_sp(), "limit {l:#x} sp {:#x}", current_sp());
}

#[test]
fn deep_trace_keeps_both_ends_innermost_first() {
    let frame = |name: &str| crate::exception::FrameSnapshot {
        func_name: name.into(), file: "".into(), line: 0, column: 0, offset: u32::MAX,
    };
    let mut frames = vec![frame("Main")];
    frames.extend((0..100).map(|i| frame(&format!("F{i}"))));
    let t = format_trace(&frames);
    let lines: Vec<&str> = t.lines().collect();
    assert_eq!(lines.first(), Some(&"  at F99"), "innermost frame first");
    assert_eq!(lines.last(), Some(&"  at Main"), "entry point last");
    assert!(t.contains("... 61 frames omitted ..."), "{t}");
}
