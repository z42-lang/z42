use super::*;
use std::path::Path;

// ── extract_import_namespaces ─────────────────────────────────────────────────

fn ns(imports: &[&str]) -> Vec<String> {
    extract_import_namespaces(&imports.iter().map(|s| s.to_string()).collect::<Vec<_>>())
}

#[test]
fn empty_imports_returns_empty() {
    assert!(ns(&[]).is_empty());
}

#[test]
fn single_import_emits_all_prefixes() {
    // namespace-aware: emit every `.`-bounded prefix so 3+ segment stdlib
    // namespaces (Std.IO.Binary, …) get a chance to match a zpkg, not just
    // the first two segments.
    assert_eq!(
        ns(&["Std.IO.Console.WriteLine"]),
        vec!["Std", "Std.IO", "Std.IO.Console"]
    );
}

#[test]
fn deeper_namespace_emits_three_segment_prefix() {
    // Regression: Std.IO.Binary.BinaryWriter.WriteByte must yield
    // "Std.IO.Binary" so lazy loader pulls in z42.io.binary.zpkg.
    assert_eq!(
        ns(&["Std.IO.Binary.BinaryWriter.WriteByte"]),
        vec!["Std", "Std.IO", "Std.IO.Binary", "Std.IO.Binary.BinaryWriter"]
    );
}

#[test]
fn multiple_imports_same_namespace_deduplicated() {
    assert_eq!(
        ns(&["Std.IO.Console.WriteLine", "Std.IO.File.ReadText"]),
        vec!["Std", "Std.IO", "Std.IO.Console", "Std.IO.File"]
    );
}

#[test]
fn imports_from_different_namespaces_all_returned() {
    let result = ns(&["Std.IO.File.ReadText", "Std.Math.Math.Abs"]);
    assert!(result.contains(&"Std.IO".to_owned()));
    assert!(result.contains(&"Std.Math".to_owned()));
    assert!(result.contains(&"Std".to_owned()));
}

#[test]
fn import_with_one_dot_emits_first_segment() {
    assert_eq!(ns(&["mylib.Foo"]), vec!["mylib"]);
}

#[test]
fn import_with_no_dot_uses_full_name() {
    assert_eq!(ns(&["standalone"]), vec!["standalone"]);
}

// ── resolve_namespace ─────────────────────────────────────────────────────────

/// Build a minimal binary zpkg (indexed, lib) with a STRS + NSPC section.
/// Layout: header(16) + dir(sec_count×12) + META + STRS + NSPC sections.
fn make_fake_zpkg(dir: &Path, filename: &str, namespaces: &[&str]) {
    use crate::metadata::formats::ZPKG_MAGIC;

    // Build STRS section (segment-dict, zbc 1.21): seg_count + (varint len + utf8)×n
    //   + str_count + (varint segN + varint segIdx×segN)×n. Names split on '.'.
    fn wv(out: &mut Vec<u8>, mut v: u32) {
        loop {
            let b = (v & 0x7f) as u8; v >>= 7;
            if v != 0 { out.push(b | 0x80); } else { out.push(b); break; }
        }
    }
    let mut seg_dict: Vec<String> = Vec::new();
    let mut seqs: Vec<Vec<u32>> = Vec::new();
    for s in namespaces {
        let mut seq = Vec::new();
        for part in s.split('.') {
            let idx = seg_dict.iter().position(|x| x == part).unwrap_or_else(|| {
                seg_dict.push(part.to_string());
                seg_dict.len() - 1
            });
            seq.push(idx as u32);
        }
        seqs.push(seq);
    }
    let mut strs_data: Vec<u8> = Vec::new();
    strs_data.extend_from_slice(&(seg_dict.len() as u32).to_le_bytes());
    for seg in &seg_dict { wv(&mut strs_data, seg.len() as u32); strs_data.extend_from_slice(seg.as_bytes()); }
    strs_data.extend_from_slice(&(seqs.len() as u32).to_le_bytes());
    for seq in &seqs { wv(&mut strs_data, seq.len() as u32); for &i in seq { wv(&mut strs_data, i); } }

    // Build NSPC section: count[4] + idx[4] per ns
    let mut nspc_data: Vec<u8> = Vec::new();
    nspc_data.extend_from_slice(&(namespaces.len() as u32).to_le_bytes());
    for i in 0u32..namespaces.len() as u32 {
        nspc_data.extend_from_slice(&i.to_le_bytes());
    }

    // Build META section: name, version, entry (each u16-len + bytes)
    let mut meta_data: Vec<u8> = Vec::new();
    for s in &["test", "0.1.0", ""] {
        let b = s.as_bytes();
        meta_data.extend_from_slice(&(b.len() as u16).to_le_bytes());
        meta_data.extend_from_slice(b);
    }

    // Assemble: 3 sections (META, STRS, NSPC)
    let sections: &[(&[u8; 4], &[u8])] = &[
        (b"META", &meta_data),
        (b"STRS", &strs_data),
        (b"NSPC", &nspc_data),
    ];
    let sec_count = sections.len() as u16;
    let header_size: usize = 16;
    let dir_size: usize = sec_count as usize * 12;
    let mut next_offset = (header_size + dir_size) as u32;

    let mut data: Vec<u8> = Vec::new();
    // Header: magic[4] + major[2] + minor[2] + flags[2] + sec_count[2] + reserved[4]
    data.extend_from_slice(&ZPKG_MAGIC);
    data.extend_from_slice(&0u16.to_le_bytes()); // major
    data.extend_from_slice(&1u16.to_le_bytes()); // minor
    data.extend_from_slice(&0u16.to_le_bytes()); // flags: indexed, lib
    data.extend_from_slice(&sec_count.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes()); // reserved

    // Directory
    for (tag, sec) in sections {
        data.extend_from_slice(*tag);
        data.extend_from_slice(&next_offset.to_le_bytes());
        data.extend_from_slice(&(sec.len() as u32).to_le_bytes());
        next_offset += sec.len() as u32;
    }

    // Section data
    for (_, sec) in sections { data.extend_from_slice(sec); }

    std::fs::write(dir.join(filename), &data).expect("write test zpkg");
}


