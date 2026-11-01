//! Type-parameter **markers** inside runtime type names — the spelling a generic body uses
//! for a type argument it only learns at run time.
//!
//! A shared (erased) generic body cannot spell `T` in `o is T`, `new List<T>()` or
//! `catch (Err<T> e)`: the argument differs per call / per receiver. z42c writes a marker in
//! its place and the VM substitutes it from the same carriers `typeof(T)` reads:
//!
//! | marker               | bound by                                                       |
//! |----------------------|----------------------------------------------------------------|
//! | `$mta:N`             | method-level param `N`: the frame's `method_type_args[N]`      |
//! | `$cta:N@Owner`       | class-level param `N` of `Owner` (erased FQ name): the receiver (reg 0), walked to the level declaring `Owner` — `__class_type_arg` |
//!
//! Markers may sit anywhere a type name may (`Std.Collections.List<$cta:0@Demo.Bag>`,
//! `$mta:1[]`). `$mta:N` is the forwarding marker generic call sites already use
//! (`Foo<T>() { Bar<T>(); }`), so the two meet at one spelling.
//!
//! This module is the pure half — finding and substituting markers; what a marker resolves to
//! is the caller's (`interp::type_param_env`). A marker that cannot be resolved stays verbatim:
//! it reads as an open type argument (`dispatch::is_open_type_arg`), which is what the literal
//! `T` meant before.

/// Prefix of a method-level type-parameter marker (`$mta:N`).
pub const METHOD_MARKER: &str = "$mta:";
/// Prefix of a class-level type-parameter marker (`$cta:N@Owner`).
pub const CLASS_MARKER: &str = "$cta:";

/// One marker, parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker<'a> {
    /// Method-level type parameter `index`.
    Method(usize),
    /// Class-level type parameter `index` declared by `owner` (erased FQ class name).
    Class { index: usize, owner: &'a str },
}

/// Does `s` contain any marker? Cheap: names without a `$` answer on the first scan.
#[inline]
pub fn has_markers(s: &str) -> bool {
    s.contains('$') && (s.contains(METHOD_MARKER) || s.contains(CLASS_MARKER))
}

/// Does `s` contain a **method-level** marker? Those read `frame.method_type_args`, which a JIT
/// frame does not carry (see `jit::translate::unsupported`).
#[inline]
pub fn has_method_markers(s: &str) -> bool {
    s.contains(METHOD_MARKER)
}

/// A type name ends at one of these (the owner of a `$cta:` marker runs up to the next).
fn is_name_delim(c: char) -> bool {
    matches!(c, '<' | '>' | ',' | '[' | ']' | ' ' | '?')
}

/// Parse the marker at the start of `s`: `(marker, byte length)`.
pub fn parse_marker(s: &str) -> Option<(Marker<'_>, usize)> {
    let (rest, method) = if let Some(r) = s.strip_prefix(METHOD_MARKER) {
        (r, true)
    } else if let Some(r) = s.strip_prefix(CLASS_MARKER) {
        (r, false)
    } else {
        return None;
    };
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let index: usize = rest[..digits].parse().ok()?;
    let head = s.len() - rest.len() + digits;
    if method {
        return Some((Marker::Method(index), head));
    }
    let owner_part = rest[digits..].strip_prefix('@')?;
    let owner_len = owner_part.find(is_name_delim).unwrap_or(owner_part.len());
    if owner_len == 0 {
        return None;
    }
    Some((Marker::Class { index, owner: &owner_part[..owner_len] }, head + 1 + owner_len))
}

/// Replace every marker in `s` by `resolve(marker)`; a marker it answers `None` for is kept
/// verbatim.
pub fn substitute(s: &str, mut resolve: impl FnMut(Marker<'_>) -> Option<String>) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    let mut i = 0;
    while let Some(off) = s[i..].find('$') {
        let at = i + off;
        out.push_str(&s[i..at]);
        match parse_marker(&s[at..]) {
            Some((m, len)) => {
                match resolve(m) {
                    Some(name) => out.push_str(&name),
                    None => out.push_str(&s[at..at + len]),
                }
                i = at + len;
            }
            None => {
                out.push('$');
                i = at + 1;
            }
        }
    }
    out.push_str(&s[i..]);
    out
}

#[cfg(test)]
#[path = "type_param_markers_tests.rs"]
mod type_param_markers_tests;
