//! Unit tests for REPL bracket-balance detection (add-z42-repl).

use super::bracket_depth;

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
