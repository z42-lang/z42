use super::*;

// ── Namespace resolution ──────────────────────────────────────────────────────

/// Resolve which files provide a given namespace.
///
/// Multiple zpkgs may legitimately declare the same namespace (C# assembly
/// model). Returns **all** matching files sorted by search tier:
///   1. `module_paths`: scan `.zbc` files (binary, read namespace from header)
///   2. `libs_paths`:   scan `.zpkg` files (binary, read NSPC section)
///
/// If a `.zbc` file in `module_paths` matches, `.zpkg` files in `libs_paths`
/// are **not** scanned (module-path override). This preserves the historical
/// override behaviour without coupling it to single-result semantics.
///
/// Used by compiler tooling and diagnostics. The VM's lazy loader no longer
/// routes by namespace; it uses zpkg file names (`resolve_dependency`).
pub fn resolve_namespace(
    ns: &str,
    module_paths: &[PathBuf],
    libs_paths: &[PathBuf],
) -> Result<Vec<PathBuf>> {
    let zbc_matches = find_namespace_in_zbc_dirs(ns, module_paths)?;
    if !zbc_matches.is_empty() {
        return Ok(zbc_matches);
    }
    find_namespace_in_zpkg_dirs(ns, libs_paths)
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

fn find_namespace_in_zbc_dirs(ns: &str, dirs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut found: Vec<PathBuf> = Vec::new();
    let backend = fs_backend::active();
    for dir in dirs {
        let dir_str = match dir.to_str() { Some(s) => s, None => continue };
        let names = match backend.read_dir(dir_str) { Ok(e) => e, Err(_) => continue };
        for name in names {
            let path = dir.join(&name);
            if path.extension().and_then(|e| e.to_str()) != Some("zbc") { continue; }
            let path_str = match path.to_str() { Some(s) => s, None => continue };
            let data = match backend.read(path_str) { Ok(d) => d, Err(_) => continue };
            if data.len() < 4 || &data[0..4] != ZBC_MAGIC { continue; }
            let file_ns = match read_zbc_namespace(&data) { Ok(n) => n, Err(_) => continue };
            if file_ns == ns && !found.contains(&path) {
                found.push(path);
            }
        }
    }
    Ok(found)
}

fn find_namespace_in_zpkg_dirs(ns: &str, dirs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut found: Vec<PathBuf> = Vec::new();
    let backend = fs_backend::active();
    for dir in dirs {
        let dir_str = match dir.to_str() { Some(s) => s, None => continue };
        let names = match backend.read_dir(dir_str) { Ok(e) => e, Err(_) => continue };
        for name in names {
            let path = dir.join(&name);
            if path.extension().and_then(|e| e.to_str()) != Some("zpkg") { continue; }
            let path_str = match path.to_str() { Some(s) => s, None => continue };
            let data = match backend.read(path_str) { Ok(d) => d, Err(_) => continue };
            if data.len() < 4 || &data[0..4] != ZPKG_MAGIC { continue; }
            let namespaces = match read_zpkg_namespaces(&data) { Ok(v) => v, Err(_) => continue };
            if namespaces.iter().any(|n| n == ns) && !found.contains(&path) {
                found.push(path);
            }
        }
    }
    Ok(found)
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Reads the namespace from a binary zbc buffer (NSPC section fast-path).
pub fn read_zbc_namespace(data: &[u8]) -> Result<String> {
    use crate::metadata::formats::ZBC_MAGIC;
    if data.len() < 16 { bail!("zbc buffer too short ({} bytes)", data.len()) }
    if &data[0..4] != ZBC_MAGIC { bail!("not a zbc file (bad magic)") }

    let sec_count = u16::from_le_bytes([data[10], data[11]]);
    let dir = crate::metadata::zbc_reader::read_directory_pub(data, sec_count)?;

    match dir.get(b"NSPC") {
        None => Ok(String::new()),
        Some(&(off, size)) => {
            if off + size > data.len() { bail!("NSPC section out of bounds") }
            let sec = &data[off..off + size];
            if sec.len() < 2 { return Ok(String::new()); }
            let len = u16::from_le_bytes([sec[0], sec[1]]) as usize;
            if len == 0 || sec.len() < 2 + len { return Ok(String::new()); }
            Ok(std::str::from_utf8(&sec[2..2 + len])?.to_owned())
        }
    }
}

/// Extract unique namespace prefixes from a module's external calls and static
/// field accesses.
///
/// Namespace = first two dot-separated components of a Call / static.get target
/// not defined locally.
///
/// 2026-04-27 fix-static-field-access: 加上 StaticGet 扫描。修前 user code
/// `Math.PI` 编译为 `static.get @Std.Math.Math.PI`，但 namespace 提取只看 Call/
/// Builtin → 不发现 Std.Math 依赖 → 不 lazy-load z42.math → __static_init__ 不
/// 跑 → 字段永远 null。
pub(super) fn extract_import_namespaces_from_module(module: &Module) -> Vec<String> {
    use crate::metadata::bytecode::Instruction;
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
                    // fix-objnew-import-ns (2026-05-29): `new Foo()` on an imported
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

