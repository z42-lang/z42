//! REPL line editor builtins — back `Std.Repl.ReadLine` / `Std.Repl.ReadBlock`
//! used by the native interactive REPL (`z42i`, add-z42-repl).
//!
//! `__repl_readline(prompt)`  → one edited line (history, emacs keys, Ctrl-D EOF).
//! `__repl_readblock(prompt, cont)` → a bracket-balanced multi-line block: keeps
//! reading continuation lines (with the `cont` prompt) until `()[]{}` are
//! balanced, so pasting/typing `fn f() {` … `}` reads as a single unit.
//!
//! Return convention: `Value::Str` for a line/block; `Value::Null` on EOF
//! (Ctrl-D) or interrupt (Ctrl-C) so the z42 side can treat null as "exit".
//! An empty line is `Value::Str("")` — distinct from EOF.
//!
//! Host-only: rustyline backs the editor on non-wasm targets; wasm32 (where the
//! rustyline dep is cfg-gated out) falls back to a plain stdin read so the
//! builtins still resolve. The REPL itself is host-only (scripting-charter 2b).

use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

/// Extract argument `idx` as an owned prompt string (empty when absent/non-str).
fn prompt_arg(args: &[Value], idx: usize) -> String {
    match args.get(idx) {
        Some(Value::Str(s)) => s.to_string(),
        _ => String::new(),
    }
}

/// `__repl_readline(prompt: string) -> string?` — read one edited line.
/// Returns null on Ctrl-D (EOF) / Ctrl-C (interrupt).
pub fn builtin_repl_readline(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let prompt = prompt_arg(args, 0);
    read_one_line(&prompt)
}

/// `__repl_readblock(prompt: string, cont: string) -> string?` — read a
/// bracket-balanced multi-line block. Returns null if the very first line is EOF.
/// EOF encountered mid-block returns the (possibly unbalanced) text read so far,
/// leaving the final balance judgment to the caller's classifier.
pub fn builtin_repl_readblock(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let prompt = prompt_arg(args, 0);
    let cont = prompt_arg(args, 1);
    let mut buf = match read_one_line(&prompt)? {
        Value::Str(s) => s.to_string(),
        _ => return Ok(Value::Null), // EOF on first line
    };
    while bracket_depth(&buf) > 0 {
        match read_one_line(&cont)? {
            Value::Str(s) => {
                buf.push('\n');
                buf.push_str(&s);
            }
            _ => break, // EOF mid-block: hand back what we have
        }
    }
    Ok(Value::Str(buf.into()))
}

/// Net bracket depth of `s`, ignoring brackets inside string / char literals and
/// `//` line + `/* */` block comments. Positive = unclosed opens remain.
///
/// MVP scope: handles regular `"..."` / `'...'` (with `\` escape) and both
/// comment forms. Raw/triple-quoted string literals are NOT specially handled —
/// a rare REPL edge case; the z42-side classifier does the authoritative parse.
pub(crate) fn bracket_depth(s: &str) -> i64 {
    #[derive(PartialEq)]
    enum St {
        Normal,
        Str,
        Char,
        Line,
        Block,
    }
    let mut st = St::Normal;
    let mut depth: i64 = 0;
    let mut escaped = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match st {
            St::Normal => match c {
                '/' if chars.peek() == Some(&'/') => {
                    chars.next();
                    st = St::Line;
                }
                '/' if chars.peek() == Some(&'*') => {
                    chars.next();
                    st = St::Block;
                }
                '"' => st = St::Str,
                '\'' => st = St::Char,
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                _ => {}
            },
            St::Str => {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    st = St::Normal;
                }
            }
            St::Char => {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '\'' {
                    st = St::Normal;
                }
            }
            St::Line => {
                if c == '\n' {
                    st = St::Normal;
                }
            }
            St::Block => {
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    st = St::Normal;
                }
            }
        }
    }
    depth
}

// ── Line source: rustyline on host, plain stdin on wasm / when unavailable ──

#[cfg(not(target_arch = "wasm32"))]
fn read_one_line(prompt: &str) -> Result<Value> {
    use parking_lot::Mutex;
    use rustyline::error::ReadlineError;
    use rustyline::DefaultEditor;
    use std::sync::OnceLock;

    // One editor for the process → shared history across calls. Lazily created;
    // if rustyline can't init (e.g. no tty), we fall back to plain stdin.
    static EDITOR: OnceLock<Mutex<Option<DefaultEditor>>> = OnceLock::new();
    let cell = EDITOR.get_or_init(|| Mutex::new(None));
    let mut guard = cell.lock();
    if guard.is_none() {
        if let Ok(ed) = DefaultEditor::new() {
            *guard = Some(ed);
        }
    }
    match guard.as_mut() {
        Some(ed) => match ed.readline(prompt) {
            Ok(line) => {
                let _ = ed.add_history_entry(line.as_str());
                Ok(Value::Str(line.into()))
            }
            Err(ReadlineError::Eof) | Err(ReadlineError::Interrupted) => Ok(Value::Null),
            Err(e) => bail!("__repl_readline: {e}"),
        },
        None => plain_readline(prompt),
    }
}

#[cfg(target_arch = "wasm32")]
fn read_one_line(prompt: &str) -> Result<Value> {
    plain_readline(prompt)
}

/// Fallback: print the prompt to stderr, read one line from stdin. EOF → null.
fn plain_readline(prompt: &str) -> Result<Value> {
    use std::io::Write;
    let mut err = std::io::stderr();
    let _ = err.write_all(prompt.as_bytes());
    let _ = err.flush();
    let mut line = String::new();
    let n = std::io::stdin().read_line(&mut line)?;
    if n == 0 {
        return Ok(Value::Null); // EOF
    }
    Ok(Value::Str(line.trim_end_matches(['\n', '\r']).to_string().into()))
}

#[cfg(test)]
#[path = "repl_tests.rs"]
mod repl_tests;