/// resolve_namespace with empty paths returns an empty vec
#[test]
fn test_resolve_namespace_empty_paths() {
    let result = resolve_namespace("Std.IO", &[]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

/// Two zpkg files in the same libs tier providing the same namespace
/// → both are returned (legit under C# assembly model; disambiguation
/// happens at the lazy-load layer by zpkg file name).
#[test]
fn test_resolve_namespace_ambiguous_returns_both() {
    let tmp = std::env::temp_dir().join(format!("z42_test_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();

    make_fake_zpkg(&tmp, "libA.zpkg", &["z42.conflict"]);
    make_fake_zpkg(&tmp, "libB.zpkg", &["z42.conflict"]);

    let result = resolve_namespace("z42.conflict", &[tmp.clone()]);
    std::fs::remove_dir_all(&tmp).ok();

    assert!(result.is_ok(), "unexpected error: {:?}", result.err());
    let paths = result.unwrap();
    assert_eq!(paths.len(), 2, "both zpkgs should be reported; got {paths:?}");
    let names: std::collections::HashSet<String> = paths
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_owned))
        .collect();
    assert!(names.contains("libA.zpkg"));
    assert!(names.contains("libB.zpkg"));
}


// ── fix-cross-pkg-subclass-fields (2026-05-14) ────────────────────────────────

/// Build a minimal Module containing a single class declaration. Used by
/// fixup tests to simulate per-zpkg load + merge-into-global-registry.
fn module_with_one_class(
    name: &str,
    base: Option<&str>,
    fields: Vec<(&str, &str)>,
) -> crate::metadata::bytecode::Module {
    use crate::metadata::bytecode::{ClassDesc, FieldDesc, Module};
    Module {
        name: name.to_owned(),
        string_pool: vec![],
        classes: vec![ClassDesc {
            static_fields: vec![].into(),
            interfaces: vec![].into(),
            enum_members: vec![].into(),
            iface_methods: vec![].into(),
            struct_layout: None,
            inline_layout: None,
            object_layout: None,
            class_flags: 0,
            visibility: 0,
            class_flags2: 0,
            struct_field_table: Box::new([]),
            name: name.to_owned(),
            base_class: base.map(str::to_owned),
            fields: fields.into_iter().map(|(n, t)| FieldDesc {
                name: n.to_owned(), type_tag: t.to_owned(), attributes: Box::new([]), visibility: 0,
            }).collect(),
            type_params: Box::new([]),
            type_param_constraints: Box::new([]),
            attributes: Box::new([]),
        }],
        functions: vec![],
        type_registry: rustc_hash::FxHashMap::default(),
        func_index: rustc_hash::FxHashMap::default(),
    }
}

/// Cross-zpkg subclass: base in module A, subclass in module B. After
/// `try_fixup_inheritance` runs against the merged registry, the subclass
/// must inherit base's fields at low slot indices.
#[test]
fn fixup_inherits_base_fields_from_separate_module() {
    let mut mod_a = module_with_one_class("Base", None,
        vec![("name", "str"), ("age", "i64")]);
    let mut mod_b = module_with_one_class("Sub", Some("Base"),
        vec![("flag", "bool")]);

    crate::metadata::loader::build_type_registry(&mut mod_a);
    crate::metadata::loader::build_type_registry(&mut mod_b);

    // Before merge, B's Sub has only its own field — base is unresolvable
    // within mod_b's local registry.
    let sub_before = mod_b.type_registry.get("Sub").expect("Sub registered");
    assert_eq!(sub_before.own_fields().len(), 1, "Sub has 1 own field");
    assert_eq!(sub_before.fields.len(), 1, "Sub.fields lacks inherited slots pre-fixup");

    // Simulate lazy_loader merge: copy both modules' TypeDescs into a
    // global registry and run fixup.
    let mut global: rustc_hash::FxHashMap<String, std::sync::Arc<TypeDesc>> =
        rustc_hash::FxHashMap::default();
    for (n, td) in std::mem::take(&mut mod_a.type_registry) { global.insert(n, td); }
    for (n, td) in std::mem::take(&mut mod_b.type_registry) { global.insert(n, td); }

    let fixed = crate::metadata::loader::try_fixup_inheritance(&mut global);
    assert_eq!(fixed, 1, "Sub should be the single newly-fixed type");

    let sub_after = global.get("Sub").expect("Sub still present");
    assert_eq!(sub_after.fields.len(), 3, "fields = base (2) + own (1)");
    assert_eq!(sub_after.field_index.get("name"), Some(&0), "base.name at slot 0");
    assert_eq!(sub_after.field_index.get("age"),  Some(&1), "base.age at slot 1");
    assert_eq!(sub_after.field_index.get("flag"), Some(&2), "own.flag at slot 2");
}

/// Three-level cross-zpkg chain: A.Base → B.Mid → C.Leaf. Leaf must
/// inherit fields from both Mid and Base after fixup converges.
#[test]
fn fixup_handles_three_level_chain() {
    let mut mod_a = module_with_one_class("Base", None, vec![("a", "str")]);
    let mut mod_b = module_with_one_class("Mid",  Some("Base"), vec![("b", "str")]);
    let mut mod_c = module_with_one_class("Leaf", Some("Mid"),  vec![("c", "str")]);

    for m in [&mut mod_a, &mut mod_b, &mut mod_c] {
        crate::metadata::loader::build_type_registry(m);
    }

    let mut global: rustc_hash::FxHashMap<String, std::sync::Arc<TypeDesc>> =
        rustc_hash::FxHashMap::default();
    for m in [&mut mod_a, &mut mod_b, &mut mod_c] {
    }
    for (n, td) in std::mem::take(&mut mod_a.type_registry) { global.insert(n, td); }
    for (n, td) in std::mem::take(&mut mod_b.type_registry) { global.insert(n, td); }
    for (n, td) in std::mem::take(&mut mod_c.type_registry) { global.insert(n, td); }

    // Fixed-point loop: Mid may resolve in pass 1 (base = Base, already
    // present), then Leaf in pass 2 (base = Mid, freshly fixed up).
    let mut total = 0;
    loop {
        let n = crate::metadata::loader::try_fixup_inheritance(&mut global);
        if n == 0 { break; }
        total += n;
    }
    assert!(total >= 2, "Both Mid and Leaf should fix up (got {total})");

    let leaf = global.get("Leaf").expect("Leaf present");
    assert_eq!(leaf.fields.len(), 3, "Leaf.fields = a + b + c");
    assert_eq!(leaf.field_index.get("a"), Some(&0));
    assert_eq!(leaf.field_index.get("b"), Some(&1));
    assert_eq!(leaf.field_index.get("c"), Some(&2));
}

/// Deferred fixup: load B (subclass) before A (base). On B-only fixup,
/// nothing changes. After A loads and fixup re-runs, Sub gets inherited
/// fields.
#[test]
fn fixup_deferred_until_base_loads() {
    let mut mod_b = module_with_one_class("Sub", Some("Base"), vec![("x", "str")]);
    crate::metadata::loader::build_type_registry(&mut mod_b);

    let mut global: rustc_hash::FxHashMap<String, std::sync::Arc<TypeDesc>> =
        rustc_hash::FxHashMap::default();
    for (n, td) in std::mem::take(&mut mod_b.type_registry) { global.insert(n, td); }

    // First fixup pass: base "Base" unresolvable — Sub stays own-only.
    let n1 = crate::metadata::loader::try_fixup_inheritance(&mut global);
    assert_eq!(n1, 0, "no fixup possible without Base");
    assert_eq!(global.get("Sub").unwrap().fields.len(), 1, "Sub still has only its own field");

    // Now A loads, bringing Base into the global registry.
    let mut mod_a = module_with_one_class("Base", None, vec![("b", "str")]);
    crate::metadata::loader::build_type_registry(&mut mod_a);
    for (n, td) in std::mem::take(&mut mod_a.type_registry) { global.insert(n, td); }

    // Second fixup pass: Sub now resolvable, gets inherited slot.
    let n2 = crate::metadata::loader::try_fixup_inheritance(&mut global);
    assert_eq!(n2, 1, "Sub should fix up now that Base is present");
    let sub = global.get("Sub").unwrap();
    assert_eq!(sub.fields.len(), 2);
    assert_eq!(sub.field_index.get("b"), Some(&0), "base.b at slot 0");
    assert_eq!(sub.field_index.get("x"), Some(&1), "own.x  at slot 1");
}

/// Fixup is idempotent: a second pass with no new types changes nothing.
#[test]
fn fixup_idempotent_when_no_new_resolutions() {
    let mut mod_a = module_with_one_class("Base", None, vec![("x", "str")]);
    let mut mod_b = module_with_one_class("Sub", Some("Base"), vec![]);
    for m in [&mut mod_a, &mut mod_b] {
        crate::metadata::loader::build_type_registry(m);
    }
    let mut global: rustc_hash::FxHashMap<String, std::sync::Arc<TypeDesc>> =
        rustc_hash::FxHashMap::default();
    for (n, td) in std::mem::take(&mut mod_a.type_registry) { global.insert(n, td); }
    for (n, td) in std::mem::take(&mut mod_b.type_registry) { global.insert(n, td); }

    let pass1 = crate::metadata::loader::try_fixup_inheritance(&mut global);
    assert!(pass1 >= 1);
    let pass2 = crate::metadata::loader::try_fixup_inheritance(&mut global);
    assert_eq!(pass2, 0, "second pass is no-op");
}

/// Regression (fix-z42c-load-fixup-loop): a class carrying *duplicate* own
/// field names must still converge. `merge_with_base` dedups own fields by
/// name (a merged object can't have two slots of the same name), so the
/// merged layout has fewer fields than the raw own-field count. If
/// `needs_fixup` counted own fields with multiplicity it would forever
/// disagree with the merged length → the loader's fixed-point loop spins at
/// 100% CPU. Base carries `b` so Sub genuinely needs an inheritance fixup;
/// Sub declares `dup` twice (3 own, 2 distinct). Pre-fix `needs_fixup`
/// expected base(1)+own-with-multiplicity(3)=4 but merge produces 3 → never
/// converges. Post-fix expected base(1)+distinct(2)=3 == 3 → converges.
#[test]
fn fixup_converges_with_duplicate_field_names() {
    let mut mod_a = module_with_one_class("Base", None, vec![("b", "str")]);
    let mut mod_b = module_with_one_class("Sub", Some("Base"),
        vec![("dup", "str"), ("dup", "str"), ("x", "i64")]);
    for m in [&mut mod_a, &mut mod_b] {
        crate::metadata::loader::build_type_registry(m);
    }
    let mut global: rustc_hash::FxHashMap<String, std::sync::Arc<TypeDesc>> =
        rustc_hash::FxHashMap::default();
    for (n, td) in std::mem::take(&mut mod_a.type_registry) { global.insert(n, td); }
    for (n, td) in std::mem::take(&mut mod_b.type_registry) { global.insert(n, td); }

    // First pass merges Sub against Base; subsequent passes MUST reach the
    // fixed point (0) rather than re-fixing `Sub` every round.
    let pass1 = crate::metadata::loader::try_fixup_inheritance(&mut global);
    assert_eq!(pass1, 1, "Sub fixed up once against Base");
    let pass2 = crate::metadata::loader::try_fixup_inheritance(&mut global);
    assert_eq!(pass2, 0, "converged: duplicate field name must not re-trigger fixup");

    let sub = global.get("Sub").unwrap();
    assert_eq!(sub.fields.len(), 3, "merged layout = base(b) + own deduped (dup, x)");
    assert_eq!(sub.field_index.get("b"),   Some(&0), "base.b at slot 0");
    assert_eq!(sub.field_index.get("dup"), Some(&1));
    assert_eq!(sub.field_index.get("x"),   Some(&2));
}

// ── aggregate_zpkg_test_index (aggregate-zpkg-tidx, 2026-06-06) ──────────────
//
// These exercise the pure aggregation helper. End-to-end loader path
// (decode zpkg → aggregate → resolve strings) is covered by the
// xtask test stdlib smoke once a Phase 5 dir-mode demo lands; here we
// test the offset math in isolation against hand-constructed input.

use crate::metadata::bytecode::Module;
use crate::metadata::test_index::{TestEntry, TestEntryKind, TestFlags, TestCase};

/// Build a fresh empty Module with `func_count` synthetic Function
/// stubs and `str_count` placeholder strings — just enough for the
/// aggregator to compute offsets against. Functions / strings carry
/// no semantics; the aggregator only reads `module.functions.len()`
/// and `module.string_pool.len()`.
fn make_stub_module(func_count: usize, str_count: usize) -> Module {
    let functions = (0..func_count)
        .map(|i| crate::metadata::bytecode::Function {
            name:         format!("f{i}"),
            param_count:  0,
            ret_type:     "void".to_owned(),
            exec_mode:    crate::metadata::ExecMode::Interp,
            blocks:       vec![],
            is_static:    false,
            visibility:   0,
            method_flags: 0, min_arg: 0, params_from: 0xFF,
            max_reg:      0,
            cold:         None,
            reg_types:    Box::new([]),
            block_index:  std::collections::HashMap::new(),
            branch_targets: Vec::new(),
            fused_tails: Vec::new(),
            frame_meta: None,
            resolved:     std::sync::OnceLock::new(),
            owner_init:     Default::default(),
        })
        .collect();
    let string_pool = (0..str_count).map(|i| format!("s{i}")).collect();
    Module {
        name: "stub".to_owned(),
        string_pool,
        classes: vec![],
        functions,
        type_registry: rustc_hash::FxHashMap::default(),
        func_index: rustc_hash::FxHashMap::default(),
    }
}


fn empty_entry(method_id: u32, skip_reason_str_idx: u32) -> TestEntry {
    TestEntry {
        method_id,
        kind: TestEntryKind::Test,
        flags: TestFlags::empty(),
        skip_reason_str_idx,
        skip_platform_str_idx: 0,
        skip_feature_str_idx: 0,
        expected_throw_type_idx: 0,
        test_cases: vec![],
        timeout_ms: 0,
        skip_reason: None,
        skip_platform: None,
        skip_feature: None,
        expected_throw_type: None,
    }
}

#[test]
fn aggregate_zpkg_tidx_empty_module_list_yields_empty_vec() {
    let triples: Vec<(Module, String, Vec<TestEntry>)> = vec![];
    let result = crate::metadata::loader::aggregate_zpkg_test_index(&triples).unwrap();
    assert!(result.is_empty());
}

#[test]
fn aggregate_zpkg_tidx_single_module_no_offset() {
    let module = make_stub_module(3, 10);
    let entries = vec![empty_entry(1, 0), empty_entry(2, 5)];
    let triples = vec![(module, "ns".to_owned(), entries)];

    let result = crate::metadata::loader::aggregate_zpkg_test_index(&triples).unwrap();
    assert_eq!(result.len(), 2);
    // Single module → cumulative function offset is 0.
    assert_eq!(result[0].method_id, 1);
    assert_eq!(result[1].method_id, 2);
    // 字符串索引**原样保留** —— 聚合不再碰它（解析发生在读取点，见 fix-tidx-strings-in-zpkg）。
    assert_eq!(result[1].skip_reason_str_idx, 5);
}

#[test]
fn aggregate_zpkg_tidx_multi_module_method_id_remap() {
    // Module 0: 3 functions, 1 test pointing at fn 2.
    let m0 = make_stub_module(3, 0);
    let m0_tidx = vec![empty_entry(2, 0)];
    // Module 1: 5 functions, 2 tests at local fn 0 and fn 4.
    let m1 = make_stub_module(5, 0);
    let m1_tidx = vec![empty_entry(0, 0), empty_entry(4, 0)];

    let triples = vec![
        (m0, "a".to_owned(), m0_tidx),
        (m1, "b".to_owned(), m1_tidx),
    ];
    let result = crate::metadata::loader::aggregate_zpkg_test_index(&triples).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].method_id, 2);     // M0: 2 + 0
    assert_eq!(result[1].method_id, 0 + 3); // M1: 0 + cum(M0)=3
    assert_eq!(result[2].method_id, 4 + 3); // M1: 4 + cum(M0)=3
}

