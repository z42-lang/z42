use super::*;
use std::path::PathBuf;

// ── namespace_prefix ──────────────────────────────────────────────────────────

#[test]
fn namespace_prefix_of_qualified_call() {
    assert_eq!(
        namespace_prefix("Std.IO.Console.WriteLine"),
        Some("Std.IO".to_string())
    );
    assert_eq!(
        namespace_prefix("Std.Text.StringBuilder.Append$1"),
        Some("Std.Text".to_string())
    );
}

#[test]
fn namespace_prefix_of_deep_call() {
    // "Std.Collections.Stack.Push" → "Std.Collections" (W1 routing key)
    assert_eq!(
        namespace_prefix("Std.Collections.Stack.Push"),
        Some("Std.Collections".to_string())
    );
}

#[test]
fn namespace_prefix_of_shallow_name() {
    assert_eq!(namespace_prefix("Assert.Equal"), Some("Assert".to_string()));
    assert_eq!(namespace_prefix("main"), None);
}

// ── candidates_for_namespace (strategy C routing) ────────────────────────────

fn fake_candidate(namespaces: &[&str]) -> ZpkgCandidate {
    ZpkgCandidate {
        file_path:  PathBuf::from("/does/not/matter"),
        namespaces: namespaces.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn candidates_routes_by_exact_namespace() {
    let loader = LazyLoader::new(
        Vec::new(),
        0,
        vec![
            ("a.zpkg".to_string(), fake_candidate(&["Std.IO"])),
            ("b.zpkg".to_string(), fake_candidate(&["Std.Collections"])),
        ],
        Vec::new(),
    );
    let matches = loader.candidates_for_namespace("Std.Collections");
    assert_eq!(matches, vec!["b.zpkg".to_string()]);
}

#[test]
fn candidates_routes_by_descendant_namespace() {
    // Querying `Std.Collections` should match a zpkg declaring
    // `Std.Collections.Generic` (descendant prefix match).
    let loader = LazyLoader::new(
        Vec::new(),
        0,
        vec![
            ("a.zpkg".to_string(), fake_candidate(&["Std.Collections.Generic"])),
            ("b.zpkg".to_string(), fake_candidate(&["Std.IO"])),
        ],
        Vec::new(),
    );
    let matches = loader.candidates_for_namespace("Std.Collections");
    assert_eq!(matches, vec!["a.zpkg".to_string()]);
}

/// W1 regression guard: two zpkgs legitimately share `Std.Collections`
/// (`z42.core` declares it for List/Dictionary; `z42.collections` declares
/// it for Queue/Stack). Both must be routed as candidates — no ambiguity
/// error at this layer.
#[test]
fn candidates_routes_multi_zpkg_sharing_namespace() {
    let loader = LazyLoader::new(
        Vec::new(),
        0,
        vec![
            (
                "z42.core.zpkg".to_string(),
                fake_candidate(&["Std", "Std.Collections"]),
            ),
            (
                "z42.collections.zpkg".to_string(),
                fake_candidate(&["Std.Collections"]),
            ),
        ],
        Vec::new(),
    );
    let mut matches = loader.candidates_for_namespace("Std.Collections");
    matches.sort();
    assert_eq!(
        matches,
        vec![
            "z42.collections.zpkg".to_string(),
            "z42.core.zpkg".to_string(),
        ]
    );
}

/// fix-runtime-load-order-determinism (common-pitfalls.md §1): `candidates_for_namespace`
/// iterates an FxHashMap (non-deterministic order) and downstream consumers pick first-wins,
/// so the returned Vec must be sorted regardless of insertion order. Insert several
/// same-namespace candidates in reverse-sorted order and assert the result is sorted
/// *without* the caller sorting it.
#[test]
fn candidates_returns_sorted_order() {
    let loader = LazyLoader::new(
        Vec::new(),
        0,
        vec![
            ("z.zpkg".to_string(), fake_candidate(&["Std.Collections"])),
            ("m.zpkg".to_string(), fake_candidate(&["Std.Collections"])),
            ("a.zpkg".to_string(), fake_candidate(&["Std.Collections"])),
        ],
        Vec::new(),
    );
    let matches = loader.candidates_for_namespace("Std.Collections");
    assert_eq!(
        matches,
        vec!["a.zpkg".to_string(), "m.zpkg".to_string(), "z.zpkg".to_string()],
        "candidates must be returned in stable sorted order (no caller-side sort)",
    );
}

/// fix-runtime-load-order-determinism: `remaining_declared` likewise must return a
/// stable sorted order (drives `__static_init__` force-load enumeration).
#[test]
fn remaining_declared_returns_sorted_order() {
    let loader = LazyLoader::new(
        Vec::new(),
        0,
        vec![
            ("z.zpkg".to_string(), fake_candidate(&["Z"])),
            ("m.zpkg".to_string(), fake_candidate(&["M"])),
            ("a.zpkg".to_string(), fake_candidate(&["A"])),
        ],
        Vec::new(),
    );
    assert_eq!(
        loader.remaining_declared(),
        vec!["a.zpkg".to_string(), "m.zpkg".to_string(), "z.zpkg".to_string()],
        "remaining_declared must be returned in stable sorted order (no caller-side sort)",
    );
}

#[test]
fn install_filters_already_loaded_from_declared() {
    let loader = LazyLoader::new(
        Vec::new(),
        0,
        vec![(
            "z42.collections.zpkg".to_string(),
            fake_candidate(&["Std.Collections"]),
        )],
        vec!["z42.collections.zpkg".to_string()], // already loaded
    );
    assert!(loader.declared_zpkgs.is_empty());
    assert!(loader.candidates_for_namespace("Std.Collections").is_empty());
}

#[test]
fn remaining_declared_excludes_loaded() {
    let mut loader = LazyLoader::new(
        Vec::new(),
        0,
        vec![
            ("a.zpkg".to_string(), fake_candidate(&["X"])),
            ("b.zpkg".to_string(), fake_candidate(&["Y"])),
        ],
        Vec::new(),
    );
    loader.loaded_zpkgs.insert("a.zpkg".to_string());
    let mut r = loader.remaining_declared();
    r.sort();
    assert_eq!(r, vec!["b.zpkg".to_string()]);
}

#[test]
fn candidates_excludes_subsequently_loaded() {
    let mut loader = LazyLoader::new(
        Vec::new(),
        0,
        vec![(
            "a.zpkg".to_string(),
            fake_candidate(&["Std.Collections"]),
        )],
        Vec::new(),
    );
    // Initially routed as candidate.
    assert_eq!(
        loader.candidates_for_namespace("Std.Collections"),
        vec!["a.zpkg".to_string()]
    );
    // After marking loaded, no longer a candidate (Decision 4 idempotency).
    loader.loaded_zpkgs.insert("a.zpkg".to_string());
    assert!(loader.candidates_for_namespace("Std.Collections").is_empty());
}

// ── VmContext-based install / uninstall (replaces former thread_local API) ───

#[test]
fn vm_context_install_then_uninstall_is_clean() {
    let ctx = crate::vm_context::VmContext::new();
    ctx.install_lazy_loader(None, 0);
    assert!(ctx.try_lookup_function("Std.IO.Console.WriteLine").is_none());
    ctx.uninstall_lazy_loader();
    assert!(ctx.try_lookup_function("Anything.Foo").is_none());
}

#[test]
fn vm_context_install_with_deps_no_libs_no_declared_returns_none() {
    let ctx = crate::vm_context::VmContext::new();
    ctx.install_lazy_loader_with_deps(Vec::new(), 0, Vec::new(), Vec::new());
    assert!(ctx.try_lookup_function("Std.Anything.F").is_none());
    assert!(ctx.try_lookup_type("Std.Anything").is_none());
    ctx.uninstall_lazy_loader();
}

// ── build_in_dirs: colocated dep search (support-colocated-zpkg-deps) ─────────

/// Path to a committed, valid zpkg fixture usable as a real on-disk zpkg.
fn fixture_zpkg() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/zpkg-format/packed-minimal/source.zpkg")
}

#[test]
fn build_in_dirs_not_found_errors() {
    let a = std::env::temp_dir().join("z42-coloc-a-empty");
    let _ = std::fs::create_dir_all(&a);
    assert!(ZpkgCandidate::build_in_dirs(&[a], "nope.zpkg").is_err());
    assert!(ZpkgCandidate::build_in_dirs(&[], "nope.zpkg").is_err());
}

#[test]
fn build_in_dirs_finds_in_later_dir() {
    // Two dirs; the zpkg lives only in the SECOND — `build_in_dirs` must skip
    // the first and resolve from the second (colocated-dep search semantics).
    let base = std::env::temp_dir().join("z42-coloc-order");
    let dir1 = base.join("empty");
    let dir2 = base.join("has");
    let _ = std::fs::create_dir_all(&dir1);
    let _ = std::fs::create_dir_all(&dir2);
    let target = dir2.join("colo.zpkg");
    std::fs::copy(fixture_zpkg(), &target).expect("copy fixture zpkg");

    let cand = ZpkgCandidate::build_in_dirs(&[dir1.clone(), dir2.clone()], "colo.zpkg")
        .expect("resolves from the second dir");
    assert_eq!(cand.file_path, target, "resolved from the dir that actually has the file");
}

#[test]
fn build_in_dirs_first_dir_wins() {
    // When present in BOTH dirs, the FIRST listed dir wins (deterministic order).
    let base = std::env::temp_dir().join("z42-coloc-firstwins");
    let dir1 = base.join("first");
    let dir2 = base.join("second");
    let _ = std::fs::create_dir_all(&dir1);
    let _ = std::fs::create_dir_all(&dir2);
    std::fs::copy(fixture_zpkg(), dir1.join("dup.zpkg")).unwrap();
    std::fs::copy(fixture_zpkg(), dir2.join("dup.zpkg")).unwrap();

    let cand = ZpkgCandidate::build_in_dirs(&[dir1.clone(), dir2.clone()], "dup.zpkg").unwrap();
    assert_eq!(cand.file_path, dir1.join("dup.zpkg"), "first search dir wins on conflict");
}

// ── fix-repl-inmemory-dep-warn ───────────────────────────────────────────────
// REPL rounds are compiled to zpkg bytes and loaded in memory, never written to
// disk. A later round referencing an earlier round's type (R2 uses `Repl.R1.A`)
// records a dep on `repl_r1.zpkg`. The dep-resolution loop must recognise that
// package as already resident instead of probing disk (which fails → spurious
// "cannot read dep zpkg meta `repl_r1.zpkg`" WARN, even though the ref resolves
// in-process). The fix: on load, mark `<package_name>.zpkg` in `loaded_zpkgs`.

fn ll_empty_module(name: &str) -> crate::metadata::bytecode::Module {
    crate::metadata::bytecode::Module {
        name: name.to_owned(),
        string_pool: vec![],
        classes: vec![],
        functions: vec![],
        type_registry: rustc_hash::FxHashMap::default(),
        type_registry_vec: Vec::new(),
        func_index: rustc_hash::FxHashMap::default(),
        func_ref_cache_slots: 0,
    }
}

fn ll_inmem_artifact(
    module_name: &str, pkg: Option<&str>, deps: &[&str],
) -> crate::metadata::loader::LoadedArtifact {
    crate::metadata::loader::LoadedArtifact {
        module: ll_empty_module(module_name),
        entry_hint: None,
        dependencies: deps
            .iter()
            .map(|f| crate::metadata::formats::ZpkgDep { file: (*f).to_string(), namespaces: vec![] })
            .collect(),
        import_namespaces: vec![],
        test_index: vec![],
        impl_pairs: vec![],
        package_name: pkg.map(str::to_string),
    }
}

#[test]
fn inmemory_package_registers_zpkg_file_as_resident() {
    // Non-empty search_dirs → the dep-resolution loop runs (as in a real REPL
    // loader). The dir need not exist: R1 has no deps, and R2's single dep must
    // short-circuit on `loaded_zpkgs` before any disk probe.
    let mut loader = LazyLoader::new(
        vec![PathBuf::from("/z42-nonexistent-search-dir")], 0, Vec::new(), Vec::new(),
    );

    // R1: `class A {}` → package `repl_r1` (namespace `Repl.R1`), in memory only.
    loader
        .register_loaded_artifact(ll_inmem_artifact("Repl.R1", Some("repl_r1"), &[]))
        .expect("register R1");
    assert!(
        loader.loaded_zpkgs.contains("repl_r1.zpkg"),
        "an in-memory package must mark its canonical zpkg file name resident \
         so later dependents recognise it (regression: fix-repl-inmemory-dep-warn)",
    );

    // R2: `A a = new A()` → dep on `repl_r1.zpkg`. Registers without probing disk;
    // the dependency stays resolved via `loaded_zpkgs`, not `declared_zpkgs`.
    loader
        .register_loaded_artifact(ll_inmem_artifact("Repl.R2", Some("repl_r2"), &["repl_r1.zpkg"]))
        .expect("register R2");
    assert!(
        loader.loaded_zpkgs.contains("repl_r1.zpkg"),
        "R1 stays resident after R2 loads",
    );
    assert!(
        !loader.declared_zpkgs.contains_key("repl_r1.zpkg"),
        "the dependent short-circuits on the resident set, never building a disk candidate",
    );
}

// ── fix-repl-sdk-compiler-closure ────────────────────────────────────────────
// Loading a module by a foreign absolute path (e.g. z42.scripting's REPL injector
// loading <sdk>/programs/z42c/z42c.pipeline.zpkg) must let that zpkg's colocated
// transitive deps (z42c.semantics + siblings, sitting in the SAME dir) resolve —
// even though that dir isn't in the VM's startup search_dirs. The loader adds the
// loaded artifact's own directory to search_dirs (appended, lowest priority).

#[test]
fn load_module_from_path_adds_loaded_module_dir_to_search_dirs() {
    let base = std::env::temp_dir().join("z42-coloc-loadpath");
    let start = base.join("startup-empty"); // in startup search_dirs, has nothing
    let progdir = base.join("programs-z42c"); // where the module + its siblings live
    let _ = std::fs::create_dir_all(&start);
    let _ = std::fs::create_dir_all(&progdir);
    let modpath = progdir.join("colo_mod.zpkg");
    std::fs::copy(fixture_zpkg(), &modpath).expect("copy fixture zpkg");

    // Loader's startup search_dirs deliberately EXCLUDE progdir.
    let mut loader = LazyLoader::new(vec![start.clone()], 0, Vec::new(), Vec::new());
    assert!(
        !loader.search_dirs.iter().any(|d| d == &progdir),
        "precondition: loaded module's dir is not a startup search dir",
    );

    loader
        .load_module_from_path(modpath.to_str().unwrap())
        .expect("load colocated fixture module by path");

    assert!(
        loader.search_dirs.iter().any(|d| d == &progdir),
        "the loaded module's own directory must be added to search_dirs so its \
         colocated transitive deps resolve (fix-repl-sdk-compiler-closure)",
    );
    // Appended, not prepended: the startup dir keeps priority.
    assert_eq!(
        loader.search_dirs.first(),
        Some(&start),
        "startup search dir stays first (module dir is a lowest-priority fallback)",
    );
}

#[test]
fn load_module_from_path_dedupes_module_dir() {
    // If the module's dir is ALREADY a search dir, it isn't added twice.
    let dir = std::env::temp_dir().join("z42-coloc-dedupe");
    let _ = std::fs::create_dir_all(&dir);
    let modpath = dir.join("colo_dedupe.zpkg");
    std::fs::copy(fixture_zpkg(), &modpath).expect("copy fixture zpkg");

    let mut loader = LazyLoader::new(vec![dir.clone()], 0, Vec::new(), Vec::new());
    loader
        .load_module_from_path(modpath.to_str().unwrap())
        .expect("load fixture module by path");

    let count = loader.search_dirs.iter().filter(|d| **d == dir).count();
    assert_eq!(count, 1, "module dir already present → not duplicated in search_dirs");
}

// ── defer-class-initialization: 原生类型名守卫 ────────────────────────────────

#[test]
fn primitive_keyword_names_are_recognized() {
    // 无点号的编译器关键字 → 守卫命中（不进 Fallback-B 全量扫描）
    for n in ["int", "long", "short", "byte", "sbyte", "uint", "ulong", "ushort",
              "float", "double", "bool", "char", "string", "object", "void"] {
        assert!(is_primitive_keyword_name(n), "`{n}` should be a primitive keyword name");
    }
}

#[test]
fn qualified_and_user_names_are_not_primitive() {
    // 带点号的一律不是（哪怕最后一段撞了关键字）
    assert!(!is_primitive_keyword_name("Std.Int32"));
    assert!(!is_primitive_keyword_name("Demo.int"));
    // 用户类名不是
    assert!(!is_primitive_keyword_name("Process"));
    assert!(!is_primitive_keyword_name("Integer"));
}

#[test]
fn init_state_distinguishes_running_thread() {
    // 重入判定靠 ThreadId 相等：同线程 Running → 跳过；他线程 Running → 等待。
    let me = std::thread::current().id();
    let other = std::thread::spawn(|| std::thread::current().id()).join().unwrap();
    assert_ne!(me, other);
    assert_eq!(InitState::Running(me), InitState::Running(me));
    assert_ne!(InitState::Running(me), InitState::Running(other));
    assert_ne!(InitState::Running(me), InitState::Done);
}

// ── cache-failed-name-resolution: negative resolve cache ─────────────────────

/// A minimal `Function` usable as an in-memory module member.
fn ll_stub_function(name: &str) -> crate::metadata::bytecode::Function {
    use crate::metadata::bytecode::{BasicBlock, Function, Terminator};
    use crate::metadata::types::ExecMode;
    Function {
        name: name.to_string(),
        param_count: 0,
        ret_type: "void".to_string(),
        exec_mode: ExecMode::Interp,
        blocks: vec![BasicBlock {
            label: "entry".to_string(),
            instructions: Vec::new(),
            terminator: Terminator::Ret { reg: None },
        }],
        is_static: true,
        visibility: 0,
        method_flags: 0, min_arg: 0, params_from: 0xFF,
        max_reg: 0,
        cold: None,
        reg_types: Box::new([]),
        block_index: std::collections::HashMap::new(),
        branch_targets: Vec::new(),
        fused_tails: Vec::new(),
        frame_meta: None,
        resolved: std::sync::OnceLock::new(),
    }
}

#[test]
fn failed_function_resolve_is_remembered() {
    let mut loader = LazyLoader::new(
        Vec::new(), 0,
        vec![("a.zpkg".to_string(), fake_candidate(&["X"]))],
        Vec::new(),
    );
    // `a.zpkg` doesn't exist on disk, so the walk finds nothing and gives up.
    assert!(loader.resolve_function("Foo.Bar.Baz$0").is_none());
    assert!(
        loader.negative.as_ref().unwrap().functions.contains("Foo.Bar.Baz$0"),
        "a full failed walk must be recorded so the next identical lookup is a hash probe",
    );
    // Second call answers from the cache — same result.
    assert!(loader.resolve_function("Foo.Bar.Baz$0").is_none());
}

#[test]
fn failed_type_resolve_is_remembered() {
    let mut loader = LazyLoader::new(Vec::new(), 0, Vec::new(), Vec::new());
    assert!(loader.resolve_type("Some.Missing.Class").is_none());
    assert!(loader.negative.as_ref().unwrap().types.contains("Some.Missing.Class"));
    assert!(loader.resolve_type("Some.Missing.Class").is_none());
}

#[test]
fn negative_cache_is_dropped_when_a_package_registers() {
    let mut loader = LazyLoader::new(
        vec![PathBuf::from("/z42-nonexistent-search-dir")], 0, Vec::new(), Vec::new(),
    );
    // Miss recorded while `Late.Pkg.F$0` genuinely isn't loadable.
    assert!(loader.resolve_function("Late.Pkg.F$0").is_none());
    assert!(loader.negative.as_ref().unwrap().functions.contains("Late.Pkg.F$0"));

    // Registering a package that *does* declare it must not be masked by the
    // stale negative entry — the length fingerprint moves, so the cache drops.
    let mut artifact = ll_inmem_artifact("Late.Pkg", Some("late_pkg"), &[]);
    artifact.module.functions.push(ll_stub_function("Late.Pkg.F$0"));
    loader.register_loaded_artifact(artifact).expect("register late pkg");

    assert!(
        loader.resolve_function("Late.Pkg.F$0").is_some(),
        "a name registered after its miss must resolve — the negative cache is \
         only valid while the append-only registries are unchanged",
    );
    // The stale entry may still sit in the set — `resolve_function` answered from
    // `function_table` before ever probing it. Harmless by construction: the
    // positive table is checked first, and the next *miss* sees the moved
    // fingerprint and drops the whole set.
    assert!(loader.resolve_function("Still.Missing$0").is_none());
    assert!(
        !loader.negative.as_ref().unwrap().functions.contains("Late.Pkg.F$0"),
        "the first miss after a registration drops every stale negative entry",
    );
}

#[test]
fn negative_cache_survives_a_repeat_miss_without_growing_stale() {
    let mut loader = LazyLoader::new(Vec::new(), 0, Vec::new(), Vec::new());
    for _ in 0..3 {
        assert!(loader.resolve_function("A.B.C$0").is_none());
        assert!(loader.resolve_type("A.B").is_none());
    }
    assert_eq!(loader.negative.as_ref().unwrap().functions.len(), 1);
    assert_eq!(loader.negative.as_ref().unwrap().types.len(), 1);
}
