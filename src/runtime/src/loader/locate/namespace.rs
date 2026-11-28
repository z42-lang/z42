use anyhow::Result;
use std::path::PathBuf;
use crate::pal::fs::Vfs;
use crate::metadata::Module;
use crate::loader::locate::namespace_index::NamespaceLookup;

// ── Namespace resolution ──────────────────────────────────────────────────────

/// Resolve which files on `fs` provide a given namespace.
///
/// Multiple zpkgs may legitimately declare the same namespace (C# assembly
/// model). Returns **all** matching `.zpkg` files under `libs_paths`.
///
/// Provides a one-shot query for tooling. Loads with legacy namespace imports share a lookup.
pub fn resolve_namespace(fs: &dyn Vfs, ns: &str, libs_paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut lookup = NamespaceLookup::new(fs, libs_paths);
    Ok(lookup.matching(ns).map(|candidate| candidate.file_path.clone()).collect())
}

/// Resolve a zpkg dependency by its file name (e.g. `"z42.collections.zpkg"`).
/// Searches `libs_paths` in order and returns the first match. Used by the
/// lazy loader to locate declared dependencies for on-demand load.
pub fn resolve_dependency(
    zpkg_file: &str,
    libs_paths: &[PathBuf],
) -> Result<Option<PathBuf>> {
    for dir in libs_paths {
        let path = dir.join(zpkg_file);
        if path.is_file() {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Extract unique namespace prefixes from a module's external calls and static
/// field accesses.
///
/// Namespace = first two dot-separated components of a Call / static.get target
/// not defined locally.
///
/// fix-static-field-access: 加上 StaticGet 扫描。修前 user code
/// `Math.PI` 编译为 `static.get @Std.Math.Math.PI`，但 namespace 提取只看 Call/
/// Builtin → 不发现 Std.Math 依赖 → 不 lazy-load z42.math → __static_init__ 不
/// 跑 → 字段永远 null。
pub(crate) fn extract_import_namespaces_from_module(module: &Module) -> Vec<String> {
    use crate::zpkg::ir::Instruction;
    let defined: std::collections::HashSet<&str> =
        module.functions.iter().map(|f| f.name.as_str()).collect();
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for func in &module.functions {
        for block in &func.blocks {
            for instr in &block.instructions {
                let target = match instr {
                    Instruction::Call(insn)    if !defined.contains(insn.func.as_str()) => &insn.func,
                    Instruction::Builtin(insn) => &insn.name,
                    Instruction::StaticGet(insn) => &insn.field,
                    Instruction::StaticSet(insn) => &insn.field,
                    // fix-objnew-import-ns : `new Foo()` on an imported
                    // class emits ObjNew (not Call), and the subsequent method
                    // calls are VCall (vtable) — neither was scanned, so the
                    // providing zpkg's namespace was never inferred and the lazy
                    // loader never declared/loaded it → `VCall: function not
                    // found` on the first method. Use `class_name` so the
                    // constructed type's namespace (e.g. `Std.Collections` from
                    // `Std.Collections.LinkedList`) is registered; loading the
                    // zpkg then makes every method resolvable. Previously masked
                    // when the method happened to compile to a Call (DepIndex
                    // shortcut) instead of a VCall — order-dependent, hence the
                    // cross-platform-flaky failures.
                    Instruction::ObjNew(insn) if !defined.contains(insn.class_name.as_str()) => &insn.class_name,
                    _ => continue,
                };
                for ns in infer_namespace_candidates(target) {
                    if seen.insert(ns.to_owned()) {
                        result.push(ns.to_owned());
                    }
                }
            }
        }
    }
    result
}

/// Extract candidate import-namespace prefixes from a list of fully-qualified
/// call targets. Returns each unique prefix in first-seen order.
pub fn extract_import_namespaces(imports: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for import in imports {
        for ns in infer_namespace_candidates(import) {
            if seen.insert(ns.to_owned()) { result.push(ns.to_owned()); }
        }
    }
    result
}

/// All candidate namespace prefixes of a fully-qualified call target.
///
/// For `Std.IO.Binary.BinaryWriter.WriteByte` returns
/// `["Std", "Std.IO", "Std.IO.Binary", "Std.IO.Binary.BinaryWriter"]` — every
/// `.`-bounded prefix shorter than the full name. The lazy loader feeds these
/// to `resolve_namespace`: only prefixes that match an actual zpkg's declared
/// namespace pull in deps. Returning the full set covers stdlib namespaces of
/// any depth (`Std.IO` vs `Std.IO.Binary`) without the resolver needing to
/// know in advance where the namespace ends and `class.method` begins.
///
/// Names with no dot fall back to the name itself (preserves legacy behaviour
/// for single-segment idents).
fn infer_namespace_candidates(name: &str) -> Vec<&str> {
    let mut result: Vec<&str> = name
        .char_indices()
        .filter_map(|(i, c)| if c == '.' { Some(&name[..i]) } else { None })
        .collect();
    if result.is_empty() { result.push(name); }
    result
}

