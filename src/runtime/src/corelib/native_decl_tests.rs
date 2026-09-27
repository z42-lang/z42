//! add-native-decl-consistency-check: the stdlib's `[Native("__x")]` declarations and the
//! VM's `BUILTINS` table are two hand-maintained lists that must agree. Before this test the
//! first sign of drift was a load-time `unknown builtin` panic when the mismatched function
//! was first resolved (and only if that code path ran). The test scans `src/libraries/**/*.z42`
//! at `cargo test` time and enforces both directions:
//!
//! * every declared `[Native("__x")]` name must exist in `BUILTINS` (a typo / a removed builtin
//!   fails here, not at runtime);
//! * every `BUILTINS` name must be declared somewhere in the stdlib **or** be on the explicit
//!   allowlist of names that are legitimately never declared in z42 source (emitted directly by
//!   the compiler / used by the VM internally / host-only). Adding a builtin without declaring
//!   it and without extending the allowlist fails here, which keeps the allowlist honest.
//!
//! * **（split-null-sentinel-channels ④，2026-09-27）** 声明的返回类型是否 `void` 必须与
//!   表项的 `Native::Val` / `Native::Void` 一致。没有这一条，下次加 builtin 时两边会悄悄漂开：
//!   一个声明为 `void` 的 builtin 若登记成 `Native::Val`，它就又能产出 `Value` 了 ——
//!   而「用 `Value::Null` 表示无返回值」正是本 change 要消掉的那个混用。
//!
//! Only the positional `[Native("__x")]` form is checked: the `[Native(lib=..., entry=...)]`
//! form binds to native extensions (`native/ext.rs`), not to `BUILTINS`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::BUILTINS;

/// Builtins that are intentionally absent from every `[Native]` declaration in `src/libraries`.
/// Keep this list minimal and documented — an entry here is a claim that the name is consumed by
/// something other than stdlib source.
const UNDECLARED_ALLOWLIST: &[(&str, &str)] = &[
    // Emitted directly by the compiler (value boxing lowering), never spelled in stdlib source.
    ("__box_prim",   "compiler-emitted: add-primitive-value-boxing"),
    ("__box_struct", "compiler-emitted: add-struct-value-semantics"),
    // Emitted by the compiler for `available!(X)`. Doubly undeclarable: it is never spelled in
    // stdlib source, AND on the normal path it never *executes* — `fold_availability` replaces
    // the call with a `ConstBool` at module load. The runtime impl is a diagnostic fallback
    // (debug_assert on reach) whose whole job is to make "the fold pass didn't run" loud.
    ("__sym_available", "compiler-emitted: add-symbol-availability-macro"),
    // Emitted by the compiler for `methodof(Type.Member(sig))`. The overload is resolved at
    // bind time and lowered to `__methodof("<declaring FQN>.<RegKey>")`, so the name never
    // appears in stdlib source — the same shape as `__box_prim`/`__box_struct`.
    ("__methodof", "compiler-emitted: add-method-reference"),
    // Emitted by the compiler for a **class-level** `typeof(T)` — lowered to
    // `__class_type_arg(this, <classParamIndex>)`, reading the receiver's per-instance
    // `type_args`. Same shape as `__box_prim`/`__methodof`: the name never appears in stdlib
    // source, and there is no z42-callable surface for it (the index is a compile-time fact).
    ("__class_type_arg", "compiler-emitted: fix-class-level-typeof"),
    ("__class_default", "compiler-emitted: 继承链按声明类寻址（类级 default(T)）"),
    // Invoked by the VM itself (boxed-struct `GetHashCode` protocol intercept in vcall_resolve).
    ("__struct_hash_code", "VM-internal: boxed struct GetHashCode"),
    // Host-only surfaces (REPL line editor, wasm virtual filesystem) wired by their hosts.
    ("__repl_readline",       "host-only: z42-repl cdylib"),
    ("__repl_set_completer",  "host-only: z42-repl cdylib"),
    ("__repl_set_key_editor", "host-only: z42-repl cdylib"),
    ("__repl_set_keywords",   "host-only: z42-repl cdylib"),
    ("__vfs_enable", "host-only: wasm playground VFS"),
    ("__vfs_mount",  "host-only: wasm playground VFS"),
    // Legacy string primitives retained for `exec_builtin(name, …)` unit tests / embedders
    // (`BUILTINS` is append-only for `BuiltinId` stability, so they are not removed).
    ("__concat",   "legacy: kept for BuiltinId stability"),
    ("__contains", "legacy: kept for BuiltinId stability"),
    ("__len",      "legacy: kept for BuiltinId stability"),
    // 🪦 store-sync-values-in-heap 阶段 2 已落地（2026-09-26）：旧同步原语的 19 条豁免连同它们的
    // 表槽、`corelib/sync.rs`、三个 registry 一并删除 —— 所以这里**不再需要**对应条目。
    //
    // 📌 删得掉的依据（读码核实，与本文件原注的说法不同）：zbc 存的是 builtin **名字**，
    // `BuiltinId` 是 resolver 在加载期按名填的派发令牌、不跨进程持久化 ⇒ 抽掉槽位不会让既有 zbc
    // 错位。真正的判据是「已发布种子里还有没有 z42 源声明这个名字」，按归档
    // `2026-09-14-store-sync-values-in-heap` 规定的 `strings -n 3 | grep` 核过：0 引用。
];

