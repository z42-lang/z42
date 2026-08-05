//! Unit tests for REPL bracket-balance detection (add-z42-repl) and
//! continuation-line auto-indent (add-repl-multiline-completion).

use super::{bracket_depth, continuation_indent};

#[test]
fn balanced_simple() {
    assert_eq!(bracket_depth("1 + 2"), 0);
    assert_eq!(bracket_depth("f(1, 2)"), 0);
    assert_eq!(bracket_depth("a[0] + b[1]"), 0);
}

#[test]
fn unclosed_opens_positive() {
    assert_eq!(bracket_depth("int sq(int n) {"), 1);
    assert_eq!(bracket_depth("f(g("), 2);
    assert_eq!(bracket_depth("x = {"), 1);
}

#[test]
fn closes_across_lines_balance() {
    // A block opened then closed nets to zero.
    assert_eq!(bracket_depth("int sq(int n) {\n  return n*n;\n}"), 0);
}

#[test]
fn brackets_in_string_ignored() {
    assert_eq!(bracket_depth("\"a(b\""), 0);
    assert_eq!(bracket_depth("var s = \"{[(\";"), 0);
    // escaped quote does not end the string
    assert_eq!(bracket_depth("\"a\\\"{\""), 0);
}

#[test]
fn brackets_in_char_ignored() {
    assert_eq!(bracket_depth("char c = '('"), 0);
    assert_eq!(bracket_depth("'\\''"), 0);
}

#[test]
fn brackets_in_line_comment_ignored() {
    assert_eq!(bracket_depth("f(1) // trailing ({["), 0);
    assert_eq!(bracket_depth("// only a comment ({["), 0);
}

#[test]
fn brackets_in_block_comment_ignored() {
    assert_eq!(bracket_depth("f(1) /* ({[ */"), 0);
    assert_eq!(bracket_depth("/* ( */ g()"), 0);
}

#[test]
fn division_not_treated_as_comment() {
    // A lone `/` (division) must not start a comment.
    assert_eq!(bracket_depth("a / b + (c)"), 0);
    assert_eq!(bracket_depth("(a / b"), 1);
}

#[test]
fn extra_closes_go_negative() {
    // More closes than opens → non-positive (caller stops reading).
    assert!(bracket_depth("})") < 0);
}

// ── continuation_indent: 4 spaces per still-open bracket level ────────────────

#[test]
fn indent_one_level_after_open_brace() {
    // `fn f() {` leaves depth 1 → body indents one level (4 spaces).
    assert_eq!(continuation_indent("int f() {"), "    ");
    assert_eq!(continuation_indent("class A {"), "    ");
}

#[test]
fn indent_scales_with_nesting_depth() {
    // Nested opens accumulate: `class A { int f() {` → depth 2 → 8 spaces.
    assert_eq!(continuation_indent("class A {\n    int f() {"), "        ");
    assert_eq!(continuation_indent("f(g("), "        ");
}

#[test]
fn indent_reflects_partial_close() {
    // Opening two then closing one leaves net depth 1 → one level.
    assert_eq!(continuation_indent("class A {\n    int f() {\n        return 1;\n    }"), "    ");
}

#[test]
fn indent_empty_when_balanced_or_over_closed() {
    // Balanced (depth 0) and over-closed (negative, clamped) → no indent.
    assert_eq!(continuation_indent("1 + 2"), "");
    assert_eq!(continuation_indent("f() {}"), "");
    assert_eq!(continuation_indent("}}"), "");
}

#[test]
fn indent_ignores_brackets_in_strings_and_comments() {
    // Bracket-depth's string/comment handling carries through to the indent.
    assert_eq!(continuation_indent("s = \"{[(\""), "");
    assert_eq!(continuation_indent("x() // {"), "");
}