#[test]
fn aggregate_zpkg_tidx_multi_module_str_idx_is_left_alone() {
    // fix-tidx-strings-in-zpkg：**契约反转**。此前聚合会给 `*_str_idx` 叠加「累计字符串
    // 偏移」，那是错的 —— 两条独立理由：
    //   ① 打包 zpkg 的 TIDX 索引**已经是全局的**（写入端把每模块索引 remap 进了共享池），
    //      再叠一层偏移是把对的推错；
    //   ② 那个偏移取自 `module.string_pool.len()`，而它是 `rebuild_string_pool` **之后**的池，
    //      只被 TIDX 引用的串（`[ShouldThrow<E>]` 类型链、`[Skip(reason)]` 文案）在重建时就没了。
    // 现在字符串在**读取点**就解析好（打包态对全局 raw 池、索引态对各散装 zbc 的 raw 池），
    // 聚合只负责 `method_id` 的函数偏移，索引原样带过。
    let m0 = make_stub_module(0, 10);
    let m0_tidx = vec![empty_entry(0, 4)];
    let m1 = make_stub_module(0, 5);
    let m1_tidx = vec![empty_entry(0, 3)];

    let triples = vec![
        (m0, "a".to_owned(), m0_tidx),
        (m1, "b".to_owned(), m1_tidx),
    ];
    let result = crate::metadata::loader::aggregate_zpkg_test_index(&triples).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].skip_reason_str_idx, 4);
    assert_eq!(result[1].skip_reason_str_idx, 3, "第二个模块的索引也不得被偏移");
    assert_eq!(result[0].skip_platform_str_idx, 0);
    assert_eq!(result[1].skip_platform_str_idx, 0);
}

