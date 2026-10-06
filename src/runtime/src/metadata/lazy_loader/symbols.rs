//! precise-pkg-refs: "which package defines this name" from the consumers' DEPS symbol lists.
//!
//! Each zpkg's DEPS entry lists the full names of the types / free functions the package
//! references that are **defined by** that dependency (zpkg 0.52). This is the z42 analogue of
//! a .NET TypeRef's resolution scope: the referencing side states where the definition lives,
//! so a miss loads exactly that package instead of guessing by namespace prefix. The table is
//! fed by the entry artifact (boot) and by every package as it loads, so it always covers the
//! references of the code that can currently run.
//!
//! Keys are *symbol keys* ([`symbol_key`]): generic arguments and `$` overload / arity suffixes
//! stripped — the compiler writes them the same way (`Z42.Semantics.DepRef.SymKey`).

use super::*;
use crate::metadata::formats::ZpkgDep;

/// Where a lookup name was routed by the symbol table.
pub(crate) struct SymbolRoute {
    /// zpkg file that defines the symbol.
    pub file:  String,
    /// The name itself (modulo generic args / `$` suffix) is a recorded symbol — a type or a free
    /// function. A miss after loading `file` is then a genuinely missing symbol. When only an
    /// *enclosing* type matched (a method or field name), the member may still come from another
    /// package (an `impl` block, a base class), so the caller keeps its broader fallbacks.
    pub exact: bool,
}

/// Normalize a lookup name to its symbol key: cut at the first `<` (generic arguments) or `$`
/// (overload / arity suffix), whichever comes first.
pub(crate) fn symbol_key(name: &str) -> &str {
    let cut = name.find(|c| c == '<' || c == '$').unwrap_or(name.len());
    &name[..cut]
}

impl LazyLoader {
    /// Record the symbol → package pairs of a DEPS list. First writer wins (two consumers naming
    /// different owners for one full name is a duplicate definition — E0601 territory at compile
    /// time — and the loader's first-wins registration would pick one anyway).
    pub fn note_symbol_owners(&mut self, deps: &[ZpkgDep]) {
        for dep in deps {
            for sym in &dep.symbols {
                if self.symbol_owners.contains_key(sym) { continue; }
                self.symbol_owners.insert(sym.clone(), dep.file.clone());
                let short = sym.rsplit(|c| c == '.' || c == '+').next().unwrap_or(sym);
                match self.short_symbols.get(short) {
                    None => { self.short_symbols.insert(short.to_string(), Some(sym.clone())); }
                    Some(Some(prev)) if prev != sym => { self.short_symbols.insert(short.to_string(), None); }
                    Some(_) => {}
                }
            }
        }
    }

    /// Route `name` (a function, type or `Type.member` lookup name) to its defining package:
    /// the name itself, then its enclosing type (`Ns.Cls.Method` → `Ns.Cls`), then one more level
    /// (`Ns.Outer.Cls.M`-shaped static members of nested owners are spelled with `+`, so two levels
    /// cover every member form).
    pub(crate) fn symbol_route(&self, name: &str) -> Option<SymbolRoute> {
        if self.symbol_owners.is_empty() { return None; }
        let key = symbol_key(name);
        if let Some(f) = self.symbol_owners.get(key) {
            return Some(SymbolRoute { file: f.clone(), exact: !name.contains('<') });
        }
        let mut cur = key;
        for _ in 0..2 {
            let Some((head, _)) = cur.rsplit_once('.') else { break };
            if let Some(f) = self.symbol_owners.get(head) {
                return Some(SymbolRoute { file: f.clone(), exact: false });
            }
            cur = head;
        }
        None
    }

    /// The unique full name recorded for a simple (dotless) type name, if exactly one referenced
    /// symbol has that short name. Backs reflection's dotless lookups without force-loading
    /// every package.
    pub fn full_name_for_short(&self, short: &str) -> Option<String> {
        self.short_symbols.get(short).and_then(|v| v.clone())
    }

    /// Load the package `route` points at (if declared and not yet loaded). Returns whether the
    /// package is now resident.
    pub(crate) fn load_routed(&mut self, route: &SymbolRoute) -> bool {
        if self.loaded_zpkgs.contains(&route.file) { return true; }
        self.load_zpkg_file(&route.file).is_ok() && self.loaded_zpkgs.contains(&route.file)
    }
}

#[cfg(test)]
#[path = "symbols_tests.rs"]
mod symbols_tests;
