//! `ReplHelper` — rustyline `Completer` / `Hinter` / `Highlighter` / `Validator`
//! for the REPL editor. Completion candidates come from the z42 session completer
//! via the `complete` re-entrancy callback (`crate::call_complete`); no z42 type
//! crosses the boundary. Ported verbatim from the previous in-VM `repl.rs`
//! `ReplHelper`, with `complete_via_callback` → `call_complete`. (extract-repl-native-cdylib)

use crate::call_complete;

pub struct ReplHelper {
    /// fish-style ghost from prior inputs; the fallback when no live identifier
    /// completion extends the current word.
    history: rustyline::hint::HistoryHinter,
}

impl ReplHelper {
    pub fn new() -> Self {
        Self { history: rustyline::hint::HistoryHinter::new() }
    }

    /// Inline completion hint for the word under the cursor: ask the session
    /// completer for candidates and ghost the suffix of the first one that
    /// *strictly extends* the typed word. Works for bare identifiers (`Con`→`sole`)
    /// and member context (`Console.W`→`riteLine`). Prefix-only filter means a
    /// non-extending list simply yields no hint — the ghost is never wrong.
    fn identifier_hint(&self, line: &str, pos: usize) -> Option<String> {
        let start = word_start(line, pos);
        let word = &line[start..pos];
        if word.is_empty() {
            return None;
        }
        let cands = call_complete(line, pos);
        cands
            .into_iter()
            .find(|c| c.len() > word.len() && c.starts_with(word))
            .map(|c| c[word.len()..].to_string())
    }
}

impl rustyline::completion::Completer for ReplHelper {
    type Candidate = String;
    fn complete(
        &self,
        line: &str,
        pos: usize,
        _rlctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<String>)> {
        // Replacement span starts at the identifier word (excludes `.`, so a member
        // candidate replaces only the post-`.` prefix). Matches the z42 completer's
        // `_wordStart`. Candidates come from the VM re-entrancy callback.
        let start = word_start(line, pos);
        let cands = call_complete(line, pos);
        if cands.is_empty() {
            Ok((pos, Vec::new()))
        } else {
            Ok((start, cands))
        }
    }
}

impl rustyline::hint::Hinter for ReplHelper {
    type Hint = String;
    fn hint(&self, line: &str, pos: usize, rlctx: &rustyline::Context<'_>) -> Option<String> {
        // Only at end-of-line on a non-empty buffer (rustyline's ghosting convention).
        if line.is_empty() || pos != line.len() {
            return None;
        }
        self.identifier_hint(line, pos)
            .or_else(|| self.history.hint(line, pos, rlctx))
    }
}

impl rustyline::highlight::Highlighter for ReplHelper {
    /// Render the inline hint dimmed (ANSI bright-black) so the ghost reads as a
    /// suggestion, not typed text.
    fn highlight_hint<'h>(&self, hint: &'h str) -> std::borrow::Cow<'h, str> {
        std::borrow::Cow::Owned(format!("\x1b[90m{hint}\x1b[0m"))
    }
}

impl rustyline::validate::Validator for ReplHelper {}
impl rustyline::Helper for ReplHelper {}

/// Start index of the identifier word ending at `pos` (letters/digits/`_`).
/// **Excludes `.`** so a member candidate replaces only the post-`.` prefix, not
/// the whole `receiver.prefix`. Matches the z42 side `_wordStart` (Completer.z42).
pub(crate) fn word_start(line: &str, pos: usize) -> usize {
    let bytes = line.as_bytes();
    let mut i = pos;
    while i > 0 {
        let c = bytes[i - 1];
        if c.is_ascii_alphanumeric() || c == b'_' {
            i -= 1;
        } else {
            break;
        }
    }
    i
}

#[cfg(test)]
mod tests {
    use super::word_start;

    #[test]
    fn word_start_excludes_dot_and_stops_at_boundary() {
        assert_eq!(word_start("Console.Wri", 11), 8); // post-`.` prefix only
        assert_eq!(word_start("Con", 3), 0);
        assert_eq!(word_start("  foo", 5), 2);
        assert_eq!(word_start("", 0), 0);
        assert_eq!(word_start("a.b.c", 5), 4);
    }
}