fn aggregate_zpkg_tidx_zero_len_skips_without_panic() {
    let m0 = make_stub_module(2, 4);
    let m1 = make_stub_module(2, 4);
    let m2 = make_stub_module(2, 4);
    // Middle module has no TIDX bytes but still contributes to the
    // cumulative function / string offsets.
    let m0_tidx = vec![empty_entry(1, 0)];
    let m1_tidx: Vec<TestEntry> = vec![]; // empty
    let m2_tidx = vec![empty_entry(1, 0)];

    let triples = vec![
        (m0, "a".to_owned(), m0_tidx),
        (m1, "b".to_owned(), m1_tidx),
        (m2, "c".to_owned(), m2_tidx),
    ];
    let result = crate::metadata::loader::aggregate_zpkg_test_index(&triples).unwrap();
    assert_eq!(result.len(), 2);
    // First entry from M0: offset 0 → method_id = 1.
    assert_eq!(result[0].method_id, 1);
    // Second entry from M2: cum offset = M0.funcs (2) + M1.funcs (2) = 4
    // → method_id = 1 + 4 = 5. M1's empty TIDX still advanced the offset.
    assert_eq!(result[1].method_id, 5);
}

#[test]
fn aggregate_zpkg_tidx_test_case_arg_repr_is_left_alone() {
    // 同上（fix-tidx-strings-in-zpkg）：`arg_repr_str_idx` 也属于「读取点已解析」的字符串，
    // 聚合不得再叠加累计偏移。此前这里断言 2 + 6 = 8。
    let m0 = make_stub_module(0, 6);
    let m0_tidx: Vec<TestEntry> = vec![];
    let m1 = make_stub_module(0, 4);
    let mut entry = empty_entry(0, 0);
    entry.test_cases = vec![TestCase { arg_repr_str_idx: 2 }];
    let m1_tidx = vec![entry];

    let triples = vec![
        (m0, "a".to_owned(), m0_tidx),
        (m1, "b".to_owned(), m1_tidx),
    ];
    let result = crate::metadata::loader::aggregate_zpkg_test_index(&triples).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].test_cases.len(), 1);
    assert_eq!(result[0].test_cases[0].arg_repr_str_idx, 2);
}

// ── indexed zpkg load（add-indexed-zpkg-min-patch，zpkg 0.24）─────────────────

/// committed fixture: indexed 主文件 + 散装自包含 zbc（src/compiler/z42.package/tests/fixtures/zpkg-format/indexed-minimal）。
fn indexed_fixture_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../compiler/z42.package/tests/fixtures/zpkg-format/indexed-minimal")
}

