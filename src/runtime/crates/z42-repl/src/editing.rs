//! Indent-aware key editing for the REPL editor. The **decision logic lives in
//! z42** (`Std.Repl.ReplEditing.KeyEdit`, reached via the `key_edit` re-entrancy
//! callback); this file is the policy-free rustyline adapter: on each controlled
//! key it asks z42 for an action string and maps it to a redo-immune `Cmd`.
//! Ported verbatim from the previous in-VM `repl_editing.rs` (the VM side keeps
//! only the `key_edit_via_callback` re-entrancy + `__repl_set_key_editor`).
//! (extract-repl-native-cdylib)
//!
//! Action-string protocol (Rust is a dumb translator). Non-empty actions map to
//! **redo-immune** commands (rustyline replays a custom binding through
//! `redo(Some(1))`, which clobbers movement counts — so only count-free movements
//! (`Dedent(WholeLine)`) and payload-carrying `Insert(1, text)` survive):
//!   ""               → `None` (the key's default: Tab→complete, Backspace→delete 1, `}`→self-insert)
//!   "dedent"         → `Cmd::Dedent(WholeLine)`   (Backspace one-level dedent)
//!   "insert:<text>"  → `Cmd::Insert(1, text)`     (Tab grid-snap-ceil)
//!   "replace:<text>" → `Cmd::Replace(WholeLine, text)` (variable-width whole-line delete+insert;
//!                                                       `}` auto-dedent + Backspace floor — add-repl-rbrace-floor)
//!   "newline:<ind>"  → `Cmd::Insert(1, "\n"+ind)` (Enter on an incomplete buffer)
//!   "accept"         → `Cmd::AcceptLine`          (Enter on a complete buffer: submit)

use crate::call_key_edit;
use rustyline::{Cmd, ConditionalEventHandler, Event, EventContext, Movement, RepeatCount};

/// Translate a z42 action string into a rustyline `Cmd`; `None` = the key's default.
///
/// `"replace:<text>"` → `Cmd::Replace(Movement::WholeLine, Some(text))` (add-repl-rbrace-floor):
/// the redo-immune variable-width delete+insert used by `}` auto-dedent and Backspace floor.
/// `Replace` = `edit_kill(WholeLine)` (cursor homes to the logical line start) + `edit_insert_text`.
/// Upstream rustyline's `edit_insert_text` left `pos` at column 0 (breaking `} else {`); our
/// `[patch.crates-io]` fork advances it past the inserted text, so the cursor lands after
/// `<text>` (e.g. after the `}`). `WholeLine` carries no count and the text rides in the
/// payload, so `redo(Some(1))` replays it verbatim.
pub(crate) fn parse_action(s: &str) -> Option<Cmd> {
    match s {
        "dedent" => Some(Cmd::Dedent(Movement::WholeLine)),
        "accept" => Some(Cmd::AcceptLine),
        _ if s.starts_with("insert:") => Some(Cmd::Insert(1, s["insert:".len()..].to_string())),
        _ if s.starts_with("replace:") => Some(Cmd::Replace(
            Movement::WholeLine,
            Some(s["replace:".len()..].to_string()),
        )),
        _ if s.starts_with("newline:") => {
            Some(Cmd::Insert(1, format!("\n{}", &s["newline:".len()..])))
        }
        _ => None,
    }
}

/// Conditional handler bound to a controlled key (Backspace / Tab / Enter). Asks
/// the z42 key-editor (via the callback) for an action, then maps it to a `Cmd`.
/// Returns `None` (default behavior) when there is no editor / the callback errors
/// / the action is empty — so normal editing is never blocked.
pub struct KeyEditHandler {
    key: &'static str,
}

impl KeyEditHandler {
    pub fn new(key: &'static str) -> Self {
        Self { key }
    }
}

impl ConditionalEventHandler for KeyEditHandler {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        ectx: &EventContext,
    ) -> Option<Cmd> {
        match call_key_edit(self.key, ectx.line(), ectx.pos()) {
            // Enter at-end gate (the one cursor mechanic Rust owns): "accept" means
            // z42 judged the whole buffer complete. Submit only when the cursor is at
            // the very end (byte-robust for UTF-8); a mid-buffer Enter on a complete
            // buffer inserts a newline instead (accept_in_the_middle: false).
            Some(action) if action == "accept" && ectx.pos() != ectx.line().len() => {
                Some(Cmd::Insert(1, "\n".to_string()))
            }
            Some(action) => parse_action(&action),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_action;
    use rustyline::{Cmd, Movement};

    #[test]
    fn empty_is_default() {
        assert!(parse_action("").is_none());
    }

    #[test]
    fn dedent_maps_to_dedent_wholeline() {
        assert!(matches!(parse_action("dedent"), Some(Cmd::Dedent(Movement::WholeLine))));
    }

    #[test]
    fn insert_maps_to_insert_with_text() {
        match parse_action("insert:    ") {
            Some(Cmd::Insert(1, text)) => assert_eq!(text, "    "),
            other => panic!("expected Insert(1, 4 spaces), got {other:?}"),
        }
    }

    #[test]
    fn accept_maps_to_accept_line() {
        assert!(matches!(parse_action("accept"), Some(Cmd::AcceptLine)));
    }

    #[test]
    fn replace_maps_to_replace_wholeline_with_text() {
        // add-repl-rbrace-floor: `}` auto-dedent + Backspace floor drive a whole-line replace;
        // the patched rustyline leaves the cursor after the inserted text.
        match parse_action("replace:    }") {
            Some(Cmd::Replace(Movement::WholeLine, Some(text))) => assert_eq!(text, "    }"),
            other => panic!("expected Replace(WholeLine, Some(\"    }}\")), got {other:?}"),
        }
        // Floor to column 0 replaces the line with just the (empty) target indent.
        match parse_action("replace:") {
            Some(Cmd::Replace(Movement::WholeLine, Some(text))) => assert_eq!(text, ""),
            other => panic!("expected Replace(WholeLine, Some(\"\")), got {other:?}"),
        }
    }

    #[test]
    fn newline_maps_to_insert_newline_plus_indent() {
        match parse_action("newline:    ") {
            Some(Cmd::Insert(1, text)) => assert_eq!(text, "\n    "),
            other => panic!("expected Insert(1, newline+4), got {other:?}"),
        }
        match parse_action("newline:") {
            Some(Cmd::Insert(1, text)) => assert_eq!(text, "\n"),
            other => panic!("expected Insert(1, bare newline), got {other:?}"),
        }
    }

    #[test]
    fn unknown_is_default() {
        assert!(parse_action("frobnicate").is_none());
        assert!(parse_action("insert").is_none());
        assert!(parse_action("dedent ").is_none());
    }
}
