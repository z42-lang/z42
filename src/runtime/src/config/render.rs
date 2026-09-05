//! `--info` / `--list-knobs` / `--show-config` 的**唯一**渲染器（text + json）。
//!
//! 输入统一是 `&[KnobSpec]`（schema）+ `&Resolution`（生效值与 provenance）。
//! 渲染器**不**自己去查 env——`--info` 从前正是那么做的，于是优先级链有两份
//! 实现、可以各自漂移。
//!
//! JSON 走手写序列化而非 serde derive：`KnobSpec` 里有 `&'static str` 与
//! 自定义枚举，为一个只出不入的输出面加一套 `Serialize` impl 不划算，字段名也
//! 更容易被稳定住（工具依赖它）。
//!
//! complete-runtime-settings P3（2026-09-05）。

use super::knobs::*;
use super::*;

/// `--list-knobs` 的过滤：默认只列 `Public`。
///
/// GC 的六个比例旋钮是给调优者的，普通用户看到只增噪音与误调风险；三个元旋钮是
/// 机制内部件；`Z42_STRESS_ITERS` 是测试脚手架；`Z42_TARGET` 是占位。CoreCLR 的
/// `INTERNAL_/UNSUPPORTED_` 前缀正是这个作用。
pub fn visible_knobs(all: bool) -> impl Iterator<Item = &'static KnobSpec> {
    KNOWN_KNOBS.iter().filter(move |k| all || k.tier == Tier::Public)
}

fn availability_label(spec: &KnobSpec, ctx: &BuildCtx) -> String {
    if is_available(spec, ctx) {
        return "available".to_string();
    }
    let mut why: Vec<String> = Vec::new();
    if spec.build == BuildAvail::DebugOnly && !ctx.debug {
        why.push("debug builds only".to_string());
    }
    let missing: Vec<&str> = spec.requires.iter().copied().filter(|f| !ctx.features.contains(f)).collect();
    if !missing.is_empty() {
        why.push(format!("needs feature {}", missing.join("+")));
    }
    if !matches!(spec.platforms, PlatformAvail::All) && !is_available(spec, ctx) && why.is_empty() {
        why.push(format!("not on {}", ctx.os));
    }
    format!("UNAVAILABLE ({})", why.join("; "))
}

// ── --list-knobs ─────────────────────────────────────────────────────────────

/// Schema 转储（人类可读）。
pub fn list_knobs_text(all: bool, ctx: &BuildCtx) -> String {
    let mut out = String::new();
    let knobs: Vec<&KnobSpec> = visible_knobs(all).collect();
    out.push_str(&format!(
        "runtime knobs ({} of {}{})\n",
        knobs.len(),
        KNOWN_KNOBS.len(),
        if all { "" } else { "; pass --all for unsupported + internal knobs" }
    ));
    for k in knobs {
        let key = if k.toml_key.is_empty() { "(meta; env only)" } else { k.toml_key };
        out.push_str(&format!("\n{key}\n"));
        out.push_str(&format!("  env         {}\n", k.name));
        if !k.aliases.is_empty() {
            out.push_str(&format!("  aliases     {}\n", k.aliases.join(", ")));
        }
        out.push_str(&format!("  type        {}\n", k.value.label()));
        out.push_str(&format!("  set from    {}\n", k.sources.labels().join(", ")));
        out.push_str(&format!("  tier        {}\n", k.tier.label()));
        out.push_str(&format!("  status      {}\n", availability_label(k, ctx)));
        out.push_str(&format!("  default     {}\n", k.default_hint));
        out.push_str(&format!("  read by     {}\n", k.consumed_by));
        out.push_str(&format!("  {}\n", k.description));
    }
    out
}

/// Schema 转储（机器可读）。字段名对工具稳定。
pub fn list_knobs_json(all: bool, ctx: &BuildCtx) -> String {
    let items: Vec<serde_json::Value> = visible_knobs(all)
        .map(|k| {
            serde_json::json!({
                "key":         k.toml_key,
                "env":         k.name,
                "aliases":     k.aliases,
                "type":        k.value.label(),
                "sources":     k.sources.labels(),
                "tier":        k.tier.label(),
                "available":   is_available(k, ctx),
                "build":       if k.build == BuildAvail::DebugOnly { "debug-only" } else { "always" },
                "requires":    k.requires,
                "platforms":   platform_json(k.platforms),
                "default":     k.default_hint,
                "consumed_by": k.consumed_by,
                "description": k.description,
            })
        })
        .collect();
    let doc = serde_json::json!({
        "z42vm":  env!("CARGO_PKG_VERSION"),
        "build":  { "profile": if ctx.debug { "debug" } else { "release" },
                    "features": ctx.features, "os": ctx.os },
        "knobs":  items,
    });
    serde_json::to_string_pretty(&doc).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}

fn platform_json(p: PlatformAvail) -> serde_json::Value {
    match p {
        PlatformAvail::All => serde_json::json!({ "kind": "all" }),
        PlatformAvail::Only(l) => serde_json::json!({ "kind": "only", "list": l }),
        PlatformAvail::Except(l) => serde_json::json!({ "kind": "except", "list": l }),
    }
}

// ── --show-config / --info 的旋钮块 ──────────────────────────────────────────

fn ignore_note(reason: &IgnoreReason) -> String {
    match reason {
        IgnoreReason::Overridden => "overridden by a higher layer".to_string(),
        IgnoreReason::Unavailable(_) => "unavailable in this build".to_string(),
        IgnoreReason::Invalid(why) => format!("invalid: {why}"),
    }
}

/// 生效值 + 来源 + "某层的值为什么没生效"的解释行。
///
/// `all` 为 false 时只列 `Public` 旋钮**以及**任何有 `ignored` 记录的旋钮——
/// 用户设了却没生效的东西必须看得见，哪怕它是 internal tier。
pub fn show_config_text(res: &Resolution, all: bool) -> String {
    let mut out = String::new();
    for k in KNOWN_KNOBS {
        let Some(r) = res.get(k.name) else { continue };
        if !all && k.tier != Tier::Public && r.ignored.is_empty() && r.raw.is_none() {
            continue;
        }
        let key = if k.toml_key.is_empty() { k.name } else { k.toml_key };
        match &r.raw {
            Some(v) => out.push_str(&format!("{key} = {v}  [{}]\n", r.source.label())),
            None => out.push_str(&format!("{key} = (default: {})  [default]\n", k.default_hint)),
        }
        for ig in &r.ignored {
            out.push_str(&format!(
                "  ignored [{}] {:?}  ({})\n",
                ig.layer.label(), ig.value, ignore_note(&ig.reason)
            ));
        }
    }
    out
}

/// 生效值的机器可读形态。
pub fn show_config_json(res: &Resolution, all: bool) -> String {
    let items: Vec<serde_json::Value> = KNOWN_KNOBS
        .iter()
        .filter_map(|k| {
            let r = res.get(k.name)?;
            if !all && k.tier != Tier::Public && r.ignored.is_empty() && r.raw.is_none() {
                return None;
            }
            Some(serde_json::json!({
                "key":     if k.toml_key.is_empty() { k.name } else { k.toml_key },
                "env":     k.name,
                "value":   r.raw,
                "source":  r.source.label(),
                "default": k.default_hint,
                "ignored": r.ignored.iter().map(|ig| serde_json::json!({
                    "layer":  ig.layer.label(),
                    "value":  ig.value,
                    "reason": ignore_note(&ig.reason),
                })).collect::<Vec<_>>(),
            }))
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({ "knobs": items }))
        .unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}
