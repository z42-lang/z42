//! `config.rs` 的单测（runtime-rust.md：测试独立文件；refactor-split-config 自内联 `mod tests` 搬出）。
use super::*;
use std::collections::{BTreeMap, HashMap};

/// Build a fake env getter from a static map — avoids global env-var
/// race when cargo runs tests in parallel. Returns owned strings so
/// the closure can outlive the input slice.
fn fake_env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs.iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |name: &str| map.get(name).cloned()
}

#[test]
fn known_knobs_alphabetical_and_unique() {
    let names: Vec<&str> = KNOWN_KNOBS.iter().map(|k| k.name).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "KNOWN_KNOBS must be alphabetically sorted by name");

    let mut uniq = sorted.clone();
    uniq.dedup();
    assert_eq!(uniq.len(), sorted.len(), "KNOWN_KNOBS contains duplicate names");
}

#[test]
fn known_knobs_match_struct_fields_for_startup_knobs() {
    // The 4 path-ish fields on RuntimeConfig must each appear in KNOWN_KNOBS.
    let names: Vec<&str> = KNOWN_KNOBS.iter().map(|k| k.name).collect();
    for required in ["Z42_LIBS", "Z42_PATH", "Z42_LOG", "Z42_CRASH_DIR"] {
        assert!(names.contains(&required),
            "RuntimeConfig field expects {required} in KNOWN_KNOBS");
    }
}

#[test]
fn from_getter_all_unset() {
    let cfg = RuntimeConfig::from_getter(fake_env(&[]));
    assert!(cfg.libs_dir.is_none());
    assert!(cfg.log_filter.is_none());
    assert!(cfg.crash_dir.is_none());
    assert!(cfg.module_path.is_empty());
}

#[test]
fn from_getter_empty_string_is_unset() {
    let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_LIBS", "")]));
    assert!(cfg.libs_dir.is_none(), "empty Z42_LIBS should be treated as unset");
}

#[test]
fn from_getter_whitespace_only_is_unset() {
    let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_LOG", "   ")]));
    assert!(cfg.log_filter.is_none(), "whitespace-only Z42_LOG should be unset");
}

#[test]
fn sampling_knobs_default_off() {
    let cfg = RuntimeConfig::from_getter(fake_env(&[]));
    assert!(cfg.sample_hz.is_none(), "sampling off by default");
    assert!(cfg.trace_out.is_none(), "no trace by default");
    assert_eq!(cfg.sample_out, std::path::PathBuf::from("z42-samples.folded"));
}

#[test]
fn sampling_knobs_parse() {
    let cfg = RuntimeConfig::from_getter(fake_env(&[
        ("Z42_SAMPLE_HZ", "2000"),
        ("Z42_SAMPLE_OUT", "/tmp/out.folded"),
        ("Z42_TRACE_OUT", "/tmp/trace.json"),
    ]));
    assert_eq!(cfg.sample_hz, Some(2000));
    assert_eq!(cfg.sample_out, std::path::PathBuf::from("/tmp/out.folded"));
    assert_eq!(cfg.trace_out.as_deref(), Some(std::path::Path::new("/tmp/trace.json")));
}

#[test]
fn sample_hz_zero_or_garbage_disables() {
    for bad in ["0", "-3", "abc"] {
        let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_SAMPLE_HZ", bad)]));
        assert!(cfg.sample_hz.is_none(), "Z42_SAMPLE_HZ={bad:?} → sampling off");
    }
}

#[test]
fn from_getter_libs_set() {
    let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_LIBS", "/tmp/z42-libs")]));
    assert_eq!(cfg.libs_dir.as_deref(), Some(std::path::Path::new("/tmp/z42-libs")));
}

#[test]
fn from_getter_path_splits_on_platform_separator() {
    let sep = if cfg!(windows) { ';' } else { ':' };
    let input = format!("/a{sep}/b{sep}/c");
    let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_PATH", &input)]));
    assert_eq!(
        cfg.module_path,
        vec![PathBuf::from("/a"), PathBuf::from("/b"), PathBuf::from("/c")]
    );
}

#[test]
fn from_getter_path_skips_empty_segments() {
    let sep = if cfg!(windows) { ';' } else { ':' };
    let input = format!("/a{sep}{sep}/b{sep} {sep} /c");
    let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_PATH", &input)]));
    assert_eq!(
        cfg.module_path,
        vec![PathBuf::from("/a"), PathBuf::from("/b"), PathBuf::from("/c")]
    );
}

#[test]
fn from_getter_log_filter_passes_through() {
    let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_LOG", "z42::jit=debug,z42=warn")]));
    assert_eq!(cfg.log_filter.as_deref(), Some("z42::jit=debug,z42=warn"));
}

#[test]
fn from_getter_crash_dir_set() {
    let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_CRASH_DIR", "/var/log/z42")]));
    assert_eq!(cfg.crash_dir.as_deref(), Some(std::path::Path::new("/var/log/z42")));
}

#[test]
fn from_getter_default_values_match_documented_defaults() {
    let cfg = RuntimeConfig::from_getter(fake_env(&[]));
    assert_eq!(cfg.gc_mode,             GcMode::StwMarkSweep);
    assert_eq!(cfg.gc_minor_threshold,  0.75);
    assert_eq!(cfg.gc_pause_window,     1024);
    assert_eq!(cfg.gc_soft_threshold,   0.80);
    assert_eq!(cfg.gc_near_limit_ratio, 0.90);
    assert_eq!(cfg.gc_pressure_ratio,   0.75);
    assert_eq!(cfg.gc_throttle_ratio,   0.10);
    assert_eq!(cfg.safepoint_throttle,  1024);
    assert!(cfg.native_search_paths.is_empty());
}

// ── Phase 2 subsystem knob parsers ───────────────────────────────────────

#[test]
fn from_getter_gc_mode_recognised_aliases() {
    for (input, expected) in [
        ("concurrent",                 GcMode::ConcurrentMarkSweep),
        ("concurrent-mark-sweep",      GcMode::ConcurrentMarkSweep),
        ("generational",               GcMode::GenerationalMarkSweep),
        ("generational-mark-sweep",    GcMode::GenerationalMarkSweep),
        ("stw",                        GcMode::StwMarkSweep),
        ("stw-mark-sweep",             GcMode::StwMarkSweep),
    ] {
        let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_GC_MODE", input)]));
        assert_eq!(cfg.gc_mode, expected, "Z42_GC_MODE={input:?}");
    }
}

#[test]
fn from_getter_gc_mode_unknown_falls_back_to_stw() {
    let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_GC_MODE", "bogus-algo")]));
    assert_eq!(cfg.gc_mode, GcMode::StwMarkSweep);
}

#[test]
fn from_getter_gc_minor_threshold_validates_range() {
    // Valid in (0.0, 1.0]
    assert_eq!(
        RuntimeConfig::from_getter(fake_env(&[("Z42_GC_MINOR_THRESHOLD", "0.5")]))
            .gc_minor_threshold,
        0.5,
    );
    // Out-of-range / invalid → default 0.75
    for bad in &["0", "-0.1", "1.5", "garbage"] {
        assert_eq!(
            RuntimeConfig::from_getter(fake_env(&[("Z42_GC_MINOR_THRESHOLD", bad)]))
                .gc_minor_threshold,
            0.75,
            "bad input {bad:?} should default",
        );
    }
}

#[test]
fn from_getter_gc_pause_window_clamps_and_validates() {
    assert_eq!(
        RuntimeConfig::from_getter(fake_env(&[("Z42_GC_PAUSE_WINDOW", "2048")]))
            .gc_pause_window,
        2048,
    );
    // Clamp to MAX
    assert_eq!(
        RuntimeConfig::from_getter(fake_env(&[("Z42_GC_PAUSE_WINDOW", "999999")]))
            .gc_pause_window,
        GC_PAUSE_WINDOW_MAX,
    );
    // 0 / negative / garbage → default
    for bad in &["0", "-1", "abc"] {
        assert_eq!(
            RuntimeConfig::from_getter(fake_env(&[("Z42_GC_PAUSE_WINDOW", bad)]))
                .gc_pause_window,
            GC_PAUSE_WINDOW_DEFAULT,
        );
    }
}

#[test]
fn from_getter_gc_soft_threshold_clamps_to_unit_range() {
    assert_eq!(
        RuntimeConfig::from_getter(fake_env(&[("Z42_GC_SOFT_THRESHOLD", "0.42")]))
            .gc_soft_threshold,
        0.42,
    );
    // Clamp under / over
    assert_eq!(
        RuntimeConfig::from_getter(fake_env(&[("Z42_GC_SOFT_THRESHOLD", "-1.0")]))
            .gc_soft_threshold,
        0.0,
    );
    assert_eq!(
        RuntimeConfig::from_getter(fake_env(&[("Z42_GC_SOFT_THRESHOLD", "1.5")]))
            .gc_soft_threshold,
        1.0,
    );
    // Garbage → default 0.80
    assert_eq!(
        RuntimeConfig::from_getter(fake_env(&[("Z42_GC_SOFT_THRESHOLD", "xyz")]))
            .gc_soft_threshold,
        0.80,
    );
}

#[test]
fn from_getter_gc_auto_collect_ratios_parse_and_clamp() {
    // Each ratio parses its own env var.
    let cfg = RuntimeConfig::from_getter(fake_env(&[
        ("Z42_GC_NEAR_LIMIT_RATIO", "0.95"),
        ("Z42_GC_PRESSURE_RATIO",   "0.6"),
        ("Z42_GC_THROTTLE_RATIO",   "0.05"),
    ]));
    assert_eq!(cfg.gc_near_limit_ratio, 0.95);
    assert_eq!(cfg.gc_pressure_ratio,   0.6);
    assert_eq!(cfg.gc_throttle_ratio,   0.05);

    // Out-of-unit values clamp to [0, 1] (not rejected).
    let clamped = RuntimeConfig::from_getter(fake_env(&[
        ("Z42_GC_NEAR_LIMIT_RATIO", "1.5"),
        ("Z42_GC_PRESSURE_RATIO",   "-0.2"),
    ]));
    assert_eq!(clamped.gc_near_limit_ratio, 1.0);
    assert_eq!(clamped.gc_pressure_ratio,   0.0);

    // Garbage → per-knob documented default.
    let bad = RuntimeConfig::from_getter(fake_env(&[
        ("Z42_GC_NEAR_LIMIT_RATIO", "nope"),
        ("Z42_GC_PRESSURE_RATIO",   "xyz"),
        ("Z42_GC_THROTTLE_RATIO",   ""),
    ]));
    assert_eq!(bad.gc_near_limit_ratio, 0.90);
    assert_eq!(bad.gc_pressure_ratio,   0.75);
    assert_eq!(bad.gc_throttle_ratio,   0.10);
}

