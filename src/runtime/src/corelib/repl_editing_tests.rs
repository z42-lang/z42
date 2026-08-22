//! Unit tests for the action-string → `Cmd` parser (add-repl-indent-editing).
//! The trigger-condition logic itself lives in z42 and is covered by the
//! `repl_editing` golden test; here we only pin the Rust-side translation to the
//! two redo-immune commands.

use super::*;
use rustyline::{Cmd, Movement};

#[test]
fn empty_is_default() {
    assert!(parse_action("").is_none());
}

#[test]
fn indent_maps_to_indent_wholeline() {
    match parse_action("indent") {
        Some(Cmd::Indent(Movement::WholeLine)) => {}
        other => panic!("expected Indent(WholeLine), got {other:?}"),
    }
}

#[test]
fn dedent_maps_to_dedent_wholeline() {
    match parse_action("dedent") {
        Some(Cmd::Dedent(Movement::WholeLine)) => {}
        other => panic!("expected Dedent(WholeLine), got {other:?}"),
    }
}

#[test]
fn unknown_action_is_default() {
    assert!(parse_action("kill 4").is_none());
    assert!(parse_action("frobnicate").is_none());
    assert!(parse_action("indent ").is_none()); // exact match only
}