fn libraries_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../libraries")
}

fn collect_z42_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            collect_z42_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "z42") {
            out.push(p);
        }
    }
}

/// `[Native("__name")]` occurrences outside line comments. Returns (name, "file:line").
fn declared_natives() -> Vec<(String, String)> {
    declared_natives_full().into_iter().map(|(n, _, w)| (n, w)).collect()
}

/// 同上，外加**声明的返回类型是不是 `void`**（split-null-sentinel-channels ④）。
///
/// 判据：`[Native("__x")]` 之后（同一行的剩余部分，或随后第一行非空非注释）的声明里
/// 是否出现 `void` 后紧跟一个标识符 —— 即 `... void Name(...)`。这是与 `declared_natives`
/// 同一套启发式扫描（本文件一贯手法），足以覆盖 stdlib 的实际写法。
fn declared_natives_full() -> Vec<(String, bool, String)> {
    let root = libraries_root();
    let mut files = Vec::new();
    collect_z42_files(&root, &mut files);
    assert!(!files.is_empty(), "no .z42 files under {}", root.display());
    let mut out = Vec::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        for (ln, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            let mut rest = code;
            while let Some(i) = rest.find("[Native(\"") {
                let after = &rest[i + "[Native(\"".len()..];
                if let Some(end) = after.find('"') {
                    let name = &after[..end];
                    if name.starts_with("__") {
                        let rel = f.strip_prefix(&root).unwrap_or(&f).display().to_string();
                        // 声明体 = 本行 `[Native(..)]` 之后的剩余部分；为空则往下找第一行
                        // 非空非注释（stdlib 两种写法都有）。
                        // `end` 指向关闭引号 ⇒ 从 end+1 起，再剥掉 `)]` 与空白。
                        let tail = after[end + 1..].trim_start_matches(&[')', ']', ' ', '\t'][..]);
                        let decl: String = if !tail.trim().is_empty() {
                            tail.to_string()
                        } else {
                            text.lines().skip(ln + 1)
                                .map(|l| l.trim())
                                .find(|l| !l.is_empty() && !l.starts_with("//"))
                                .unwrap_or("").to_string()
                        };
                        let is_void = decl.split_whitespace().collect::<Vec<_>>()
                            .windows(2)
                            .any(|w| w[0] == "void"
                                 && w[1].chars().next().is_some_and(|c| c.is_alphabetic() || c == '_'));
                        out.push((name.to_string(), is_void, format!("{}:{}", rel, ln + 1)));
                    }
                    rest = &after[end..];
                } else {
                    break;
                }
            }
        }
    }
    out
}