#[test]
fn from_getter_safepoint_throttle_validates_positive_u32() {
    assert_eq!(
        RuntimeConfig::from_getter(fake_env(&[("Z42_SAFEPOINT_THROTTLE", "1")]))
            .safepoint_throttle,
        1,
    );
    assert_eq!(
        RuntimeConfig::from_getter(fake_env(&[("Z42_SAFEPOINT_THROTTLE", "4096")]))
            .safepoint_throttle,
        4096,
    );
    // 0 / negative / garbage → default 1024
    for bad in &["0", "-1", "abc"] {
        assert_eq!(
            RuntimeConfig::from_getter(fake_env(&[("Z42_SAFEPOINT_THROTTLE", bad)]))
                .safepoint_throttle,
            1024,
            "bad input {bad:?}",
        );
    }
}

#[test]
fn from_getter_native_path_splits_on_platform_separator() {
    let sep = if cfg!(windows) { ';' } else { ':' };
    let input = format!("/native/a{sep}/native/b");
    let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_NATIVE_PATH", &input)]));
    assert_eq!(
        cfg.native_search_paths,
        vec![PathBuf::from("/native/a"), PathBuf::from("/native/b")],
    );
}

#[test]
fn from_getter_ignores_unrelated_env_vars() {
    let cfg = RuntimeConfig::from_getter(fake_env(&[
        ("RUST_BACKTRACE", "1"),  // unrelated env
    ]));
    assert!(cfg.libs_dir.is_none());
    assert!(cfg.log_filter.is_none());
    assert_eq!(cfg.gc_mode, GcMode::StwMarkSweep);
}

// ── unify-run-modes P0: layered resolution + [runtime] config file ────────

fn rt_table(src: &str) -> toml::Table {
    toml::from_str(src).expect("test TOML must parse")
}

/// Unique temp path per (pid, name) — no global env mutation, parallel-safe.
fn write_temp(name: &str, content: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir()
        .join(format!("z42-p0cfg-{}-{name}.toml", std::process::id()));
    std::fs::write(&p, content).expect("write temp config");
    p
}

#[test]
fn resolve_none_equals_from_getter_nonbreaking() {
    // The non-breaking guarantee: with no config-file layer, resolve is
    // byte-for-byte the old env-only behaviour.
    let env = &[("Z42_LIBS", "/l"), ("Z42_GC_MODE", "concurrent"), ("Z42_JIT_PROFILE", "1")];
    let a = RuntimeConfig::resolve(fake_env(env), None);
    let b = RuntimeConfig::from_getter(fake_env(env));
    assert_eq!(a.libs_dir, b.libs_dir);
    assert_eq!(a.gc_mode, b.gc_mode);
    assert_eq!(a.jit_profile, b.jit_profile);
    assert_eq!(a.gc_minor_threshold, b.gc_minor_threshold);
}

#[test]
fn resolve_env_wins_over_table() {
    let t = rt_table("gc-mode = \"stw\"\ngc-minor-threshold = 0.5");
    let cfg = RuntimeConfig::resolve(fake_env(&[("Z42_GC_MODE", "concurrent")]), Some(&t));
    assert_eq!(cfg.gc_mode, GcMode::ConcurrentMarkSweep, "env beats table");
    assert_eq!(cfg.gc_minor_threshold, 0.5, "table used where env absent");
}

#[test]
fn resolve_table_wins_over_default() {
    let t = rt_table("gc-mode = \"generational\"\ngc-pause-window = 4096\njit-profile = true");
    let cfg = RuntimeConfig::resolve(fake_env(&[]), Some(&t));
    assert_eq!(cfg.gc_mode, GcMode::GenerationalMarkSweep);
    assert_eq!(cfg.gc_pause_window, 4096);
    assert!(cfg.jit_profile, "table boolean jit-profile=true → on");
}

#[test]
fn resolve_all_unset_uses_defaults() {
    let t = rt_table("");
    let cfg = RuntimeConfig::resolve(fake_env(&[]), Some(&t));
    assert_eq!(cfg.gc_mode, GcMode::StwMarkSweep);
    assert_eq!(cfg.gc_minor_threshold, 0.75);
    assert!(!cfg.jit_profile);
}

#[test]
fn resolve_empty_env_falls_to_table() {
    // Empty env value = unset (config-wide convention) → table consulted.
    let t = rt_table("gc-mode = \"generational\"");
    let cfg = RuntimeConfig::resolve(fake_env(&[("Z42_GC_MODE", "")]), Some(&t));
    assert_eq!(cfg.gc_mode, GcMode::GenerationalMarkSweep);
}

#[test]
fn new_knobs_registered_with_toml_keys() {
    for name in ["Z42_JIT_PROFILE", "Z42_TARGET", "Z42_CONFIG"] {
        assert!(KNOWN_KNOBS.iter().any(|k| k.name == name), "{name} must be registered");
    }
    assert_eq!(knob_by_env_name("Z42_GC_MODE").map(|k| k.toml_key), Some("gc-mode"));
    assert_eq!(knob_by_env_name("Z42_GC_MINOR_THRESHOLD").map(|k| k.toml_key), Some("gc-minor-threshold"));
    // Z42_CONFIG is a meta pointer — no [runtime] value key.
    assert_eq!(knob_by_env_name("Z42_CONFIG").map(|k| k.toml_key), Some(""));
}

#[test]
fn gc_minor_threshold_hint_not_stale() {
    // Regression: the KNOWN_KNOBS entry used to say "64 KiB" / "bytes of
    // allocation", contradicting the actual survival-ratio 0.75 semantics.
    let k = KNOWN_KNOBS.iter().find(|k| k.name == "Z42_GC_MINOR_THRESHOLD").unwrap();
    assert!(!k.default_hint.contains("64 KiB"));
    assert!(!k.description.contains("bytes of allocation"));
    assert!(k.default_hint.contains("0.75"));
}

#[test]
fn jit_profile_from_env() {
    assert!(RuntimeConfig::from_getter(fake_env(&[("Z42_JIT_PROFILE", "1")])).jit_profile);
    assert!(!RuntimeConfig::from_getter(fake_env(&[])).jit_profile);
    assert!(!RuntimeConfig::from_getter(fake_env(&[("Z42_JIT_PROFILE", "")])).jit_profile,
        "empty = unset (config convention)");
}

#[test]
fn load_runtime_toml_unset_is_none() {
    let out = load_runtime_toml(fake_env(&[])).unwrap();
    assert!(out.is_none());
}

#[test]
fn load_runtime_toml_missing_file_is_none_not_panic() {
    let out = load_runtime_toml(fake_env(&[("Z42_CONFIG", "/no/such/z42-cfg-xyz.toml")])).unwrap();
    assert!(out.is_none(), "missing file → None (warn), not error/panic");
}

#[test]
fn load_runtime_toml_reads_runtime_section() {
    let p = write_temp("reads", "[runtime]\ngc-mode = \"concurrent\"\n[other]\nx = 1\n");
    let pstr = p.to_string_lossy().into_owned();
    let table = load_runtime_toml(fake_env(&[("Z42_CONFIG", &pstr)])).unwrap().unwrap();
    assert_eq!(table.get("gc-mode").and_then(|v| v.as_str()), Some("concurrent"));
    // end-to-end through resolve
    let cfg = RuntimeConfig::resolve(fake_env(&[]), Some(&table));
    assert_eq!(cfg.gc_mode, GcMode::ConcurrentMarkSweep);
    let _ = std::fs::remove_file(&p);
}

#[test]
fn load_runtime_toml_no_runtime_section_is_none() {
    let p = write_temp("noruntime", "[other]\nx = 1\n");
    let pstr = p.to_string_lossy().into_owned();
    let out = load_runtime_toml(fake_env(&[("Z42_CONFIG", &pstr)])).unwrap();
    assert!(out.is_none(), "no [runtime] table → None");
    let _ = std::fs::remove_file(&p);
}

#[test]
fn load_runtime_toml_malformed_errors_explicitly() {
    let p = write_temp("malformed", "this is = = not toml [[[");
    let pstr = p.to_string_lossy().into_owned();
    let out = load_runtime_toml(fake_env(&[("Z42_CONFIG", &pstr)]));
    assert!(out.is_err(), "malformed TOML → explicit error, not silent default");
    let _ = std::fs::remove_file(&p);
}

// ── unify-run-modes P2: Z42_MODE / [runtime].mode knob ────────────────────

#[test]
fn mode_registered_with_toml_key() {
    assert!(KNOWN_KNOBS.iter().any(|k| k.name == "Z42_MODE"));
    assert_eq!(knob_by_env_name("Z42_MODE").map(|k| k.toml_key), Some("mode"));
}

#[test]
fn mode_unset_is_none() {
    assert!(RuntimeConfig::from_getter(fake_env(&[])).mode.is_none());
    assert!(RuntimeConfig::from_getter(fake_env(&[("Z42_MODE", "")])).mode.is_none(),
        "empty = unset");
}

#[test]
fn mode_from_env_raw_string() {
    // config.rs stores the raw value (main.rs validates + feature-gates).
    assert_eq!(RuntimeConfig::from_getter(fake_env(&[("Z42_MODE", "jit")])).mode.as_deref(), Some("jit"));
    assert_eq!(RuntimeConfig::from_getter(fake_env(&[("Z42_MODE", "interp")])).mode.as_deref(), Some("interp"));
}

#[test]
fn mode_env_wins_over_runtime_table() {
    let t = rt_table("mode = \"interp\"");
    let cfg = RuntimeConfig::resolve(fake_env(&[("Z42_MODE", "jit")]), Some(&t));
    assert_eq!(cfg.mode.as_deref(), Some("jit"), "env beats [runtime].mode");
}

#[test]
fn mode_from_runtime_table_when_env_unset() {
    let t = rt_table("mode = \"aot\"");
    let cfg = RuntimeConfig::resolve(fake_env(&[]), Some(&t));
    assert_eq!(cfg.mode.as_deref(), Some("aot"), "[runtime].mode used when env unset");
}

// ── complete-runtime-settings P0: schema 不变式 + 可用性求值 ────────────────
//
// 求值测试一律**注入假 BuildCtx**，不读真实构建配置——否则同一断言在
// `--no-default-features` 的 wasm/ios preset 下结果会漂移（design.md Testing Strategy）。

/// 造一个可控的构建环境。
fn ctx(debug: bool, features: &[&'static str], os: &'static str) -> BuildCtx {
    BuildCtx { debug, features: features.to_vec(), os }
}

fn spec_named(name: &str) -> &'static KnobSpec {
    KNOWN_KNOBS.iter().find(|k| k.name == name).unwrap_or_else(|| panic!("{name} not registered"))
}

