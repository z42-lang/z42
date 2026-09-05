//! 单个旋钮的解析函数（`parse_*`、toml 键映射与标量转换）。
//! refactor-split-config（2026-09-03）：自 config.rs 逐行搬出（私有 fn 改 `pub(super)` 供 `RuntimeConfig::from_env` 用）。

#![allow(unused_imports)]
use super::*;
use crate::gc::GcMode;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Parse `Z42_SAMPLE_HZ` → `Some(hz)` (hz ≥ 1) enables sampling; missing / empty
/// / invalid / `0` → `None` (off). A `0` or garbage value warns then disables.
pub(super) fn parse_sample_hz<F>(get: &F) -> Option<u32>
where F: Fn(&str) -> Option<String> {
    let raw = get("Z42_SAMPLE_HZ").filter(|s| !s.trim().is_empty())?;
    match raw.trim().parse::<u32>() {
        Ok(hz) if hz >= 1 => Some(hz),
        _ => {
            eprintln!("z42: invalid Z42_SAMPLE_HZ={raw:?} (want integer ≥ 1); sampling disabled");
            None
        }
    }
}

/// Render a scalar TOML value the way the env-string parsers expect. Non-scalar
/// values (array / table / datetime) are not valid knob values → `None`.
pub(super) fn toml_scalar_to_string(v: &toml::Value) -> Option<String> {
    match v {
        toml::Value::String(s)  => Some(s.clone()),
        toml::Value::Integer(i) => Some(i.to_string()),
        toml::Value::Float(f)   => Some(f.to_string()),
        toml::Value::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

// ── Phase 2 parsers (one per subsystem knob) ─────────────────────────────────
//
// Centralised so `from_getter` reads as a flat list of field assignments;
// each parser owns its own default + invalid-value `eprintln`.

pub(super) fn parse_gc_mode<F>(get: &F) -> GcMode
where F: Fn(&str) -> Option<String> {
    let Some(s) = get("Z42_GC_MODE").filter(|s| !s.trim().is_empty()) else {
        return GcMode::default();
    };
    match s.as_str() {
        "concurrent" | "concurrent-mark-sweep"     => GcMode::ConcurrentMarkSweep,
        "generational" | "generational-mark-sweep" => GcMode::GenerationalMarkSweep,
        "stw" | "stw-mark-sweep"                   => GcMode::StwMarkSweep,
        other => {
            eprintln!("z42: Z42_GC_MODE={other:?} not recognized; falling back to stw-mark-sweep");
            GcMode::StwMarkSweep
        }
    }
}

pub(super) fn parse_gc_minor_threshold<F>(get: &F) -> f32
where F: Fn(&str) -> Option<String> {
    let Some(raw) = get("Z42_GC_MINOR_THRESHOLD").filter(|s| !s.trim().is_empty()) else {
        return 0.75;
    };
    match raw.parse::<f32>() {
        Ok(v) if v > 0.0 && v <= 1.0 => v,
        _ => {
            eprintln!("z42: invalid Z42_GC_MINOR_THRESHOLD={raw:?}; using default 0.75");
            0.75
        }
    }
}

/// Hard ceiling — 65536 × 8 bytes per slot = 512 KB per heap pause-window
/// deque. Generous but prevents a hostile env from allocating GB.
pub(super) const GC_PAUSE_WINDOW_MAX: usize = 65536;
pub(super) const GC_PAUSE_WINDOW_DEFAULT: usize = 1024;

pub(super) fn parse_gc_pause_window<F>(get: &F) -> usize
where F: Fn(&str) -> Option<String> {
    let Some(raw) = get("Z42_GC_PAUSE_WINDOW").filter(|s| !s.trim().is_empty()) else {
        return GC_PAUSE_WINDOW_DEFAULT;
    };
    match raw.parse::<i64>() {
        Ok(n) if n >= 1 => (n as usize).min(GC_PAUSE_WINDOW_MAX),
        _               => GC_PAUSE_WINDOW_DEFAULT,
    }
}

pub(super) fn parse_gc_soft_threshold<F>(get: &F) -> f64
where F: Fn(&str) -> Option<String> {
    let Some(raw) = get("Z42_GC_SOFT_THRESHOLD").filter(|s| !s.trim().is_empty()) else {
        return 0.80;
    };
    raw.parse::<f64>()
        .map(|v| v.clamp(0.0, 1.0))
        .unwrap_or(0.80)
}

/// Shared parser for the GC auto-collect ratio knobs (`Z42_GC_NEAR_LIMIT_RATIO`
/// / `Z42_GC_PRESSURE_RATIO` / `Z42_GC_THROTTLE_RATIO`). Missing / empty →
/// `default`; a parseable value is clamped to `[0.0, 1.0]`; unparseable → warns
/// then falls back to `default`. Cross-knob ordering (pressure < near) is *not*
/// enforced here — an inverted pair simply makes the pressure-event branch dead,
/// harmless; keeping each knob independent avoids surprising silent rewrites.
pub(super) fn parse_gc_ratio<F>(get: &F, name: &str, default: f64) -> f64
where F: Fn(&str) -> Option<String> {
    let Some(raw) = get(name).filter(|s| !s.trim().is_empty()) else {
        return default;
    };
    match raw.parse::<f64>() {
        Ok(v) => v.clamp(0.0, 1.0),
        Err(_) => {
            eprintln!("z42: invalid {name}={raw:?}; using default {default}");
            default
        }
    }
}

pub(super) fn parse_safepoint_throttle<F>(get: &F) -> u32
where F: Fn(&str) -> Option<String> {
    let Some(raw) = get("Z42_SAFEPOINT_THROTTLE").filter(|s| !s.trim().is_empty()) else {
        return 1024;
    };
    match raw.parse::<u32>() {
        Ok(n) if n >= 1 => n,
        _ => {
            eprintln!("z42: invalid Z42_SAFEPOINT_THROTTLE={raw:?}; using default 1024");
            1024
        }
    }
}

pub(super) fn parse_native_search_paths<F>(get: &F) -> Vec<PathBuf>
where F: Fn(&str) -> Option<String> {
    get("Z42_NATIVE_PATH")
        .filter(|s| !s.trim().is_empty())
        .map(|s| split_paths(&s))
        .unwrap_or_default()
}

// ── Config-file (`[runtime]`) loading ────────────────────────────────────────

/// `Z42_GC_MAX_BYTES` — soft heap-size budget that **arms auto-collect**
/// (add-gc-runtime-knobs, 2026-09-05).
///
/// This is the knob the rest of the GC family hangs off: `maybe_auto_collect`
/// returns immediately while `max_bytes` is `None`, and
/// `Z42_GC_NEAR_LIMIT_RATIO` / `Z42_GC_PRESSURE_RATIO` / `Z42_GC_THROTTLE_RATIO`
/// are all *fractions of it*. With no budget set there is no automatic
/// collection at all and those three ratios are inert — the historical default,
/// kept for compatibility (`None` = unlimited, collect only on explicit
/// `Std.GC.Collect()`).
///
/// Accepts a plain byte count or a `K`/`KB`/`M`/`MB`/`G`/`GB` suffix (binary
/// units, case-insensitive, optional space): `536870912`, `512MB`, `2 g`.
/// `0` / `unlimited` / `none` → `None`. Invalid input warns and falls back to
/// `None` rather than guessing a budget.
pub(super) fn parse_gc_max_bytes<F>(get: &F) -> Option<u64>
where F: Fn(&str) -> Option<String> {
    let raw = get("Z42_GC_MAX_BYTES").filter(|s| !s.trim().is_empty())?;
    let t = raw.trim().to_ascii_lowercase();
    if t == "0" || t == "unlimited" || t == "none" {
        return None;
    }
    let (digits, mult) = match t.strip_suffix("gb").or_else(|| t.strip_suffix('g')) {
        Some(d) => (d, 1024u64 * 1024 * 1024),
        None => match t.strip_suffix("mb").or_else(|| t.strip_suffix('m')) {
            Some(d) => (d, 1024u64 * 1024),
            None => match t.strip_suffix("kb").or_else(|| t.strip_suffix('k')) {
                Some(d) => (d, 1024u64),
                None => (t.as_str(), 1u64),
            },
        },
    };
    match digits.trim().parse::<u64>() {
        Ok(n) => match n.checked_mul(mult) {
            Some(v) if v > 0 => Some(v),
            // 0 was handled above; only an overflowing product lands here.
            _ => {
                eprintln!("z42: Z42_GC_MAX_BYTES={raw:?} overflows u64; treating as unlimited");
                None
            }
        },
        Err(_) => {
            eprintln!(
                "z42: invalid Z42_GC_MAX_BYTES={raw:?} \
                 (expected a byte count, optionally suffixed K/KB/M/MB/G/GB); \
                 treating as unlimited"
            );
            None
        }
    }
}

/// Shared parser for on/off knobs (add-gc-runtime-knobs, 2026-09-05).
///
/// Truthy: any non-empty value other than the falsey set. Falsey:
/// `0` / `false` / `off` / `no` (case-insensitive). Unset / empty → `false`.
/// Spelling `=false` and getting "on" is the kind of thing that costs an hour
/// of debugging, so the falsey set is honoured rather than "non-empty = on".
pub(super) fn parse_bool_knob<F>(get: &F, name: &str) -> bool
where F: Fn(&str) -> Option<String> {
    match get(name) {
        Some(raw) => {
            let t = raw.trim().to_ascii_lowercase();
            !(t.is_empty() || t == "0" || t == "false" || t == "off" || t == "no")
        }
        None => false,
    }
}
