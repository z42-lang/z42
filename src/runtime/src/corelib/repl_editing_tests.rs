//! Unit tests for the action-string → `Cmd` parser (add-repl-indent-editing +
//! add-repl-tab-grid-snap). The trigger-condition logic itself lives in z42 and is
//! covered by the `repl_editing` golden test; here we only pin the Rust-side
//! translation to the redo-immune commands.

use super::*;
use rustyline::{Cmd, Movement};

#[test]
fn empty_is_default() {
    assert!(parse_action("").is_none());
}

#[test]
fn dedent_maps_to_dedent_wholeline() {
    match parse_action("dedent") {
        Some(Cmd::Dedent(Movement::WholeLine)) => {}
        other => panic!("expected Dedent(WholeLine), got {other:?}"),
    }
}

#[test]
fn insert_maps_to_insert_with_text() {
    match parse_action("insert:  ") {
        Some(Cmd::Insert(1, text)) => assert_eq!(text, "  "),
        other => panic!("expected Insert(1, \"  \"), got {other:?}"),
    }
    // 4-space grid-snap.
    match parse_action("insert:    ") {
        Some(Cmd::Insert(1, text)) => assert_eq!(text, "    "),
        other => panic!("expected Insert(1, 4 spaces), got {other:?}"),
    }
}

#[test]
fn accept_maps_to_accept_line() {
    // add-repl-multiline-editing: Enter on a complete buffer submits. (The mid-buffer
    // at-end gate is applied in the handler, not parse_action.)
    match parse_action("accept") {
        Some(Cmd::AcceptLine) => {}
        other => panic!("expected AcceptLine, got {other:?}"),
    }
}

#[test]
fn newline_maps_to_insert_newline_plus_indent() {
    // add-repl-multiline-editing: Enter on an incomplete buffer inserts a newline plus
    // the continuation indent (redo-immune Insert, cursor after it).
    match parse_action("newline:    ") {
        Some(Cmd::Insert(1, text)) => assert_eq!(text, "\n    "),
        other => panic!("expected Insert(1, \"\\n    \"), got {other:?}"),
    }
    // Zero-indent continuation (depth 0) still inserts the bare newline.
    match parse_action("newline:") {
        Some(Cmd::Insert(1, text)) => assert_eq!(text, "\n"),
        other => panic!("expected Insert(1, \"\\n\"), got {other:?}"),
    }
}

#[test]
fn unknown_action_is_default() {
    assert!(parse_action("indent").is_none()); // dropped: Tab now uses insert:<delta>
    // `replace:` is intentionally NOT handled: the `}` / deep-misaligned-backspace
    // paths it would drive are Deferred (rustyline cursor-home limitation).
    assert!(parse_action("replace:    ").is_none());
    assert!(parse_action("kill 4").is_none());
    assert!(parse_action("frobnicate").is_none());
    assert!(parse_action("dedent ").is_none()); // exact match only for `dedent`
    assert!(parse_action("insert").is_none()); // needs the `:` prefix
    assert!(parse_action("accept ").is_none()); // exact match only for `accept`
    assert!(parse_action("newline").is_none()); // needs the `:` prefix
}