#[test]
fn requires_only_reference_known_features() {
    for k in KNOWN_KNOBS {
        for f in k.requires {
            assert!(
                KNOWN_FEATURES.contains(f),
                "{}: requires unknown feature `{f}` — add it to availability::KNOWN_FEATURES \
                 (and to feature_enabled's match) or fix the typo",
                k.name
            );
        }
    }
}

#[test]
fn feature_table_covers_cargo_features() {
    // Mirror of `[features]` in src/runtime/Cargo.toml, minus the `default` meta-feature.
    // Cargo.toml gains a feature → this test goes red until KNOWN_FEATURES + the
    // feature_enabled match are updated in lockstep.
    let expected = [
        "android", "aot", "bundled-compression", "dhat-heap", "interp-only", "ios",
        "jit", "mimalloc-alloc", "native-interop", "profile-contention", "wasm",
    ];
    assert_eq!(
        KNOWN_FEATURES, &expected[..],
        "KNOWN_FEATURES drifted from Cargo.toml [features]; keep both (and feature_enabled) in sync"
    );
    // Sorted so the table stays scannable.
    let mut sorted = expected;
    sorted.sort();
    assert_eq!(KNOWN_FEATURES, &sorted[..], "KNOWN_FEATURES must stay alphabetically sorted");
}

#[test]
fn feature_enabled_rejects_unknown_names() {
    assert!(!feature_enabled("not-a-real-feature"));
    assert!(!feature_enabled(""));
}

#[test]
fn meta_knobs_are_internal_and_never_file_settable() {
    for k in KNOWN_KNOBS.iter().filter(|k| k.is_meta()) {
        assert_eq!(k.tier, Tier::Internal, "{}: meta knob must be Internal tier", k.name);
        assert_eq!(
            k.sources,
            LayerMask::CLI_ENV,
            "{}: a knob that names a config file (or controls diagnostic severity) must not be \
             settable from a config file — that is self-referential",
            k.name
        );
    }
    // The three meta knobs are all registered.
    for name in ["Z42_CONFIG", "Z42_APP_CONFIG", "Z42_STRICT_CONFIG"] {
        assert!(spec_named(name).is_meta(), "{name} must be a meta knob");
    }
}

#[test]
fn non_meta_knobs_have_a_toml_key() {
    for k in KNOWN_KNOBS.iter().filter(|k| k.tier != Tier::Internal) {
        assert!(!k.toml_key.is_empty(), "{}: non-internal knob needs a toml_key", k.name);
    }
}

#[test]
fn aliases_are_unique_and_dont_shadow_keys() {
    let mut seen: Vec<&str> = Vec::new();
    for k in KNOWN_KNOBS {
        for a in k.aliases {
            assert!(
                !KNOWN_KNOBS.iter().any(|o| o.toml_key == *a),
                "{}: alias `{a}` collides with another knob's toml_key",
                k.name
            );
            assert!(!seen.contains(a), "{}: alias `{a}` is already used by another knob", k.name);
            seen.push(a);
        }
    }
}

#[test]
fn knob_lookup_by_key_and_env_name() {
    assert_eq!(knob_by_key("gc-mode").unwrap().name, "Z42_GC_MODE");
    assert_eq!(knob_by_env_name("Z42_GC_MODE").unwrap().toml_key, "gc-mode");
    // env-name form is NOT accepted as a --set key (design.md Decision 1 / User U2)
    assert!(knob_by_key("Z42_GC_MODE").is_none(), "env-var form must not be a CLI key");
    // meta knobs have no key form at all
    assert!(knob_by_key("").is_none(), "empty key must never match a meta knob");
    assert!(knob_by_key("nope").is_none());
}

#[test]
fn enum_value_kinds_list_every_parser_arm() {
    // GC_MODES must cover exactly what parse_gc_mode accepts, else --set/-file
    // validation would reject a value the parser handles (or vice versa).
    let ValueKind::Enum(modes) = spec_named("Z42_GC_MODE").value else {
        panic!("Z42_GC_MODE must be an Enum knob")
    };
    for m in ["stw", "stw-mark-sweep", "concurrent", "concurrent-mark-sweep",
              "generational", "generational-mark-sweep"] {
        assert!(modes.contains(&m), "GC_MODES missing parser arm `{m}`");
    }
    assert_eq!(modes.len(), 6, "GC_MODES has an arm parse_gc_mode does not handle");

    let ValueKind::Enum(exec) = spec_named("Z42_MODE").value else {
        panic!("Z42_MODE must be an Enum knob")
    };
    assert_eq!(exec, &["interp", "jit", "aot"]);
}

#[test]
fn z42_mode_has_no_knob_level_feature_gate() {
    // Deliberate exception (design.md Decision 2): jit/aot gating is PER-VALUE and
    // lives in main.rs::resolve_config_mode; interp works in every build. Marking the
    // knob `requires:["jit"]` would wrongly reject `Z42_MODE=interp` on interp-only builds.
    assert!(spec_named("Z42_MODE").requires.is_empty(),
        "Z42_MODE must not carry a knob-level feature gate");
}

// ── 可用性四轴求值 ───────────────────────────────────────────────────────────

#[test]
fn availability_accepts_a_plain_knob_from_every_layer() {
    let c = ctx(false, &[], "linux");
    let k = spec_named("Z42_GC_MODE");
    for layer in [Layer::Cli, Layer::Env, Layer::UserConfig, Layer::AppConfig] {
        assert_eq!(evaluate(k, layer, &c), Ok(()), "gc-mode should be settable from {layer:?}");
    }
    assert!(is_available(k, &c));
}

#[test]
fn availability_rejects_layer_outside_sources_mask() {
    let c = ctx(true, &[], "linux");
    let k = spec_named("Z42_STRESS_ITERS"); // env-only
    assert_eq!(evaluate(k, Layer::Env, &c), Ok(()));
    assert_eq!(
        evaluate(k, Layer::Cli, &c),
        Err(Rejection::NotAcceptedFrom { layer: Layer::Cli, accepted: LayerMask::ENV_ONLY })
    );
    assert!(matches!(evaluate(k, Layer::UserConfig, &c), Err(Rejection::NotAcceptedFrom { .. })));
}

#[test]
fn availability_rejects_debug_only_knob_in_release() {
    let k = spec_named("Z42_STRESS_ITERS");
    assert_eq!(evaluate(k, Layer::Env, &ctx(true, &[], "linux")), Ok(()));
    assert_eq!(evaluate(k, Layer::Env, &ctx(false, &[], "linux")), Err(Rejection::DebugOnly));
    assert!(!is_available(k, &ctx(false, &[], "linux")));
}

#[test]
fn availability_rejects_missing_feature() {
    let k = spec_named("Z42_JIT_PROFILE");
    assert_eq!(evaluate(k, Layer::Env, &ctx(false, &["jit"], "linux")), Ok(()));
    assert_eq!(
        evaluate(k, Layer::Env, &ctx(false, &["native-interop"], "linux")),
        Err(Rejection::MissingFeatures(vec!["jit"]))
    );
}

#[test]
fn availability_rejects_excluded_platform() {
    let k = spec_named("Z42_SAMPLE_HZ");
    assert_eq!(evaluate(k, Layer::Env, &ctx(false, &[], "macos")), Ok(()));
    assert_eq!(evaluate(k, Layer::Env, &ctx(false, &[], "wasm")), Err(Rejection::WrongPlatform));
}

#[test]
fn availability_checks_layer_before_build_and_feature() {
    // A knob rejected on several axes reports the *layer* one first — it is the most
    // actionable ("you cannot set this here") and does not depend on the build.
    let c = ctx(false, &[], "linux");
    let k = spec_named("Z42_STRESS_ITERS"); // env-only AND debug-only
    assert!(matches!(evaluate(k, Layer::Cli, &c), Err(Rejection::NotAcceptedFrom { .. })));
}

#[test]
fn platform_only_form_is_an_allowlist() {
    // No shipped knob uses Only(..) yet; assert the evaluator handles it so the
    // variant is not silently broken when the first user shows up.
    let k = KnobSpec { platforms: PlatformAvail::Only(&["windows"]), ..*spec_named("Z42_LOG") };
    assert_eq!(evaluate(&k, Layer::Env, &ctx(false, &[], "windows")), Ok(()));
    assert_eq!(evaluate(&k, Layer::Env, &ctx(false, &[], "linux")), Err(Rejection::WrongPlatform));
}

#[test]
fn layer_mask_labels_follow_priority_order() {
    assert_eq!(LayerMask::ALL.labels(), vec!["cli", "env", "user-config", "app-config"]);
    assert_eq!(LayerMask::CLI_ENV.labels(), vec!["cli", "env"]);
    assert_eq!(LayerMask::ENV_ONLY.labels(), vec!["env"]);
    assert!(LayerMask::ALL.contains(LayerMask::CLI));
    assert!(!LayerMask::ENV_ONLY.contains(LayerMask::CLI));
    // Default is not an input layer — it never satisfies a sources mask.
    assert!(!LayerMask::ALL.contains(Layer::Default.mask()));
}

// ── 诊断渲染 ─────────────────────────────────────────────────────────────────

#[test]
fn rejection_message_names_the_missing_feature_and_this_builds_features() {
    let c = ctx(false, &["interp-only", "native-interop"], "linux");
    let k = spec_named("Z42_JIT_PROFILE");
    let msg = evaluate(k, Layer::Env, &c).unwrap_err().render(k, Layer::Env, &c);
    assert!(msg.contains("jit-profile"), "message must name the knob key: {msg}");
    assert!(msg.contains("Z42_JIT_PROFILE"), "message must name the env var: {msg}");
    assert!(msg.contains("[env]"), "message must name the source layer: {msg}");
    assert!(msg.contains("requires feature `jit`"), "message must name the missing feature: {msg}");
    assert!(msg.contains("interp-only, native-interop"),
        "message must list what this build DOES have: {msg}");
    assert!(msg.contains("value ignored"), "message must say what happens to the value: {msg}");
    assert!(msg.contains("--list-knobs --all"), "message must point at the discovery command: {msg}");
}

#[test]
fn rejection_message_for_wrong_layer_lists_accepted_layers() {
    let c = ctx(true, &[], "linux");
    let k = spec_named("Z42_STRESS_ITERS");
    let msg = evaluate(k, Layer::Cli, &c).unwrap_err().render(k, Layer::Cli, &c);
    assert!(msg.contains("cannot be set from [cli]"), "{msg}");
    assert!(msg.contains("accepted layers: env"), "{msg}");
}

#[test]
fn rejection_message_for_platform_lists_supported_set() {
    let c = ctx(false, &[], "wasm");
    let k = spec_named("Z42_SAMPLE_HZ");
    let msg = evaluate(k, Layer::Env, &c).unwrap_err().render(k, Layer::Env, &c);
    assert!(msg.contains("unavailable on this platform (wasm)"), "{msg}");
    assert!(msg.contains("all except wasm"), "{msg}");
}

