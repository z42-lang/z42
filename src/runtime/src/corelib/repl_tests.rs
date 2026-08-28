//! Unit tests for the REPL Tab-completion replacement span (`word_start`).
//!
//! Bracket-balance detection + continuation-indent computation moved to the script
//! layer (`Std.Scripting.Completeness`, sink-repl-indent-to-script) and are covered
//! by the z42 golden test `src/libraries/z42.scripting/tests/completeness/`. This file
//! keeps only the native-side `word_start` coverage.

use super::word_start;

// ── word_start: replacement span excludes `.` (fix-repl-completion-span-and-index) ──
// The bug: `.`-inclusive word_start made a member candidate (`WriteLine`) replace the
// whole `Console.Wr` span → wiping the receiver. It must stop at `.` so only the
// post-`.` prefix is replaced.

#[test]
fn word_start_member_stops_after_dot() {
    // "Console.Wr" → span starts at 'W' (index 8), so `WriteLine` replaces only "Wr".
    assert_eq!(word_start("Console.Wr", 10), 8);
    // Nested receiver: "a.b.Wr" → after the LAST dot (index 4).
    assert_eq!(word_start("a.b.Wr", 6), 4);
    // Just typed the dot: "Console." → span is the empty word right after the dot.
    assert_eq!(word_start("Console.", 8), 8);
}

#[test]
fn word_start_bare_identifier_spans_whole_word() {
    // Bare prefix "Con" → whole word (index 0), `Console` replaces "Con".
    assert_eq!(word_start("Con", 3), 0);
    // Leading text then a bare word: "x = Con" → start of "Con" (index 4).
    assert_eq!(word_start("x = Con", 7), 4);
    // Digits / underscore are part of the word; stops at the space.
    assert_eq!(word_start("foo_2", 5), 0);
}
