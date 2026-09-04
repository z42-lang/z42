//! `config.rs` 的单测（runtime-rust.md：测试独立文件；refactor-split-config 自内联 `mod tests` 搬出）。
use super::*;
use std::collections::HashMap;

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
    assert_eq!(toml_key_for("Z42_GC_MODE"), Some("gc-mode"));
    assert_eq!(toml_key_for("Z42_GC_MINOR_THRESHOLD"), Some("gc-minor-threshold"));
    // Z42_CONFIG is a meta pointer — no [runtime] value key.
    assert_eq!(toml_key_for("Z42_CONFIG"), Some(""));
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
    assert_eq!(toml_key_for("Z42_MODE"), Some("mode"));
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