#[test]
fn build_ctx_current_reports_a_normalized_os() {
    let c = BuildCtx::current();
    assert!(!c.os.is_empty());
    // Whatever the target, every reported feature must be a known one.
    for f in &c.features {
        assert!(KNOWN_FEATURES.contains(f), "BuildCtx::current reported unknown feature {f}");
    }
    assert_eq!(c.debug, cfg!(debug_assertions));
}

// ── complete-runtime-settings P1: 注册表防腐（源码扫描）─────────────────────

/// 只在测试里存在的 `Z42_*` 名字——不是运行时旋钮，不该进 `KNOWN_KNOBS`。
const TEST_ONLY_ENV_NAMES: &[&str] = &[
    "Z42_CLEAR_TEST",
    "Z42_DEFINITELY_NOT_SET_XYZZY_KEY",
    "Z42_PARENT_VAR",
    "Z42_STRESS_SEED", // 压力测试种子，与 Z42_STRESS_ITERS 成对；仅测试代码消费
    "Z42_TEST_VAR",
    "Z42_X",
];

/// `off` 所在的那一行（用来判断这个字面量是不是一次真正的 env 读取，而不是
/// 一个诊断消息里的名字、或一个当参数传的 key）。
fn line_around(text: &str, off: usize) -> &str {
    let start = text[..off].rfind('\n').map_or(0, |i| i + 1);
    let end = text[off..].find('\n').map_or(text.len(), |i| off + i);
    &text[start..end]
}

/// 递归收集 `dir` 下所有非测试 `.rs` 文件里出现的 `"Z42_*"` 字符串字面量，
/// 并标注它是否是一次真正的 `env::var` / `var_os` **读取**。
fn scan_env_literals(dir: &std::path::Path, out: &mut Vec<(String, String, bool)>) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    scan_env_literals_in(dir, &root, out)
}

/// `rel` 相对 `root` 而非相对当前递归层——否则 `config/parse.rs` 会被记成 `parse.rs`，
/// 按目录过滤就失效了（我第一版正是这么错的，被下面那道门当场抓到）。
fn scan_env_literals_in(dir: &std::path::Path, root: &std::path::Path,
                        out: &mut Vec<(String, String, bool)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            // 测试目录整体跳过。
            if path.file_name().is_some_and(|n| n == "tests") {
                continue;
            }
            scan_env_literals_in(&path, root, out);
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !name.ends_with(".rs") || name.ends_with("_tests.rs") || name == "tests.rs" {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let rel = path.strip_prefix(root).unwrap_or(&path).display().to_string();
        let bytes = text.as_bytes();
        let mut i = 0;
        while let Some(off) = text[i..].find("\"Z42_") {
            let start = i + off + 1;
            let mut end = start;
            while end < bytes.len()
                && (bytes[end].is_ascii_uppercase() || bytes[end].is_ascii_digit() || bytes[end] == b'_')
            {
                end += 1;
            }
            // `"Z42_"` on its own is a prefix test, not a knob name.
            if end < bytes.len() && bytes[end] == b'"' && end > start + 4 && bytes[end - 1] != b'_' {
                let line = line_around(&text, start);
                let is_read = (line.contains("env::var(") || line.contains("env::var_os("))
                    && !line.contains("set_var") && !line.contains("remove_var");
                out.push((text[start..end].to_string(), rel.clone(), is_read));
            }
            i = start;
        }
    }
}

#[test]
fn every_z42_env_literal_in_the_vm_is_registered() {
    // The registry claims to be the authoritative list of every Z42_* the runtime
    // reads. It had drifted: 8 knobs (jit/osr thresholds, the three fusion switches,
    // stackalloc, jit-debug-promote, repl-native) were read inline via std::env::var
    // and absent from the table, so `--info` / `--list-knobs` could not show them.
    // This gate keeps that from happening again.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    scan_env_literals(&root, &mut found);
    assert!(found.len() > 10, "source scan found suspiciously few Z42_* literals — is the walk broken?");

    let mut unregistered: Vec<String> = found
        .into_iter()
        .filter(|(name, _, _)| {
            !TEST_ONLY_ENV_NAMES.contains(&name.as_str()) && knob_by_env_name(name).is_none()
        })
        .map(|(name, file, _)| format!("{name} (in {file})"))
        .collect();
    unregistered.sort();
    unregistered.dedup();
    assert!(
        unregistered.is_empty(),
        "these Z42_* env vars are read by the runtime but missing from KNOWN_KNOBS —\n  \
         register them in config/knob_table.rs (use the INLINE_ENV base if the read stays\n  \
         inline), or add the name to TEST_ONLY_ENV_NAMES if it is test scaffolding:\n  - {}",
        unregistered.join("\n  - ")
    );
}

#[test]
fn no_knob_is_read_inline_from_the_environment_any_more() {
    // The registry may only promise a layer the consumer can actually observe.
    // A knob read via a bare `std::env::var` at its consumption site never sees the
    // CLI or config-file layers, so declaring `LayerMask::ALL` for it would make
    // `--set` a silent no-op — worse than not registering it at all.
    //
    // adopt-inline-env-knobs routed the last eight such knobs through
    // `runtime_config()`. This gate is now the *reverse* of what it used to be:
    // rather than checking that inline readers stay ENV_ONLY, it refuses to let a
    // new inline reader appear. Adding one means either routing it through
    // RuntimeConfig, or declaring ENV_ONLY and saying why here.
    let inline: Vec<&str> = KNOWN_KNOBS.iter()
        .filter(|k| k.consumed_by.contains("inline env read"))
        .map(|k| k.name)
        .collect();
    assert!(inline.is_empty(),
        "these knobs claim an inline env read — route them through runtime_config() \
         (see adopt-inline-env-knobs) or mark them ENV_ONLY deliberately: {inline:?}");

    // And every consumer that a scan can reach must go through the config module.
    // `Z42_STRESS_ITERS` is the one deliberate exception (test scaffolding).
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    scan_env_literals(&root, &mut found);
    let stragglers: Vec<String> = found.into_iter()
        .filter(|(name, file, is_read)| {
            *is_read
                && !TEST_ONLY_ENV_NAMES.contains(&name.as_str())
                && !file.starts_with("config")   // the config module IS the reader
                && knob_by_env_name(name).is_some_and(|k| {
                    // Meta knobs (Z42_CONFIG / Z42_APP_CONFIG / Z42_STRICT_CONFIG) name
                    // the config files and the diagnostic severity, so `main()` reads
                    // them from the environment while ASSEMBLING the layers — there is
                    // no resolved value to consult yet. Scaffolding stays env-only too.
                    !k.is_meta() && k.sources != LayerMask::ENV_ONLY
                })
        })
        .map(|(name, file, _)| format!("{name} (read in {file})"))
        .collect();
    assert!(stragglers.is_empty(),
        "a knob that advertises the CLI / config-file layers is still being read straight \
         from the environment — the layers would be silently ignored there:\n  - {}",
        stragglers.join("\n  - "));
}

// ── complete-runtime-settings P1: provenance + 严重度分层 ───────────────────

