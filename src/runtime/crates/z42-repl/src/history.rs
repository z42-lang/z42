//! Cross-session REPL history file location. Kept dependency-free (no `dirs`
//! crate). Ported from the previous in-VM `repl.rs`. (extract-repl-native-cdylib)

/// `$HOME/.z42_history` (falls back to `%USERPROFILE%` on Windows). `None` when
/// neither is set — history then stays in-process only.
pub(crate) fn history_path() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|h| std::path::PathBuf::from(h).join(".z42_history"))
}
