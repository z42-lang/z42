use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::Result;
use super::convert::arg_char;

/// True for Unicode whitespace characters (space, tab, CR, LF, NBSP, etc.).
/// args: [this: char]
/// New in simplify-string-stdlib (2026-04-24): backs script-side Trim/TrimStart/TrimEnd.
pub fn char_is_whitespace(c: char) -> bool {
    c.is_whitespace()
}

/// `Std.Char.IsWhiteSpace` 迁 ABI v2(P1-8 批次 10):Rust 的 Unicode 分类,
/// 脚本无法等价(shrink-primitive-native-interop 保留 native 的理由)。
#[z42_macros::builtins]
pub mod typed {
    #[builtin(name = "__char_is_whitespace", pure)]
    pub fn is_whitespace(c: char) -> bool { super::char_is_whitespace(c) }
}

// shrink-primitive-native-interop (2026-08-27): builtin_char_to_lower /
// builtin_char_to_upper removed — 它们本是 ASCII-only (`to_ascii_lowercase/uppercase`)，
// Std.Char.ToLower/.ToUpper 现在是等价的 ASCII 脚本。IsWhiteSpace 保留 native
// （Rust `char::is_whitespace()` 是真 Unicode 分类，脚本无法等价）。