fn resolve_all(
    cli: &[(&'static str, &str)],
    env: &[(&str, &str)],
    user: Option<&toml::Table>,
    app: Option<&toml::Table>,
    ctx: &BuildCtx,
) -> (RuntimeConfig, Resolution) {
    let inputs = Inputs {
        cli: cli.iter().map(|(k, v)| (*k, (*v).to_string())).collect(),
        user_config: user,
        app_config: app,
    };
    let get = fake_env(env);
    RuntimeConfig::resolve_with(&get, &inputs, ctx)
}

fn full_ctx() -> BuildCtx {
    ctx(true, &["jit", "native-interop"], "linux")
}

#[test]
fn provenance_records_the_winning_layer() {
    let user = rt_table("gc-mode = \"generational\"");
    let (cfg, res) = resolve_all(
        &[("Z42_GC_MODE", "concurrent")],
        &[("Z42_GC_MODE", "stw")],
        Some(&user),
        None,
        &full_ctx(),
    );
    assert_eq!(cfg.gc_mode, GcMode::ConcurrentMarkSweep, "cli wins");
    let k = res.get("Z42_GC_MODE").unwrap();
    assert_eq!(k.source, Layer::Cli);
    assert_eq!(k.raw.as_deref(), Some("concurrent"));
    // Both lower layers are recorded as overridden, in priority order.
    assert_eq!(
        k.ignored,
        vec![
            IgnoredValue { layer: Layer::Env, value: "stw".into(), reason: IgnoreReason::Overridden },
            IgnoredValue { layer: Layer::UserConfig, value: "generational".into(), reason: IgnoreReason::Overridden },
        ]
    );
    assert!(res.diagnostics.is_empty(), "being overridden is normal, not a problem");
}

#[test]
fn full_five_layer_priority_chain() {
    let user = rt_table("gc-mode = \"generational\"");
    let app = rt_table("gc-mode = \"stw\"");
    let c = full_ctx();
    let cases: [(&[(&'static str, &str)], &[(&str, &str)], bool, bool, GcMode, Layer); 5] = [
        (&[("Z42_GC_MODE", "concurrent")], &[("Z42_GC_MODE", "stw")], true, true, GcMode::ConcurrentMarkSweep, Layer::Cli),
        (&[],                              &[("Z42_GC_MODE", "concurrent")], true, true, GcMode::ConcurrentMarkSweep, Layer::Env),
        (&[],                              &[],  true,  true, GcMode::GenerationalMarkSweep, Layer::UserConfig),
        (&[],                              &[],  false, true, GcMode::StwMarkSweep,          Layer::AppConfig),
        (&[],                              &[],  false, false, GcMode::StwMarkSweep,         Layer::Default),
    ];
    for (cli, env, with_user, with_app, want_mode, want_layer) in cases {
        let (cfg, res) = resolve_all(
            cli, env,
            with_user.then_some(&user),
            with_app.then_some(&app),
            &c,
        );
        assert_eq!(cfg.gc_mode, want_mode, "layer {want_layer:?}");
        assert_eq!(res.get("Z42_GC_MODE").unwrap().source, want_layer);
    }
}

#[test]
fn user_config_and_app_config_merge_per_key() {
    // The bug this guards: the launcher used to hand the app sidecar through
    // Z42_CONFIG, so setting Z42_CONFIG yourself dropped the sidecar wholesale.
    // Two independent layers must merge key by key instead.
    let user = rt_table("gc-mode = \"concurrent\"");
    let app = rt_table("gc-mode = \"stw\"\nsafepoint-throttle = 64\nlog = \"z42=trace\"");
    let (cfg, res) = resolve_all(&[], &[], Some(&user), Some(&app), &full_ctx());

    assert_eq!(cfg.gc_mode, GcMode::ConcurrentMarkSweep, "same key -> user wins");
    assert_eq!(cfg.safepoint_throttle, 64, "app-only key still applies");
    assert_eq!(cfg.log_filter.as_deref(), Some("z42=trace"), "app-only key still applies");
    assert_eq!(res.get("Z42_SAFEPOINT_THROTTLE").unwrap().source, Layer::AppConfig);
    assert_eq!(res.get("Z42_GC_MODE").unwrap().source, Layer::UserConfig);
}

#[test]
fn cli_empty_value_clears_and_falls_through() {
    let (cfg, res) = resolve_all(
        &[("Z42_GC_MODE", "")],
        &[("Z42_GC_MODE", "concurrent")],
        None, None, &full_ctx(),
    );
    assert_eq!(cfg.gc_mode, GcMode::ConcurrentMarkSweep);
    assert_eq!(res.get("Z42_GC_MODE").unwrap().source, Layer::Env);
}

#[test]
fn unavailable_value_is_recorded_and_diagnosed_then_lower_layer_wins() {
    // No `jit` feature: the env value is rejected, but a config-file value for a
    // *different* knob keeps working and the process is expected to continue.
    let c = ctx(true, &["native-interop"], "linux");
    let (cfg, res) = resolve_all(&[], &[("Z42_JIT_PROFILE", "1")], None, None, &c);
    assert!(!cfg.jit_profile, "rejected value must not take effect");
    let k = res.get("Z42_JIT_PROFILE").unwrap();
    assert_eq!(k.source, Layer::Default);
    assert!(matches!(k.ignored[0].reason, IgnoreReason::Unavailable(Rejection::MissingFeatures(_))));
    assert_eq!(res.diagnostics.len(), 1);
    assert_eq!(res.diagnostics[0].layer, Layer::Env);
}

#[test]
fn one_diagnostic_per_knob_even_when_several_layers_set_it() {
    // Same knob unavailable in this build, set from two layers: repeating
    // "requires feature `jit`" adds nothing. Only the highest layer reports;
    // every rejected layer is still recorded for --show-config.
    let c = ctx(true, &["native-interop"], "linux");
    let app = rt_table("jit-profile = true");
    let (_, res) = resolve_all(&[], &[("Z42_JIT_PROFILE", "1")], None, Some(&app), &c);
    assert_eq!(res.diagnostics.len(), 1, "one message per knob, from the highest layer");
    assert_eq!(res.diagnostics[0].layer, Layer::Env);
    let k = res.get("Z42_JIT_PROFILE").unwrap();
    assert_eq!(k.ignored.len(), 2, "both layers recorded for --show-config");
    assert!(k.ignored.iter().all(|i| matches!(i.reason, IgnoreReason::Unavailable(_))));
}

#[test]
fn a_value_overridden_by_a_higher_layer_is_never_diagnosed() {
    // Being overridden is the chain working, not a problem.
    let c = full_ctx();
    let app = rt_table("gc-mode = \"stw\"");
    let (_, res) = resolve_all(&[], &[("Z42_GC_MODE", "concurrent")], None, Some(&app), &c);
    assert!(res.diagnostics.is_empty());
    assert_eq!(res.get("Z42_GC_MODE").unwrap().ignored[0].reason, IgnoreReason::Overridden);
}

#[test]
fn invalid_typed_value_is_rejected_with_a_diagnostic() {
    let (cfg, res) = resolve_all(&[], &[("Z42_GC_MODE", "quantum")], None, None, &full_ctx());
    assert_eq!(cfg.gc_mode, GcMode::StwMarkSweep, "falls back to the default");
    let k = res.get("Z42_GC_MODE").unwrap();
    assert!(matches!(k.ignored[0].reason, IgnoreReason::Invalid(_)));
    assert!(res.diagnostics[0].message.contains("expected one of"), "{:?}", res.diagnostics[0]);
}

#[test]
fn out_of_range_numbers_are_not_rejected_here() {
    // Range policy belongs to the parsers, which clamp (Z42_GC_SOFT_THRESHOLD=1.5
    // -> 1.0, asserted elsewhere as "not rejected"). Rejecting here would turn a
    // clamp into a fallback — a behaviour regression.
    let (cfg, res) = resolve_all(&[], &[("Z42_GC_SOFT_THRESHOLD", "1.5")], None, None, &full_ctx());
    assert_eq!(cfg.gc_soft_threshold, 1.0);
    assert!(res.diagnostics.is_empty());
    assert_eq!(res.get("Z42_GC_SOFT_THRESHOLD").unwrap().source, Layer::Env);
}

#[test]
fn severity_is_fatal_for_cli_and_a_warning_for_everything_else() {
    let c = ctx(true, &["native-interop"], "linux"); // no jit
    let (_, from_env) = resolve_all(&[], &[("Z42_JIT_PROFILE", "1")], None, None, &c);
    assert!(from_env.into_result(false).is_ok(), "env problems must not stop the process");
    assert!(from_env.into_result(true).is_err(), "--strict-config escalates them");

    let (_, from_cli) = resolve_all(&[("Z42_JIT_PROFILE", "1")], &[], None, None, &c);
    let err = from_cli.into_result(false).unwrap_err();
    assert!(err.contains("jit-profile"), "{err}");
    assert!(err.contains("[cli]"), "CLI problems are fatal even without strict mode: {err}");
}

#[test]
fn strict_mode_error_explains_how_to_turn_it_off() {
    let c = ctx(true, &[], "linux");
    let (_, res) = resolve_all(&[], &[("Z42_JIT_PROFILE", "1")], None, None, &c);
    let err = res.into_result(true).unwrap_err();
    assert!(err.contains("--strict-config"), "{err}");
}

#[test]
fn jit_profile_now_honours_a_real_boolean() {
    // Was a doc/impl divergence: the field doc promised `false` = off, but the
    // implementation was `.is_some()` on any non-empty string, so
    // Z42_JIT_PROFILE=false turned profiling ON.
    let c = full_ctx();
    for (val, want) in [("1", true), ("true", true), ("on", true), ("yes", true),
                        ("0", false), ("false", false), ("off", false), ("no", false)] {
        let (cfg, res) = resolve_all(&[], &[("Z42_JIT_PROFILE", val)], None, None, &c);
        assert_eq!(cfg.jit_profile, want, "Z42_JIT_PROFILE={val}");
        assert!(res.diagnostics.is_empty(), "{val} is a valid boolean");
    }
    // A non-boolean is now an explicit diagnostic rather than a silent "on".
    let (cfg, res) = resolve_all(&[], &[("Z42_JIT_PROFILE", "sure")], None, None, &c);
    assert!(!cfg.jit_profile);
    assert!(res.diagnostics[0].message.contains("expected a boolean"), "{:?}", res.diagnostics[0]);
}

#[test]
fn the_four_switch_knobs_are_real_booleans_now() {
    // They used to be ValueKind::Flag ("presence enables" — `Z42_NO_FUSION=0` STILL
    // disabled fusion). That shell convention does not survive a config file:
    //
    //     [runtime]
    //     no-fusion = false     # under Flag semantics this DISABLES fusion
    //
    // adopt-inline-env-knobs opened these knobs to the config-file layers, so the
    // conversion had to happen in the same breath — see design.md Decision 2.
    for name in ["Z42_NO_FUSION", "Z42_NO_TYPED_FUSION", "Z42_FUSION_DEBUG",
                 "Z42_JIT_DEBUG_PROMOTE"] {
        assert_eq!(spec_named(name).value, ValueKind::Bool, "{name}");
    }
    let spec = spec_named("Z42_NO_FUSION");
    for v in ["1", "true", "0", "false", "on", "off"] {
        assert_eq!(validate(spec.value, v), Ok(()), "{v:?} is a boolean");
    }
    assert!(validate(spec.value, "whatever").is_err(), "a non-boolean is now a type error");
}

#[test]
fn value_kind_flag_has_no_users_left() {
    // Flag still exists for a knob that genuinely means "presence enables", but
    // nothing uses it today. If a future knob adopts it, that knob owes an
    // explanation for why the config-file trap in Decision 2 does not apply to it.
    let flags: Vec<&str> = KNOWN_KNOBS.iter()
        .filter(|k| k.value == ValueKind::Flag).map(|k| k.name).collect();
    assert!(flags.is_empty(), "ValueKind::Flag knobs need a justification: {flags:?}");
}

#[test]
fn unknown_table_keys_are_reported() {
    let t = rt_table("gc-mode = \"stw\"\ngc-mod = \"stw\"\nnonsense = 1");
    let mut keys = unknown_table_keys(&t);
    keys.sort();
    assert_eq!(keys, vec!["gc-mod".to_string(), "nonsense".to_string()]);
    let d = unknown_key_diagnostic(Layer::UserConfig, "gc-mod");
    assert_eq!(d.layer, Layer::UserConfig);
    assert!(d.message.contains("unknown runtime knob `gc-mod`"), "{}", d.message);
}

#[test]
fn parse_bool_is_closed_not_truthy() {
    for v in ["1", "TRUE", "Yes", " on "] { assert_eq!(parse_bool(v), Some(true), "{v}"); }
    for v in ["0", "False", "no", "OFF"] { assert_eq!(parse_bool(v), Some(false), "{v}"); }
    for v in ["", "2", "sure", "null"] { assert_eq!(parse_bool(v), None, "{v}"); }
}

// ── complete-runtime-settings P2: CLI --set ─────────────────────────────────

fn set_args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

#[test]
fn set_parses_key_value_pairs() {
    let m = parse_set_args(&set_args(&["gc-mode=concurrent", "safepoint-throttle=64"])).unwrap();
    assert_eq!(m.get("Z42_GC_MODE").map(String::as_str), Some("concurrent"));
    assert_eq!(m.get("Z42_SAFEPOINT_THROTTLE").map(String::as_str), Some("64"));
}

#[test]
fn set_splits_on_the_first_equals_only() {
    // Path lists and log directives legitimately contain '='.
    let m = parse_set_args(&set_args(&["path=/a=b:/c", "log=z42::jit=debug,z42=warn"])).unwrap();
    assert_eq!(m.get("Z42_PATH").map(String::as_str), Some("/a=b:/c"));
    assert_eq!(m.get("Z42_LOG").map(String::as_str), Some("z42::jit=debug,z42=warn"));
}

#[test]
fn set_empty_value_is_kept_as_an_explicit_clear() {
    let m = parse_set_args(&set_args(&["gc-mode="])).unwrap();
    assert_eq!(m.get("Z42_GC_MODE").map(String::as_str), Some(""));
}

#[test]
fn set_rejects_a_missing_equals_sign() {
    let err = parse_set_args(&set_args(&["gc-mode"])).unwrap_err();
    assert!(err.contains("expects KEY=VALUE"), "{err}");
}

#[test]
fn set_rejects_unknown_keys_with_a_suggestion() {
    let err = parse_set_args(&set_args(&["gc-mod=stw"])).unwrap_err();
    assert!(err.contains("unknown runtime knob `gc-mod`"), "{err}");
    assert!(err.contains("did you mean `gc-mode`?"), "{err}");
    assert!(err.contains("--list-knobs"), "{err}");
}

#[test]
fn set_rejects_the_env_var_spelling() {
    // Decision: only the declared key (or an explicit alias) — never an implicit
    // Z42_* equivalence (User U2).
    let err = parse_set_args(&set_args(&["Z42_GC_MODE=stw"])).unwrap_err();
    assert!(err.contains("unknown runtime knob"), "{err}");
}

#[test]
fn set_last_occurrence_wins() {
    let m = parse_set_args(&set_args(&["gc-mode=stw", "gc-mode=concurrent"])).unwrap();
    assert_eq!(m.get("Z42_GC_MODE").map(String::as_str), Some("concurrent"));
}

#[test]
fn suggestions_do_not_fire_for_unrelated_input() {
    assert_eq!(suggest_key("gc-mod"), Some("gc-mode"));
    assert_eq!(suggest_key("gcmode"), Some("gc-mode"));
    assert_eq!(suggest_key("completely-different-thing"), None);
    assert_eq!(suggest_key("x"), None, "a 1-char key must not match everything");
}

#[test]
fn dedicated_flag_and_set_for_the_same_knob_is_an_error() {
    let m = parse_set_args(&set_args(&["mode=jit"])).unwrap();
    let err = reject_flag_conflict(&m, "Z42_MODE", "--mode", Some("interp")).unwrap_err();
    assert!(err.contains("--mode interp"), "{err}");
    assert!(err.contains("--set mode=jit"), "{err}");
    assert!(err.contains("same precedence layer"), "{err}");
    // Either alone is fine.
    assert!(reject_flag_conflict(&m, "Z42_MODE", "--mode", None).is_ok());
    assert!(reject_flag_conflict(&BTreeMap::new(), "Z42_MODE", "--mode", Some("interp")).is_ok());
}

// ── complete-runtime-settings P3: 查询表面 ─────────────────────────────────

#[test]
fn list_knobs_hides_non_public_tiers_by_default() {
    let default_view: Vec<&str> = visible_knobs(false).map(|k| k.name).collect();
    let all_view: Vec<&str> = visible_knobs(true).map(|k| k.name).collect();
    assert_eq!(all_view.len(), KNOWN_KNOBS.len(), "--all must list everything");
    assert!(default_view.len() < all_view.len(), "default view must filter something");
    assert!(default_view.contains(&"Z42_GC_MODE"), "a public knob must be listed");
    assert!(!default_view.contains(&"Z42_STRESS_ITERS"), "test scaffolding must be hidden");
    assert!(!default_view.contains(&"Z42_CONFIG"), "meta knobs must be hidden");
    assert!(!default_view.contains(&"Z42_GC_SOFT_THRESHOLD"), "tuning knobs must be hidden");
}

#[test]
fn list_knobs_text_shows_the_schema_a_user_needs() {
    let text = list_knobs_text(false, &full_ctx());
    assert!(text.contains("gc-mode"), "{text}");
    assert!(text.contains("Z42_GC_MODE"));
    assert!(text.contains("set from    cli, env, user-config, app-config"));
    assert!(text.contains("enum(stw|"));
    assert!(text.contains("status      available"));
}

#[test]
fn list_knobs_marks_knobs_this_build_cannot_use() {
    let no_jit = ctx(true, &["native-interop"], "linux");
    let text = list_knobs_text(true, &no_jit);
    assert!(text.contains("UNAVAILABLE (needs feature jit)"), "{text}");
    let wasm = ctx(true, &[], "wasm");
    assert!(list_knobs_text(true, &wasm).contains("not on wasm"));
}

#[test]
fn list_knobs_json_is_valid_and_stable() {
    let v: serde_json::Value =
        serde_json::from_str(&list_knobs_json(true, &full_ctx())).expect("valid JSON");
    let knobs = v["knobs"].as_array().unwrap();
    assert_eq!(knobs.len(), KNOWN_KNOBS.len());
    let gc = knobs.iter().find(|k| k["env"] == "Z42_GC_MODE").unwrap();
    for field in ["key", "env", "aliases", "type", "sources", "tier", "available",
                  "build", "requires", "platforms", "default", "consumed_by", "description"] {
        assert!(!gc[field].is_null(), "missing schema field `{field}`");
    }
    assert_eq!(gc["sources"], serde_json::json!(["cli", "env", "user-config", "app-config"]));
    assert_eq!(v["build"]["os"], "linux");
}

#[test]
fn show_config_explains_why_a_value_did_not_take_effect() {
    let c = ctx(true, &["native-interop"], "linux"); // no jit
    let user = rt_table("gc-mode = \"stw\"");
    let (_, res) = resolve_all(
        &[("Z42_GC_MODE", "concurrent")],
        &[("Z42_JIT_PROFILE", "1")],
        Some(&user), None, &c,
    );
    let text = show_config_text(&res, true);
    assert!(text.contains("gc-mode = concurrent  [cli]"), "{text}");
    assert!(text.contains("ignored [user-config] \"stw\"  (overridden by a higher layer)"), "{text}");
    assert!(text.contains("ignored [env] \"1\"  (unavailable in this build)"), "{text}");
}

#[test]
fn show_config_default_view_still_surfaces_ignored_values() {
    // A knob the user set but that did not take effect must be visible even when
    // its tier would normally hide it — that is exactly the case worth showing.
    let c = ctx(false, &[], "linux"); // release: Z42_STRESS_ITERS is debug-only
    let (_, res) = resolve_all(&[], &[("Z42_STRESS_ITERS", "5")], None, None, &c);
    let text = show_config_text(&res, false);
    assert!(text.contains("stress-iters"), "internal knob with an ignored value must show: {text}");
    assert!(text.contains("unavailable in this build"), "{text}");
}

#[test]
fn show_config_json_is_valid() {
    let (_, res) = resolve_all(&[("Z42_GC_MODE", "concurrent")], &[], None, None, &full_ctx());
    let v: serde_json::Value =
        serde_json::from_str(&show_config_json(&res, true)).expect("valid JSON");
    let gc = v["knobs"].as_array().unwrap().iter()
        .find(|k| k["env"] == "Z42_GC_MODE").unwrap();
    assert_eq!(gc["value"], "concurrent");
    assert_eq!(gc["source"], "cli");
    assert!(gc["ignored"].as_array().unwrap().is_empty());
}

// ── complete-runtime-settings P4: 双文件层 + 格式 ───────────────────────────

fn write_cfg(name: &str, body: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("z42-cfg-{name}-{}.toml", std::process::id()));
    std::fs::write(&p, body).unwrap();
    p
}

#[test]
fn json_config_path_gets_a_migration_hint_not_silence() {
    // Someone coming from .NET writes app.runtimeconfig.json and expects it to work.
    // Silently ignoring the file costs them half an hour of debugging.
    let err = load_config_file(
        std::path::Path::new("/tmp/app.runtimeconfig.json"),
        "Z42_CONFIG",
    ).unwrap_err();
    assert!(err.contains("TOML, not JSON"), "{err}");
    assert!(err.contains("app.runtimeconfig.toml"), "must name the file they should write: {err}");
    assert!(err.contains("[runtime]"), "{err}");
}

#[test]
fn missing_config_file_is_not_fatal_but_bad_toml_is() {
    assert_eq!(
        load_config_file(std::path::Path::new("/definitely/not/here.toml"), "Z42_CONFIG"),
        Ok(None),
        "a missing file must not stop the VM — env + defaults still apply"
    );
    let bad = write_cfg("bad", "[runtime\ngc-mode = ");
    assert!(load_config_file(&bad, "Z42_CONFIG").is_err(), "malformed TOML is explicit");
    let not_table = write_cfg("nottable", "runtime = 1\n");
    assert!(load_config_file(&not_table, "Z42_CONFIG").unwrap_err().contains("must be a table"));
    let no_section = write_cfg("nosection", "version = \"0.5.0\"\n");
    assert_eq!(load_config_file(&no_section, "Z42_CONFIG"), Ok(None), "no [runtime] -> no layer");
    for p in [bad, not_table, no_section] { let _ = std::fs::remove_file(p); }
}

#[test]
fn both_file_layers_load_from_their_own_env_var() {
    let user = write_cfg("user", "[runtime]\ngc-mode = \"concurrent\"\n");
    let app = write_cfg("app", "[runtime]\nsafepoint-throttle = 64\n");
    let get = fake_env(&[
        ("Z42_CONFIG", user.to_str().unwrap()),
        ("Z42_APP_CONFIG", app.to_str().unwrap()),
    ]);
    let u = load_runtime_toml(&get).unwrap().expect("user layer");
    let a = load_app_config(&get).unwrap().expect("app layer");
    assert_eq!(u.get("gc-mode").and_then(|v| v.as_str()), Some("concurrent"));
    assert_eq!(a.get("safepoint-throttle").and_then(|v| v.as_integer()), Some(64));

    // And nothing about setting Z42_CONFIG suppresses the app layer.
    let (cfg, _) = RuntimeConfig::resolve_with(
        &fake_env(&[]),
        &Inputs { user_config: Some(&u), app_config: Some(&a), ..Default::default() },
        &full_ctx(),
    );
    assert_eq!(cfg.gc_mode, GcMode::ConcurrentMarkSweep);
    assert_eq!(cfg.safepoint_throttle, 64);
    for p in [user, app] { let _ = std::fs::remove_file(p); }
}

#[test]
fn unset_config_vars_mean_no_file_layer() {
    let get = fake_env(&[("Z42_CONFIG", "  ")]);
    assert_eq!(load_runtime_toml(&get), Ok(None));
    assert_eq!(load_app_config(&get), Ok(None));
}

// ── add-gc-runtime-knobs (2026-09-05) ────────────────────────────────────────

#[test]
fn gc_max_bytes_accepts_plain_counts_and_binary_suffixes() {
    let cases: &[(&str, Option<u64>)] = &[
        ("536870912", Some(536870912)),
        ("512MB",     Some(512 * 1024 * 1024)),
        ("512mb",     Some(512 * 1024 * 1024)),
        ("512 MB",    Some(512 * 1024 * 1024)),
        ("2G",        Some(2 * 1024 * 1024 * 1024)),
        ("64k",       Some(64 * 1024)),
        // Explicitly-unlimited spellings, and the historical "unset" default.
        ("0",         None),
        ("unlimited", None),
        ("none",      None),
        ("",          None),
    ];
    for (raw, want) in cases {
        let env = fake_env(&[("Z42_GC_MAX_BYTES", raw)]);
        assert_eq!(parse_gc_max_bytes(&env), *want, "Z42_GC_MAX_BYTES={raw:?}");
    }
}

#[test]
fn gc_max_bytes_rejects_garbage_as_unlimited_rather_than_guessing() {
    // A wrong budget is worse than none: guessing here would silently change
    // collection behaviour on a typo.
    for raw in ["abc", "12x", "1.5GB", "-1", "99999999999999999999G"] {
        let env = fake_env(&[("Z42_GC_MAX_BYTES", raw)]);
        assert_eq!(parse_gc_max_bytes(&env), None, "Z42_GC_MAX_BYTES={raw:?}");
    }
}

#[test]
fn bool_knob_honours_falsey_spellings() {
    for raw in ["0", "false", "FALSE", "off", "no", " ", ""] {
        let env = fake_env(&[("Z42_GC_TRACE", raw)]);
        assert!(!parse_bool_knob(&env, "Z42_GC_TRACE"), "{raw:?} must be off");
    }
    for raw in ["1", "true", "on", "yes", "anything"] {
        let env = fake_env(&[("Z42_GC_TRACE", raw)]);
        assert!(parse_bool_knob(&env, "Z42_GC_TRACE"), "{raw:?} must be on");
    }
    let empty = fake_env(&[]);
    assert!(!parse_bool_knob(&empty, "Z42_GC_TRACE"), "unset must be off");
}

#[test]
fn new_gc_knobs_are_registered_in_known_knobs() {
    let names: Vec<&str> = KNOWN_KNOBS.iter().map(|k| k.name).collect();
    for required in ["Z42_GC_MAX_BYTES", "Z42_GC_TRACE"] {
        assert!(names.contains(&required), "{required} missing from KNOWN_KNOBS");
    }
}

#[test]
fn bool_knobs_are_type_checked_one_layer_above_parse_bool_knob() {
    // `parse_bool_knob` is deliberately permissive ("anything but 0/false/off/no
    // is on") — the test above pins that. But both bool knobs now declare
    // `ValueKind::Bool`, so a non-boolean is caught in `resolve_knobs` and never
    // reaches the parser: it becomes an explicit diagnostic + the default.
    //
    // Net effect vs. add-gc-runtime-knobs alone: `Z42_GC_TRACE=ture` (a typo) used
    // to silently turn tracing ON; it now says "expected a boolean" and stays off.
    // Deliberate tightening — a typo that silently changes behaviour is worse than
    // one that is reported.
    assert_eq!(spec_named("Z42_GC_TRACE").value, ValueKind::Bool);
    assert_eq!(spec_named("Z42_JIT_PROFILE").value, ValueKind::Bool);

    let c = full_ctx();
    let (cfg, res) = resolve_all(&[], &[("Z42_GC_TRACE", "ture")], None, None, &c);
    assert!(!cfg.gc_trace, "a non-boolean must not enable tracing");
    assert!(res.diagnostics[0].message.contains("expected a boolean"), "{:?}", res.diagnostics[0]);

    // Valid spellings still work end to end.
    for (raw, want) in [("1", true), ("on", true), ("false", false), ("0", false)] {
        let (cfg, res) = resolve_all(&[], &[("Z42_GC_TRACE", raw)], None, None, &c);
        assert_eq!(cfg.gc_trace, want, "Z42_GC_TRACE={raw}");
        assert!(res.diagnostics.is_empty());
    }
}

#[test]
fn gc_max_bytes_is_a_string_knob_because_it_takes_unit_suffixes() {
    // Declaring it Int would reject `512MB` at the resolve layer before
    // parse_gc_max_bytes ever sees it.
    assert_eq!(spec_named("Z42_GC_MAX_BYTES").value, ValueKind::Str);
    let (cfg, res) = resolve_all(&[], &[("Z42_GC_MAX_BYTES", "512MB")], None, None, &full_ctx());
    assert_eq!(cfg.gc_max_bytes, Some(512 * 1024 * 1024));
    assert!(res.diagnostics.is_empty());
}

// ── adopt-inline-env-knobs: 收编的 8 个旋钮 ────────────────────────────────

const ADOPTED: &[&str] = &[
    "Z42_FUSION_DEBUG", "Z42_JIT_DEBUG_PROMOTE", "Z42_JIT_THRESHOLD", "Z42_NO_FUSION",
    "Z42_NO_TYPED_FUSION", "Z42_OSR_THRESHOLD", "Z42_REPL_NATIVE", "Z42_STACKALLOC",
];

#[test]
fn adopted_knobs_are_settable_from_every_layer() {
    for name in ADOPTED {
        let spec = spec_named(name);
        assert_eq!(spec.sources, LayerMask::ALL,
            "{name}: consumed via runtime_config() now, so all four layers must work");
        assert!(!spec.consumed_by.contains("inline env read"),
            "{name}: consumed_by still claims an inline env read");
    }
}

#[test]
fn env_only_is_now_reserved_for_scaffolding_and_meta_knobs() {
    // The registry should not carry "you may only set this from env" for anything
    // that a user could reasonably want on the command line.
    let env_only: Vec<&str> = KNOWN_KNOBS.iter()
        .filter(|k| k.sources == LayerMask::ENV_ONLY).map(|k| k.name).collect();
    assert_eq!(env_only, vec!["Z42_STRESS_ITERS"],
        "ENV_ONLY is for test scaffolding only; meta knobs use CLI_ENV");
}

#[test]
fn adopted_knobs_resolve_through_the_full_chain() {
    let app = rt_table("jit-threshold = 7\nstackalloc = \"stats\"");
    let (cfg, res) = resolve_all(
        &[("Z42_OSR_THRESHOLD", "500")],
        &[("Z42_JIT_THRESHOLD", "3")],
        None, Some(&app), &full_ctx(),
    );
    assert_eq!(cfg.osr_threshold, 500, "cli layer");
    assert_eq!(cfg.jit_threshold, 3, "env beats the app sidecar");
    assert_eq!(cfg.stackalloc.as_deref(), Some("stats"), "app-config layer");
    assert_eq!(res.get("Z42_JIT_THRESHOLD").unwrap().source, Layer::Env);
    assert_eq!(res.get("Z42_STACKALLOC").unwrap().source, Layer::AppConfig);
}

#[test]
fn switch_knobs_honour_falsey_values_end_to_end() {
    let c = full_ctx();
    for raw in ["false", "0", "off", "no"] {
        let (cfg, res) = resolve_all(&[], &[("Z42_NO_FUSION", raw)], None, None, &c);
        assert!(!cfg.no_fusion, "Z42_NO_FUSION={raw} must NOT disable fusion");
        assert!(res.diagnostics.is_empty(), "{raw} is a valid boolean");
    }
    for raw in ["true", "1", "on", "yes"] {
        let (cfg, _) = resolve_all(&[], &[("Z42_NO_FUSION", raw)], None, None, &c);
        assert!(cfg.no_fusion, "Z42_NO_FUSION={raw} must disable fusion");
    }
    let (cfg, res) = resolve_all(&[], &[("Z42_NO_FUSION", "maybe")], None, None, &c);
    assert!(!cfg.no_fusion);
    assert!(res.diagnostics[0].message.contains("expected a boolean"), "{:?}", res.diagnostics[0]);
}

#[test]
fn no_typed_fusion_keeps_the_knobs_own_polarity() {
    // The field is named after the knob (negative), not flipped to a positive
    // `typed_fusion_enabled` — the table's name is the SoT and --show-config prints
    // `no-typed-fusion`. The single inversion lives at the one call site that wants it.
    let (cfg, _) = resolve_all(&[], &[("Z42_NO_TYPED_FUSION", "true")], None, None, &full_ctx());
    assert!(cfg.no_typed_fusion);
    let (cfg, _) = resolve_all(&[], &[], None, None, &full_ctx());
    assert!(!cfg.no_typed_fusion, "typed fusion is on by default");
}

#[test]
fn thresholds_keep_their_previous_semantics() {
    let c = full_ctx();
    let (d, _) = resolve_all(&[], &[], None, None, &c);
    assert_eq!(d.jit_threshold, 2, "lower-jit-threshold-default");
    assert_eq!(d.osr_threshold, 10_000, "add-osr-loop-tiering");

    // 0 clamps to 1 (compiling "every zeroth call" means every call).
    let (z, _) = resolve_all(&[], &[("Z42_JIT_THRESHOLD", "0")], None, None, &c);
    assert_eq!(z.jit_threshold, 1);

    // Garbage falls back to the default — but is no longer SILENT about it.
    let (g, res) = resolve_all(&[], &[("Z42_JIT_THRESHOLD", "abc")], None, None, &c);
    assert_eq!(g.jit_threshold, 2);
    assert!(res.diagnostics[0].message.contains("expected an integer"), "{:?}", res.diagnostics[0]);
}

#[test]
fn stackalloc_typos_are_reported_instead_of_silently_meaning_on() {
    // The consumer's match ends in `_ => MODE_ON`, so `Z42_STACKALLOC=of` (a typo for
    // `off`) used to silently leave the optimisation ON while someone was mid-triage
    // believing they had turned it off. The Enum check now catches it first.
    let c = full_ctx();
    for raw in ["off", "0", "heap", "stats", "on"] {
        let (cfg, res) = resolve_all(&[], &[("Z42_STACKALLOC", raw)], None, None, &c);
        assert_eq!(cfg.stackalloc.as_deref(), Some(raw));
        assert!(res.diagnostics.is_empty(), "{raw} is a declared value");
    }
    let (cfg, res) = resolve_all(&[], &[("Z42_STACKALLOC", "of")], None, None, &c);
    assert_eq!(cfg.stackalloc, None, "rejected -> the consumer sees no override");
    assert!(res.diagnostics[0].message.contains("expected one of"), "{:?}", res.diagnostics[0]);
}

#[test]
fn repl_native_is_a_path_override() {
    let (cfg, _) = resolve_all(&[], &[("Z42_REPL_NATIVE", "/opt/z42/libz42_repl.dylib")],
                               None, None, &full_ctx());
    assert_eq!(cfg.repl_native, Some(std::path::PathBuf::from("/opt/z42/libz42_repl.dylib")));
}

// ── sidecar-reaches-published-apps P0: 库入口也读文件层 ────────────────────
//
// `from_env` 是唯一真正读进程环境的入口，也是每个**非 z42vm** 入口的必经之路
// （`runtime_config()` 懒初始化 → `z42_host_run_app` → desktop 自包含 apphost /
// wasm / iOS / Android / testhost）。它此前只读 env，于是嵌入方静默丢掉 L3/L4。
//
// 这些测试要动真实进程环境，故串行（`env_lock`）并在结束时还原。

use std::sync::Mutex;
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// 在受控的真实环境变量下跑 `f`，结束还原。`from_env` 只认真实 env，无法注入。
fn with_env<R>(pairs: &[(&str, Option<&str>)], f: impl FnOnce() -> R) -> R {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let saved: Vec<(String, Option<String>)> =
        pairs.iter().map(|(k, _)| ((*k).to_string(), std::env::var(k).ok())).collect();
    for (k, v) in pairs {
        // Safety: serialised by ENV_LOCK; these tests do not spawn threads.
        match v {
            Some(v) => unsafe { std::env::set_var(k, v) },
            None => unsafe { std::env::remove_var(k) },
        }
    }
    let out = f();
    for (k, v) in saved {
        match v {
            Some(v) => unsafe { std::env::set_var(&k, v) },
            None => unsafe { std::env::remove_var(&k) },
        }
    }
    out
}

#[test]
fn from_env_reads_the_user_config_layer() {
    let f = write_cfg("fromenv-user", "[runtime]\ngc-mode = \"concurrent\"\n");
    let got = with_env(
        &[("Z42_CONFIG", f.to_str()), ("Z42_APP_CONFIG", None), ("Z42_GC_MODE", None)],
        RuntimeConfig::from_env,
    );
    assert_eq!(got.gc_mode, GcMode::ConcurrentMarkSweep, "embedders must see Z42_CONFIG");
    assert_eq!(got.resolved.iter().find(|r| r.name == "Z42_GC_MODE").unwrap().source,
               Layer::UserConfig);
    let _ = std::fs::remove_file(f);
}

#[test]
fn from_env_reads_the_app_sidecar_layer() {
    let f = write_cfg("fromenv-app", "[runtime]\ngc-mode = \"generational\"\n");
    let got = with_env(
        &[("Z42_CONFIG", None), ("Z42_APP_CONFIG", f.to_str()), ("Z42_GC_MODE", None)],
        RuntimeConfig::from_env,
    );
    assert_eq!(got.gc_mode, GcMode::GenerationalMarkSweep, "embedders must see Z42_APP_CONFIG");
    assert_eq!(got.resolved.iter().find(|r| r.name == "Z42_GC_MODE").unwrap().source,
               Layer::AppConfig);
    let _ = std::fs::remove_file(f);
}

#[test]
fn from_env_layers_user_over_app_and_env_over_both() {
    let user = write_cfg("fromenv-both-u", "[runtime]\nmode = \"jit\"\n");
    let app = write_cfg("fromenv-both-a", "[runtime]\nmode = \"interp\"\nsafepoint-throttle = 64\n");
    let got = with_env(
        &[("Z42_CONFIG", user.to_str()), ("Z42_APP_CONFIG", app.to_str()),
          ("Z42_MODE", None), ("Z42_SAFEPOINT_THROTTLE", None)],
        RuntimeConfig::from_env,
    );
    assert_eq!(got.mode.as_deref(), Some("jit"), "same key -> user layer wins");
    assert_eq!(got.safepoint_throttle, 64, "app-only key still applies");

    // env beats both.
    let got = with_env(
        &[("Z42_CONFIG", user.to_str()), ("Z42_APP_CONFIG", app.to_str()),
          ("Z42_MODE", Some("aot")), ("Z42_SAFEPOINT_THROTTLE", None)],
        RuntimeConfig::from_env,
    );
    assert_eq!(got.mode.as_deref(), Some("aot"));
    for p in [user, app] { let _ = std::fs::remove_file(p); }
}

#[test]
fn from_env_downgrades_a_broken_config_file_instead_of_dying() {
    // The whole point of the lenient path: an embedder must not lose its process
    // over a config typo. Malformed TOML -> that layer is gone, everything else works.
    let bad = write_cfg("fromenv-bad", "[runtime\ngc-mode = ");
    let got = with_env(
        &[("Z42_CONFIG", bad.to_str()), ("Z42_APP_CONFIG", None), ("Z42_GC_MODE", Some("concurrent"))],
        RuntimeConfig::from_env,
    );
    assert_eq!(got.gc_mode, GcMode::ConcurrentMarkSweep, "env layer must still apply");
    let _ = std::fs::remove_file(bad);

    // Same for the JSON migration error — a hint, not a hard stop.
    let got = with_env(
        &[("Z42_CONFIG", Some("/tmp/nope.runtimeconfig.json")), ("Z42_APP_CONFIG", None),
          ("Z42_GC_MODE", None)],
        RuntimeConfig::from_env,
    );
    assert_eq!(got.gc_mode, GcMode::default());
}

#[test]
fn from_getter_still_has_no_file_layer() {
    // Non-breaking guarantee: the injectable path is unchanged — it reads nothing
    // off disk, which is what keeps the rest of this file hermetic.
    let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_CONFIG", "/definitely/not/read.toml")]));
    assert_eq!(cfg.gc_mode, GcMode::default());
    assert!(cfg.resolved.iter().all(|r| r.source != Layer::UserConfig
                                     && r.source != Layer::AppConfig));
}

// ── app-config-follows-the-app: 侧车由 app 文件推导 ────────────────────────

#[test]
fn sidecar_is_derived_by_replacing_the_apps_extension() {
    let d = std::env::temp_dir().join(format!("z42-sidecar-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&d);
    let app = d.join("app.zpkg");
    std::fs::write(&app, b"zpkg").unwrap();

    assert_eq!(sidecar_for(&app), None, "no sidecar is the normal case, not an error");

    let side = d.join("app.runtimeconfig.toml");
    std::fs::write(&side, b"[runtime]\n").unwrap();
    assert_eq!(sidecar_for(&app), Some(side.clone()),
        "app.zpkg -> app.runtimeconfig.toml (replace, not append)");

    // A bare .zbc app works the same way.
    let zbc = d.join("app.zbc");
    std::fs::write(&zbc, b"zbc").unwrap();
    assert_eq!(sidecar_for(&zbc), Some(side.clone()), "stem-derived, extension-agnostic");

    // A directory of that name is not a config file.
    let dird = d.join("dir.zpkg");
    std::fs::write(&dird, b"zpkg").unwrap();
    std::fs::create_dir_all(d.join("dir.runtimeconfig.toml")).unwrap();
    assert_eq!(sidecar_for(&dird), None);

    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_derived_sidecar_lands_in_the_app_config_layer_and_loses_to_the_user() {
    let d = std::env::temp_dir().join(format!("z42-sidecar-layer-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&d);
    let app = d.join("demo.zpkg");
    std::fs::write(&app, b"zpkg").unwrap();
    std::fs::write(d.join("demo.runtimeconfig.toml"),
                   b"[runtime]\ngc-mode = \"stw\"\nsafepoint-throttle = 64\n").unwrap();

    let derived = sidecar_for(&app).expect("derived");
    let app_table = load_config_file(&derived, "app sidecar").unwrap().expect("[runtime]");
    let user = rt_table("gc-mode = \"concurrent\"");

    let (cfg, res) = RuntimeConfig::resolve_with(
        &fake_env(&[]),
        &Inputs { user_config: Some(&user), app_config: Some(&app_table), ..Default::default() },
        &full_ctx(),
    );
    assert_eq!(cfg.gc_mode, GcMode::ConcurrentMarkSweep, "user config still wins per key");
    assert_eq!(cfg.safepoint_throttle, 64, "sidecar-only key applies");
    assert_eq!(res.get("Z42_SAFEPOINT_THROTTLE").unwrap().source, Layer::AppConfig);

    let _ = std::fs::remove_dir_all(&d);
}

// ── add-app-properties: 属性与旋钮分表 ─────────────────────────────────────

#[test]
fn properties_are_read_alongside_runtime_but_kept_separate() {
    let f = write_cfg("props", "[runtime]\ngc-mode = \"concurrent\"\n\n\
        [properties]\napi = \"https://x\"\nretries = 3\nflags = [\"a\", \"b\"]\n\
        [properties.limits]\nmax = 9\n");
    let (rt, props) = load_config_tables(&f, "Z42_APP_CONFIG").unwrap();
    assert_eq!(rt.as_ref().and_then(|t| t.get("gc-mode")).and_then(|v| v.as_str()),
               Some("concurrent"));
    let props = props.expect("[properties] parsed");
    assert_eq!(props.get("api").and_then(|v| v.as_str()), Some("https://x"));
    assert_eq!(props.get("retries").and_then(|v| v.as_integer()), Some(3));
    assert!(props.get("flags").and_then(|v| v.as_array()).is_some(), "arrays survive");
    assert!(props.get("limits").and_then(|v| v.as_table()).is_some(), "nested tables survive");
    let _ = std::fs::remove_file(f);
}

#[test]
fn property_keys_never_trigger_the_unknown_knob_diagnostic() {
    // 分表的**全部理由**：若属性与旋钮同住 `[runtime]`，`gc-mdoe` 这种 typo 就会被
    // 当成一个合法的用户属性静默收下，「未知旋钮就明确报出来」的诊断随之失效。
    let f = write_cfg("props-sep", "[runtime]\ngc-mode = \"stw\"\n\
        [properties]\nmy-own-thing = 1\n");
    let (rt, props) = load_config_tables(&f, "Z42_APP_CONFIG").unwrap();
    assert!(unknown_table_keys(rt.as_ref().unwrap()).is_empty(),
        "属性不在 [runtime] 里，故不会被当成未知旋钮");
    assert!(props.unwrap().contains_key("my-own-thing"));
    // 而 [runtime] 里真正的未知键**仍然**被抓。
    let g = write_cfg("props-typo", "[runtime]\ngc-mdoe = \"stw\"\n");
    let (rt2, _) = load_config_tables(&g, "Z42_APP_CONFIG").unwrap();
    assert_eq!(unknown_table_keys(rt2.as_ref().unwrap()), vec!["gc-mdoe".to_string()]);
    for p in [f, g] { let _ = std::fs::remove_file(p); }
}

#[test]
fn properties_must_be_a_table() {
    let f = write_cfg("props-scalar", "[runtime]\ngc-mode = \"stw\"\nproperties = 1\n");
    // 顶层 `properties = 1`（非表）—— [runtime] 之外，故是文档顶层的键。
    let (_, props) = load_config_tables(&f, "Z42_APP_CONFIG").unwrap_or((None, None));
    assert!(props.is_none());
    let g = write_cfg("props-bad", "[properties]\nok = 1\n");
    let (_, p2) = load_config_tables(&g, "Z42_APP_CONFIG").unwrap();
    assert!(p2.is_some(), "没有 [runtime] 段也能有 [properties]");
    for p in [f, g] { let _ = std::fs::remove_file(p); }
}

#[test]
fn app_properties_are_not_knobs() {
    // 属性不在登记表里——所以 `--set my-own-thing=1` 会按未知旋钮报错，
    // 而不是悄悄变成一个属性。
    assert!(knob_by_key("my-own-thing").is_none());
    assert!(knob_by_env_name("properties").is_none());
    // 也不占 RuntimeConfig 的解析链：默认构造时为空。
    assert!(RuntimeConfig::default().app_properties.is_none());
}