#[test]
fn indexed_zpkg_loads_via_path() {
    // 主文件 + source.zbc 拷到临时目录（fixture 目录保持只读语义）。
    let base = std::env::temp_dir().join("z42-indexed-load-ok");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    std::fs::copy(indexed_fixture_dir().join("source.zpkg"), base.join("demo.indexed.zpkg")).unwrap();
    std::fs::copy(indexed_fixture_dir().join("source.zbc"), base.join("source.zbc")).unwrap();

    let art = crate::metadata::loader::load_artifact(
        base.join("demo.indexed.zpkg").to_str().unwrap(),
    )
    .expect("indexed zpkg loads through the path-aware loader");
    assert!(!art.module.functions.is_empty(), "scattered zbc functions merged");
}

#[test]
fn indexed_zpkg_rejects_hash_mismatch() {
    let base = std::env::temp_dir().join("z42-indexed-load-bad");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    std::fs::copy(indexed_fixture_dir().join("source.zpkg"), base.join("demo.indexed.zpkg")).unwrap();
    let mut zbc = std::fs::read(indexed_fixture_dir().join("source.zbc")).unwrap();
    zbc.push(0x78); // 篡改散装 zbc → 内容 hash 必不符
    std::fs::write(base.join("source.zbc"), zbc).unwrap();

    let err = match crate::metadata::loader::load_artifact(
        base.join("demo.indexed.zpkg").to_str().unwrap(),
    ) {
        Ok(_) => panic!("tampered scattered zbc must be rejected"),
        Err(e) => e,
    };
    assert!(format!("{err:#}").contains("hash mismatch"), "error names the mismatch: {err:#}");
}

#[test]
fn indexed_zpkg_bytes_only_load_is_rejected() {
    // 字节 API（embedding）无包目录 → 明确拒绝而非静默。
    let raw = std::fs::read(indexed_fixture_dir().join("source.zpkg")).unwrap();
    let err = match crate::metadata::loader::load_artifact_from_bytes(&raw) {
        Ok(_) => panic!("indexed from bytes alone must be rejected"),
        Err(e) => e,
    };
    assert!(format!("{err:#}").contains("load it by path"), "{err:#}");
}

// ── unify-object-byte-layout (PR-2, task 2.0): composed object layout ─────────
//
// These verify that `build_type_registry` / `try_fixup_inheritance` compose a
// class's own-only zbc `object_layout` with its base's composed layout into the
// runtime `composed_object_layout` (dormant — not yet consumed for storage),
// mirroring `fields = base.fields ++ own`. The own region starts at
// `align_up(base.size, 8)` (unified 8B inheritance boundary).

/// Build a one-class Module whose single class carries an own-only object layout.
fn module_with_object_layout(
    name: &str,
    base: Option<&str>,
    fields: Vec<(&str, &str)>,
    layout: crate::metadata::bytecode::ObjectLayoutDesc,
) -> crate::metadata::bytecode::Module {
    let mut m = module_with_one_class(name, base, fields);
    m.classes[0].object_layout = Some(layout);
    m
}

/// Local (same-module) base→derived: topo order builds the base first, so the
/// derived class composes against the base's composed layout at build time.
#[test]
fn object_layout_composed_local_inheritance() {
    use crate::metadata::bytecode::{ClassDesc, Module, ObjectLayoutDesc};
    use crate::metadata::types::{STRUCT_REF_ARC_STRING, STRUCT_REF_GCREF};

    // Base { i64 age@0; str name@8 } — size 16, one ref leaf (name @ 8).
    let base_layout = ObjectLayoutDesc {
        size: 16,
        field_offsets: Box::new([0, 8]),
        field_sizes:   Box::new([8, 8]),
        field_kinds:   Box::new([0, STRUCT_REF_ARC_STRING]),
        ref_offsets:   Box::new([8]),
        ref_kinds:     Box::new([STRUCT_REF_ARC_STRING]),
    };
    // Sub : Base { bool flag@0; object other@8 } — own size 16, ref leaf (other @ 8).
    let sub_layout = ObjectLayoutDesc {
        size: 16,
        field_offsets: Box::new([0, 8]),
        field_sizes:   Box::new([1, 8]),
        field_kinds:   Box::new([0, STRUCT_REF_GCREF]),
        ref_offsets:   Box::new([8]),
        ref_kinds:     Box::new([STRUCT_REF_GCREF]),
    };

    // Two classes in one module (topo: Base before Sub).
    let mut base_mod = module_with_object_layout("Base", None,
        vec![("age", "i64"), ("name", "str")], base_layout);
    let sub_mod = module_with_object_layout("Sub", Some("Base"),
        vec![("flag", "bool"), ("other", "obj")], sub_layout);
    // Merge Sub's class into base_mod so build_type_registry sees both.
    let sub_class: ClassDesc = sub_mod.classes.into_iter().next().unwrap();
    base_mod.classes.push(sub_class);
    let mut module: Module = base_mod;

    crate::metadata::loader::build_type_registry(&mut module);

    let base = module.type_registry.get("Base").expect("Base");
    let bl = base.composed_object_layout().expect("Base has composed layout");
    assert_eq!(bl.size, 16, "Base composed size = 16");
    assert_eq!(&*bl.field_offsets, &[0, 8]);
    assert_eq!(&*bl.ref_offsets, &[8]);

    let sub = module.type_registry.get("Sub").expect("Sub");
    let sl = sub.composed_object_layout().expect("Sub has composed layout");
    // base_shift = align_up(16, 8) = 16.
    assert_eq!(sl.size, 32, "Sub composed size = base(16) + own(16)");
    assert_eq!(&*sl.field_offsets, &[0, 8, 16, 24], "base fields kept, own shifted +16");
    assert_eq!(&*sl.field_sizes, &[8, 8, 1, 8]);
    assert_eq!(&*sl.field_kinds, &[0, STRUCT_REF_ARC_STRING, 0, STRUCT_REF_GCREF]);
    // PR-3 chunk 2b: the base string leaf @8 stays in the side-table; Sub's own direct
    // object (GcRef) field @8→24 is byte-inlined, so it drops out of `ref_offsets` and
    // shows up in `inline_refs` instead.
    assert_eq!(&*sl.ref_offsets, &[8], "only base's string ref stays side-table");
    assert_eq!(&*sl.ref_kinds, &[STRUCT_REF_ARC_STRING]);
    assert_eq!(sl.inline_refs.len(), 1, "Sub's own object field inlined");
    assert_eq!(sl.inline_refs[0].offset, 24, "own object ref shifted to 24 and inlined");
    assert!(!sl.inline_refs[0].is_array);
    // ref_index maps composed offsets to side-table slots (inlined refs are absent).
    assert_eq!(sl.ref_index(8), Some(0));
    assert_eq!(sl.ref_index(24), None, "inlined object ref has no side-table slot");
    assert_eq!(sl.ref_index(16), None, "non-ref field offset not in bitmap");

    // Field-index parity with the merged `fields` (offsets align by slot).
    assert_eq!(sub.field_index.get("age"),  Some(&0));
    assert_eq!(sub.field_index.get("name"), Some(&1));
    assert_eq!(sub.field_index.get("flag"), Some(&2));
    assert_eq!(sub.field_index.get("other"), Some(&3));

    // D12: per-field access table resolves exact tags + refs slots from type_tag.
    use crate::metadata::types::{TAG_I64, TAG_STR, TAG_OBJECT};
    let fa = &sl.field_access;
    assert_eq!(fa.len(), 4, "one FieldAccess per merged field");
    // age: i64 primitive @ 0, not a ref.
    assert_eq!((fa[0].offset, fa[0].tag, fa[0].ref_slot), (0, TAG_I64, -1));
    // name: str ref @ 8 → refs slot 0 (strings stay in the side-table).
    assert_eq!((fa[1].offset, fa[1].tag, fa[1].ref_slot), (8, TAG_STR, 0));
    // flag: bool primitive @ 16 (shifted), not a ref.
    assert_eq!(fa[2].offset, 16);
    assert_eq!(fa[2].ref_slot, -1);
    // other: object ref @ 24 → PR-3 chunk 2b byte-inlined (ref_slot -1, read as an 8B
    // pointer from `bytes`), so it has no side-table slot.
    assert_eq!((fa[3].offset, fa[3].tag, fa[3].ref_slot), (24, TAG_OBJECT, -1));
}

