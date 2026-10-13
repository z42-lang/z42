//! 源码文本工具：按 span 截签名、取 rustdoc、判可见性与 `#[cfg(test)]`。

use proc_macro2::Span;
use syn::{Attribute, Expr, Lit, Meta, Visibility};

/// 一个源文件的行（span 的行号从 1 起、列号按字符计）。
pub struct Source {
    lines: Vec<Vec<char>>,
}

impl Source {
    pub fn new(text: &str) -> Self {
        Source { lines: text.lines().map(|l| l.chars().collect()).collect() }
    }

    /// `from` 起点到 `to` 终点之间的源码，空白折成单个空格。签名就靠它按源码原样写出。
    pub fn slice(&self, from: Span, to: Span) -> String {
        let (s, e) = (from.start(), to.end());
        if s.line == 0 || e.line < s.line || e.line > self.lines.len() { return String::new(); }
        let mut out = String::new();
        for ln in s.line..=e.line {
            let l = &self.lines[ln - 1];
            let a = if ln == s.line { s.column.min(l.len()) } else { 0 };
            let b = if ln == e.line { e.column.min(l.len()) } else { l.len() };
            if a < b { out.extend(&l[a..b]); }
            out.push(' ');
        }
        collapse(&out)
    }
}

fn collapse(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut sp = false;
    for c in s.chars() {
        if c.is_whitespace() { sp = true; continue; }
        if sp && !out.is_empty() { out.push(' '); }
        sp = false;
        out.push(c);
    }
    out
}

/// `///` / `//!` 编译成的 `#[doc = "..."]` 拼回原文：每行去掉一个前导空格，首尾空行去掉。
pub fn doc_of(attrs: &[Attribute]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for a in attrs {
        if !a.path().is_ident("doc") { continue; }
        if let Meta::NameValue(nv) = &a.meta {
            if let Expr::Lit(el) = &nv.value {
                if let Lit::Str(s) = &el.lit {
                    for l in s.value().split('\n') {
                        lines.push(l.strip_prefix(' ').unwrap_or(l).trim_end().to_string());
                    }
                }
            }
        }
    }
    while lines.last().is_some_and(|l| l.is_empty()) { lines.pop(); }
    while lines.first().is_some_and(|l| l.is_empty()) { lines.remove(0); }
    lines.join("\n")
}

/// `pub` → public；`pub(crate)` / `pub(super)` / `pub(in …)` → internal；不写 → private。
pub fn vis_of(v: &Visibility) -> &'static str {
    match v {
        Visibility::Public(_) => "public",
        Visibility::Restricted(_) => "internal",
        Visibility::Inherited => "private",
    }
}

/// 带 `#[cfg(test)]`（或 `cfg(all(test, …))` 一类含 `test` 的条件）的条目不进索引。
pub fn is_test_only(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && a.meta.require_list().map(|l| {
                let t = l.tokens.to_string();
                t == "test" || t.starts_with("test ") || t.contains("(test") || t.contains(" test")
            }).unwrap_or(false)
    })
}

/// `#[path = "…"]` 的值。
pub fn path_attr(attrs: &[Attribute]) -> Option<String> {
    for a in attrs {
        if !a.path().is_ident("path") { continue; }
        if let Meta::NameValue(nv) = &a.meta {
            if let Expr::Lit(el) = &nv.value {
                if let Lit::Str(s) = &el.lit { return Some(s.value()); }
            }
        }
    }
    None
}