#[test]
fn every_declared_native_has_a_builtin() {
    let table: BTreeSet<&str> = BUILTINS.iter().map(|(n, _)| *n).collect();
    let missing: Vec<String> = declared_natives().into_iter()
        .filter(|(name, _)| !table.contains(name.as_str()))
        .map(|(name, at)| format!("{name} (declared at {at})"))
        .collect();
    assert!(missing.is_empty(),
        "stdlib declares [Native] names with no BUILTINS entry (add the Rust builtin or fix the name):\n  {}",
        missing.join("\n  "));
}

#[test]
fn every_builtin_is_declared_or_allowlisted() {
    let declared: BTreeSet<String> = declared_natives().into_iter().map(|(n, _)| n).collect();
    let allow: BTreeSet<&str> = UNDECLARED_ALLOWLIST.iter().map(|(n, _)| *n).collect();
    let undeclared: Vec<&str> = BUILTINS.iter().map(|(n, _)| *n)
        .filter(|n| !declared.contains(*n) && !allow.contains(n))
        .collect();
    assert!(undeclared.is_empty(),
        "BUILTINS entries neither declared via [Native] in src/libraries nor allowlisted \
         (declare them in stdlib, or add to UNDECLARED_ALLOWLIST with a reason):\n  {}",
        undeclared.join("\n  "));
    // The allowlist must not go stale either: an allowlisted name that IS now declared should
    // simply be removed from the allowlist.
    let stale: Vec<&str> = allow.iter().copied().filter(|n| declared.contains(*n)).collect();
    assert!(stale.is_empty(),
        "UNDECLARED_ALLOWLIST entries are now declared in stdlib — drop them from the allowlist:\n  {}",
        stale.join("\n  "));
    // …and must only name real builtins.
    let table: BTreeSet<&str> = BUILTINS.iter().map(|(n, _)| *n).collect();
    let unknown: Vec<&str> = allow.iter().copied().filter(|n| !table.contains(n)).collect();
    assert!(unknown.is_empty(),
        "UNDECLARED_ALLOWLIST names are not BUILTINS at all:\n  {}", unknown.join("\n  "));
}

/// 🔴 stdlib 声明的 `void` ⟺ 表项是 `Native::Void`（split-north-null ④ 的闭环门）。
///
/// **为什么需要它**：`Native::Val` / `Native::Void` 是手写的第三份数据，与 stdlib 的
/// 返回类型必须一致。漂开的后果不是崩，而是**悄悄退回旧行为** —— 一个 `void` builtin
/// 登记成 `Val` 就又能产出 `Value` 了，而「用 `Value::Null` 表示无返回值」正是本 change
/// 要消掉的混用。名字那两条方向的对账已在上面；这条管「味」。
#[test]
fn declared_voidness_matches_the_builtin_table() {
    let table: std::collections::BTreeMap<&str, bool> =
        BUILTINS.iter().map(|(n, f)| (*n, f.is_void())).collect();
    let mut bad = Vec::new();
    for (name, decl_void, where_) in declared_natives_full() {
        let Some(&tbl_void) = table.get(name.as_str()) else { continue };  // 名字那条门管这格
        if decl_void != tbl_void {
            bad.push(format!(
                "  {name}: stdlib 声明{}，而 BUILTINS 登记为 Native::{}  ({where_})",
                if decl_void { "返回 void" } else { "有返回值" },
                if tbl_void { "Void" } else { "Val" },
            ));
        }
    }
    bad.sort();
    bad.dedup();
    assert!(
        bad.is_empty(),
        "stdlib 的 [Native] 返回类型与 BUILTINS 的 Val/Void 登记不一致：\n{}\n\
         修法：声明 void 的登记成 `Native::Void(f)` 且 `f` 返回 `Result<()>`；\n\
         有返回值的登记成 `Native::Val(f)` 且 `f` 返回 `Result<Value>`。\n\
         （change split-null-sentinel-channels ④：void 不再用 `Value::Null` 表示）",
        bad.join("\n")
    );
}