/// Cross-zpkg base→derived: the base is unresolvable at the derived module's
/// build time (own-only, base_shift 0); `try_fixup_inheritance` recomposes once
/// the base joins the global registry — in lockstep with `fields`.
#[test]
fn object_layout_composed_crosspkg_fixup() {
    use crate::metadata::bytecode::ObjectLayoutDesc;
    use crate::metadata::types::STRUCT_REF_ARC_STRING;

    // Base { str name@0 } — size 8, ref leaf @ 0.
    let base_layout = ObjectLayoutDesc {
        size: 8,
        field_offsets: Box::new([0]),
        field_sizes:   Box::new([8]),
        field_kinds:   Box::new([STRUCT_REF_ARC_STRING]),
        ref_offsets:   Box::new([0]),
        ref_kinds:     Box::new([STRUCT_REF_ARC_STRING]),
    };
    // Sub : Base { i64 n@0 } — own size 8, no refs.
    let sub_layout = ObjectLayoutDesc {
        size: 8,
        field_offsets: Box::new([0]),
        field_sizes:   Box::new([8]),
        field_kinds:   Box::new([0]),
        ref_offsets:   Box::new([]),
        ref_kinds:     Box::new([]),
    };

    let mut mod_a = module_with_object_layout("Base", None, vec![("name", "str")], base_layout);
    let mut mod_b = module_with_object_layout("Sub", Some("Base"), vec![("n", "i64")], sub_layout);
    crate::metadata::loader::build_type_registry(&mut mod_a);
    crate::metadata::loader::build_type_registry(&mut mod_b);

    // Pre-merge: Sub composed against an unresolved base → own-only (base_shift 0).
    let sub_before = mod_b.type_registry.get("Sub").expect("Sub").composed_object_layout().unwrap();
    assert_eq!(sub_before.size, 8, "own-only before fixup");
    assert_eq!(&*sub_before.field_offsets, &[0], "own field at 0 (no base region yet)");

    // Merge into a global registry + fixup.
    let mut global: rustc_hash::FxHashMap<String, std::sync::Arc<TypeDesc>> =
        rustc_hash::FxHashMap::default();
    for (n, td) in std::mem::take(&mut mod_a.type_registry) { global.insert(n, td); }
    for (n, td) in std::mem::take(&mut mod_b.type_registry) { global.insert(n, td); }

    let fixed = crate::metadata::loader::try_fixup_inheritance(&mut global);
    assert_eq!(fixed, 1, "Sub recomposed once base resolves");

    let sub_after = global.get("Sub").unwrap().composed_object_layout().unwrap();
    // base_shift = align_up(8, 8) = 8. Composed = base(8) + own(8) = 16.
    assert_eq!(sub_after.size, 16, "recomposed size = base(8) + own(8)");
    assert_eq!(&*sub_after.field_offsets, &[0, 8], "base ref field @0, own @8");
    assert_eq!(&*sub_after.field_kinds, &[STRUCT_REF_ARC_STRING, 0]);
    assert_eq!(&*sub_after.ref_offsets, &[0], "only the base's ref leaf @0");
    assert_eq!(&*sub_after.ref_kinds, &[STRUCT_REF_ARC_STRING]);
}

// ── TypeId global uniqueness (fix-crosspkg-typeid-collision, 2026-09-08) ──────

/// A minimal `Module` holding `names` as field-less, base-less classes.
fn module_with_class_names(name: &str, names: &[&str]) -> crate::metadata::bytecode::Module {
    use crate::metadata::bytecode::{ClassDesc, Module};
    Module {
        name: name.to_owned(),
        string_pool: vec![],
        classes: names.iter().map(|n| ClassDesc {
            static_fields: vec![].into(),
            interfaces: vec![].into(),
            enum_members: vec![].into(),
            iface_methods: vec![].into(),
            struct_layout: None,
            inline_layout: None,
            object_layout: None,
            class_flags: 0,
            visibility: 0,
            class_flags2: 0,
            struct_field_table: Box::new([]),
            name: (*n).to_owned(),
            base_class: None,
            fields: Box::new([]),
            type_params: Box::new([]),
            type_param_constraints: Box::new([]),
            attributes: Box::new([]),
        }).collect(),
        functions: vec![],
        type_registry: rustc_hash::FxHashMap::default(),
        func_index: rustc_hash::FxHashMap::default(),
    }
}

/// `TypeId`s must be unique across **modules**, not just within one.
///
/// They used to restart at 0 per module, which made e.g. z42c.driver's 1st class and
/// z42c.semantics' 1st class indistinguishable to `VCallIC` / `FieldIC` — both key on the
/// bare `u32`, and nothing renumbers a `TypeDesc` when it crosses a zpkg boundary. A call
/// site reached by both then ran the wrong method against the wrong fields, silently.
/// Observed for real: deleting one unrelated class from z42.core lined the ids up and the
/// bootstrap chain died inside `File.ReadAllText(<CompilationUnit>)`.
///
/// This test fails on the pre-fix allocator: both modules would hand out {0, 1}.
#[test]
fn type_ids_are_unique_across_modules() {
    use std::collections::HashSet;

    let mut a = module_with_class_names("PkgA", &["A.One", "A.Two", "A.Three"]);
    let mut b = module_with_class_names("PkgB", &["B.One", "B.Two", "B.Three"]);
    crate::metadata::loader::build_type_registry(&mut a);
    crate::metadata::loader::build_type_registry(&mut b);

    let ids = |m: &crate::metadata::bytecode::Module| -> HashSet<u32> {
        m.type_registry.values().map(|td| td.id.0).collect()
    };
    let (ids_a, ids_b) = (ids(&a), ids(&b));
    assert_eq!(ids_a.len(), 3, "each class gets its own id");
    assert_eq!(ids_b.len(), 3);
    assert!(
        ids_a.is_disjoint(&ids_b),
        "TypeIds must be globally unique; PkgA got {ids_a:?}, PkgB got {ids_b:?}"
    );
}

/// Ids stay inside the low band — the import-table / primitive / sentinel bands above
/// `IMPORT_BASE` must never be handed out as real type ids.
#[test]
fn allocated_type_ids_stay_below_import_base() {
    let mut m = module_with_class_names("PkgC", &["C.One", "C.Two"]);
    crate::metadata::loader::build_type_registry(&mut m);
    for td in m.type_registry.values() {
        assert!(
            td.id.0 < crate::metadata::tokens::IMPORT_BASE,
            "type id {} escaped the intra-module band", td.id.0
        );
        assert!(td.id.is_resolved());
    }
}

// ── symbolic-struct-field-access P0 ─────────────────────────────────────────
//
// zbc 1.45 的 TYPE 段 struct **字段表**（`ClassDesc.struct_field_table`，#903 落地）要流到
// 运行期的 `StructTypeLayout::fields`，供后续「指令带**字段序号**而不是烘焙偏移」用。
//
// 🔴 **为什么非要测**：这张表目前是**休眠元数据**（P0 没有任何消费方，接通是 P2）。
// 休眠字段最可能的结局就是「填错了也没人知道」—— 等 P2 接通时症状会是**静默错偏移**，
// 而那时离现场已经很远。趁 P0 就把「从 zbc 到 layout 逐格一致」钉住。

/// 造一个带 struct 字段表的单类模块。
fn module_with_struct_field_table(
    class: &str, size: u32, entries: &[(u32, u32, u8)],
) -> crate::metadata::bytecode::Module {
    use crate::metadata::bytecode::{FieldDesc, StructFieldEntry, StructLayoutDesc, CLASS_FLAGS2_HAS_STRUCT_FIELD_TABLE};
    let mut m = module_with_class_names("m", &[class]);
    let c = &mut m.classes[0];
    c.struct_layout = Some(StructLayoutDesc {
        size,
        ref_offsets: Box::new([]),
        ref_kinds: Box::new([]),
    });
    c.class_flags2 = CLASS_FLAGS2_HAS_STRUCT_FIELD_TABLE;
    c.struct_field_table = entries.iter()
        .map(|&(offset, size, kind)| StructFieldEntry { offset, size, kind })
        .collect();
    // symbolic-struct-field-access P2 (T1)：zbc 1.45 的约定是这张表**同序平行于 `fields`**。
    // 这个 helper 原本只填表、不填 `fields` —— 一个**真实 zbc 里不可能出现**的形状，
    // 于是 #915 的测试当初验的是一张「没有平行字段列表」的表。P2 把那条约定变成了正确性
    // 前提（偏移取自表、下一跳类型名取自 `fields[i].type_tag`），载入期已加断言，
    // 所以这里必须造出**同长**的 `fields`，夹具才对得上真实形态。
    c.fields = entries.iter().enumerate()
        .map(|(i, _)| FieldDesc {
            name: format!("f{i}"),
            type_tag: "int".to_string(),
            attributes: Box::new([]),
            visibility: 0,
        })
        .collect();
    m
}

#[test]
fn struct_field_table_reaches_the_runtime_layout() {
    // `struct P3 { int a; long b; double c; }` 形状的一张表。
    let entries = [(0u32, 4u32, 0u8), (8, 8, 0), (16, 8, 0)];
    let mut m = module_with_struct_field_table("Demo.P3", 24, &entries);
    build_type_registry(&mut m);

    let td = m.type_registry.get("Demo.P3").expect("类型进了注册表");
    let layout = td.struct_layout().expect("struct_layout 在");

    assert_eq!(layout.size, 24);
    assert_eq!(layout.field_count(), 3, "字段数必须与 zbc 表一致");
    for (i, &(off, sz, kind)) in entries.iter().enumerate() {
        assert_eq!(layout.field_offset(i), Some(off), "字段 {i} 的偏移");
        let f = layout.field_at(i).expect("字段 {i} 在");
        assert_eq!((f.offset, f.size, f.kind), (off, sz, kind), "字段 {i} 逐格一致");
    }
    assert_eq!(layout.field_offset(3), None, "越界返回 None，不 panic");
}

/// 没带字段表的类型 ⇒ 空表（**不是** panic、也不是「0 个字段」的错觉）。
///
/// ⚠️ 这条原本写着「旧产物（zbc < 1.45）与 size-only 兜底都走这条」——**前半句不可达**：
/// 版本是严格钉死的（`versions.rs:357` 的 `minor != ZBC_VERSION_MINOR` 直接 `Err`，
/// 文档也写着 "the only one it reads"），1.45 之前的产物根本进不了载入期。
/// 活着的理由只剩 `resolve_layout` 的 size-only 兜底。
/// （留着那半句会和隔壁覆盖门的判据前提直接打架——那正是「判据腐坏」的起点。）
#[test]
fn absent_struct_field_table_yields_an_empty_one() {
    use crate::metadata::bytecode::StructLayoutDesc;
    let mut m = module_with_class_names("m", &["Demo.Old"]);
    m.classes[0].struct_layout = Some(StructLayoutDesc {
        size: 8, ref_offsets: Box::new([]), ref_kinds: Box::new([]),
    });
    // class_flags2 = 0、struct_field_table 空 —— 即 1.45 之前的产物形态
    build_type_registry(&mut m);

    let layout = m.type_registry.get("Demo.Old").unwrap().struct_layout().unwrap();
    assert_eq!(layout.field_count(), 0);
    assert_eq!(layout.field_offset(0), None, "空表查任何序号都是 None");
}

/// 🔴 `inline_layout` 的字段表必须**留空** —— 两张表的偏移基准不同。
///
/// `struct_field_table` 描述的是该 struct **类型自身**的字段布局（基准 = blob 起始）；
/// `inline_layout` 是「该 class 把 struct 内联进对象之后」的合成布局（基准 = 对象起始）。
/// 拿前者去填后者会得到一张**偏移全错**的表，而它一旦被 P2 消费就是**静默错值**。
/// 这条钉住那个「顺手复用」的诱惑。
#[test]
fn inline_layout_does_not_borrow_the_struct_field_table() {
    use crate::metadata::bytecode::{StructFieldEntry, StructLayoutDesc, CLASS_FLAGS2_HAS_STRUCT_FIELD_TABLE};
    let mut m = module_with_class_names("m", &["Demo.Holder"]);
    let c = &mut m.classes[0];
    c.inline_layout = Some(StructLayoutDesc {
        size: 16, ref_offsets: Box::new([]), ref_kinds: Box::new([]),
    });
    c.class_flags2 = CLASS_FLAGS2_HAS_STRUCT_FIELD_TABLE;
    c.struct_field_table = Box::new([StructFieldEntry { offset: 0, size: 4, kind: 0 }]);
    build_type_registry(&mut m);

    let td = m.type_registry.get("Demo.Holder").unwrap();
    let inline = td.inline_layout().expect("inline_layout 在");
    assert_eq!(
        inline.field_count(), 0,
        "inline_layout 不得借用 struct_field_table —— 两张表的偏移基准不同"
    );
}

/// symbolic-struct-field-access P2 (T1) 的**正面对照**。
///
/// 载入期那条断言的全部价值在于「不平行时会响」。而真实语料里它**永远不该响**
/// （写端两张表同源），所以「全仓零命中」既是期望结果、也正因此**不能**证明门还活着 ——
/// 只有这个故意造出不平行的用例能。
///
/// ⚠️ 门是 `debug_assert!`（政策同 `__box_prim` / `prim_value_mismatch`：只有编译器或
/// 写端能违反 ⇒ 不是用户的错 ⇒ debug 响、release 放行），所以对照也只在 debug 存在。
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "index-parallel to `fields`")]
fn struct_field_table_that_is_not_parallel_to_fields_is_rejected() {
    use crate::metadata::bytecode::{StructFieldEntry, StructLayoutDesc, CLASS_FLAGS2_HAS_STRUCT_FIELD_TABLE};
    // 表有 3 条，`fields` 只有 1 个 ⇒ 第 2/3 跳会沿着不存在的字段找类型名。
    let mut m = module_with_struct_field_table("Demo.Skew", 24, &[(0, 4, 0), (8, 8, 0), (16, 8, 0)]);
    let c = &mut m.classes[0];
    c.fields = vec![crate::metadata::bytecode::FieldDesc {
        name: "only".to_string(), type_tag: "int".to_string(),
        attributes: Box::new([]), visibility: 0,
    }].into();
    // 形状自身仍然自洽，只有「两张表同长」这一条不变式被破坏。
    let _ = StructFieldEntry { offset: 0, size: 4, kind: 0 };
    let _ = StructLayoutDesc { size: 24, ref_offsets: Box::new([]), ref_kinds: Box::new([]) };
    let _ = CLASS_FLAGS2_HAS_STRUCT_FIELD_TABLE;
    build_type_registry(&mut m);
}

// ── 字段表覆盖门（`check_struct_field_table_coverage`）的三个对照 ───────────────
//
// 🔴 这道门堵的是一个**验证缺口**：P0（#915）漏给「实例化」路径填表时，T1 的反向对照
// 验的是「每条程序都加载了**某个**带表的 struct」——声明路径的表还在，于是全绿。
// 覆盖门问的是「**每个需要的**类型都有表吗」，那才是当时该问的问题。
//
// ⚠️ 门是 `debug_assert!`（只有编译器/写端能违反 ⇒ 政策同 `__box_prim` / `prim_value_mismatch`），
// 所以两条正面对照只在 debug 存在；阴性对照两个 profile 都要在。

/// 正面对照①：blob struct 有字段、却带着一张空表 ⇒ 必须响。
///
/// 这正是 #915 的形态（实例化路径没填表），当时以
/// 「field index 0 out of range for struct `Demo.Pair<int,long>` (0 field(s))」
/// 的形式在**消费方**炸出来，离现场很远。
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "empty per-field layout table")]
fn value_struct_with_fields_but_no_field_table_is_rejected() {
    use crate::metadata::bytecode::{CLASS_FLAG_STRUCT, FieldDesc, StructLayoutDesc};
    let mut m = module_with_class_names("m", &["Demo.NoTable"]);
    let c = &mut m.classes[0];
    c.class_flags = CLASS_FLAG_STRUCT;
    c.struct_layout = Some(StructLayoutDesc {
        size: 4, ref_offsets: Box::new([]), ref_kinds: Box::new([]),
    });
    c.fields = vec![FieldDesc {
        name: "X".to_string(), type_tag: "int".to_string(),
        attributes: Box::new([]), visibility: 0,
    }].into();
    // class_flags2 留 0、struct_field_table 留空 ⇒ 正是写端漏填的形态。
    build_type_registry(&mut m);
}

/// 正面对照②：blob struct 有字段、却**整个 struct 块都没有** ⇒ 必须响，且先报这一条。
///
/// 它不是假想形态：`struct Single { int X; }` 今天就落在这里 —— `PrimModel.Canon`
/// 把用户类型名 `Single` 折叠成基元 `float`，`Layouts.IsStructType` 于是 miss、
/// 布局一个字都没算，而 writer 仍按 `class_flags` bit2 无条件写出一个 size 0 的空块。
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "ships no struct byte layout block")]
fn value_struct_with_fields_but_no_struct_block_is_rejected() {
    use crate::metadata::bytecode::{CLASS_FLAG_STRUCT, FieldDesc};
    let mut m = module_with_class_names("m", &["Demo.NoBlock"]);
    let c = &mut m.classes[0];
    c.class_flags = CLASS_FLAG_STRUCT;
    c.fields = vec![FieldDesc {
        name: "X".to_string(), type_tag: "int".to_string(),
        attributes: Box::new([]), visibility: 0,
    }].into();
    build_type_registry(&mut m);
}

/// 阴性对照：**零字段**的值 struct 带空表是合法的，门不得响。
///
/// 分辨「门在挡漏填」与「门在挡一切空表」—— 写端的闸门就是 `FieldCount > 0`，
/// phantom / 零字段 wrapper 本来就没有表可带。
#[test]
fn empty_value_struct_without_a_field_table_is_accepted() {
    use crate::metadata::bytecode::{CLASS_FLAG_STRUCT, StructLayoutDesc};
    let mut m = module_with_class_names("m", &["Demo.Phantom"]);
    let c = &mut m.classes[0];
    c.class_flags = CLASS_FLAG_STRUCT;
    c.struct_layout = Some(StructLayoutDesc {
        size: 0, ref_offsets: Box::new([]), ref_kinds: Box::new([]),
    });
    build_type_registry(&mut m);

    let td = m.type_registry.get("Demo.Phantom").expect("类型进了注册表");
    assert_eq!(td.struct_layout().expect("struct_layout 在").field_count(), 0);
}
