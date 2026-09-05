//! 分层解析：把四个输入层 + 内置默认压成每旋钮一条 [`ResolvedKnob`]，
//! **同时产出 provenance**（这个值来自哪一层、哪些层的值被丢了、为什么）。
//!
//! # 为什么 provenance 要在解析时产出，而不是渲染时重算
//!
//! `--info` 从前是自己再 `std::env::var(knob.name)` 判一遍来源的——于是"优先级链"
//! 有两份实现，任何改动都要改两处且可能漂移。让解析器一次产出、渲染器纯读，
//! 优先级只有一份真相；这也是 `__cfg_source` 能诚实回答的前提。
//!
//! # 严重度分层（design.md Decision 3）
//!
//! 本模块只**记录**问题（[`Diagnostic`]），不决定它是 warn 还是 error：
//! - **CLI 层问题始终致命**——用户此刻、在这台机器上手敲的，静默忽略一个 typo
//!   会让他以为设置生效了。
//! - **env / 两个文件层只 warn**——它们跨机器与跨 build 传播（CI 全局 export、
//!   容器镜像 ENV、随产物分发的侧车）。若也致命，一个 export 了
//!   `Z42_JIT_PROFILE=1` 的 CI 环境会让所有 interp-only 二进制起不来：用可用性
//!   检查换来一场可用性事故。
//! - `--strict-config` / `Z42_STRICT_CONFIG=1` 把后者升级为 error（CI 漂移门）。
//!
//! # 范围校验的归属
//!
//! 这里只验**类型**（能不能解析成声明的 `ValueKind` / 在不在 Enum 集合里）。
//! **取值范围留给各 `parse_*`**——它们历史上是「越界钳制到边界」而非「拒绝」
//! （`Z42_GC_SOFT_THRESHOLD=1.5` → 1.0，有单测明确断言 "not rejected"）。
//! 在这里拒绝会把钳制变成回落默认，是行为回归。
//!
//! complete-runtime-settings P1（2026-09-05）。

use super::knobs::*;
use super::*;
use std::collections::BTreeMap;

/// 一个层的值为什么没成为生效值。
#[derive(Debug, Clone, PartialEq)]
pub enum IgnoreReason {
    /// 被更高优先级的层覆盖。**不产生诊断**——这是分层链的正常工作，不是问题。
    Overridden,
    /// 可用性四轴不满足（层 / build / feature / 平台）。
    Unavailable(Rejection),
    /// 无法解析成该旋钮声明的类型（范围越界不算——那由 parser 钳制）。
    Invalid(String),
}

/// 某一层给出的、但没有生效的值。
#[derive(Debug, Clone, PartialEq)]
pub struct IgnoredValue {
    pub layer: Layer,
    pub value: String,
    pub reason: IgnoreReason,
}

/// 一个旋钮的解析结果 + 完整 provenance。
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedKnob {
    /// 指向 `KNOWN_KNOBS` 的静态名。
    pub name: &'static str,
    /// 生效的原始字符串。`None` = 所有输入层都没给出可用值，取内置默认。
    pub raw: Option<String>,
    /// 生效值来自哪一层（`raw == None` 时为 [`Layer::Default`]）。
    pub source: Layer,
    /// 被丢弃的值——`--show-config` 用它回答「我明明设了，为什么没生效」。
    pub ignored: Vec<IgnoredValue>,
}

/// 解析期发现的一个问题。严重度由 [`Resolution::into_result`] 按层决定。
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub layer: Layer,
    pub message: String,
}

/// 一次完整解析的产物。
#[derive(Debug, Clone, Default)]
pub struct Resolution {
    pub knobs: Vec<ResolvedKnob>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Resolution {
    pub fn get(&self, name: &str) -> Option<&ResolvedKnob> {
        self.knobs.iter().find(|k| k.name == name)
    }

    /// 把诊断按严重度分派：CLI 层（或 `strict`）→ 汇成致命错误文本；
    /// 其余 → 逐条 `eprintln!` 后返回 `Ok(())`。
    ///
    /// 用 `eprintln!` 而非 `tracing`：这一步跑在 subscriber 装好**之前**
    /// （`Z42_LOG` 本身就是被解析的旋钮之一），与既有的 `parse_*` 警告一致。
    pub fn into_result(&self, strict: bool) -> Result<(), String> {
        let (fatal, warn): (Vec<&Diagnostic>, Vec<&Diagnostic>) = self
            .diagnostics
            .iter()
            .partition(|d| d.layer == Layer::Cli || strict);
        for d in warn {
            eprintln!("{}", d.message);
        }
        if fatal.is_empty() {
            return Ok(());
        }
        let mut out: Vec<String> = fatal.iter().map(|d| d.message.clone()).collect();
        if strict && fatal.iter().all(|d| d.layer != Layer::Cli) {
            out.push(
                "z42: (strict config mode — set via --strict-config / Z42_STRICT_CONFIG; \
                 unset it to downgrade these to warnings)"
                    .to_string(),
            );
        }
        Err(out.join("\n"))
    }
}

/// 除 env 外的输入层。env 走泛型 getter（测试注入假 map，避免 `set_var` 全局竞争）。
#[derive(Debug, Default, Clone)]
pub struct Inputs<'a> {
    /// `--set` 的结果，**按旋钮的环境变量名**索引（key→旋钮的解析在 `main.rs`，
    /// 那里才能给出带最近邻建议的报错）。
    pub cli: BTreeMap<&'static str, String>,
    /// `Z42_CONFIG` 指向的用户配置的 `[runtime]` 表。
    pub user_config: Option<&'a toml::Table>,
    /// `Z42_APP_CONFIG` 指向的应用侧车的 `[runtime]` 表。
    pub app_config: Option<&'a toml::Table>,
}

/// 跑一遍分层解析。
pub fn resolve_knobs<F>(get: &F, inputs: &Inputs, ctx: &BuildCtx) -> Resolution
where
    F: Fn(&str) -> Option<String>,
{
    let mut out = Resolution::default();
    for spec in KNOWN_KNOBS {
        let mut chosen: Option<(Layer, String)> = None;
        let mut ignored: Vec<IgnoredValue> = Vec::new();
        // 每个旋钮最多**一条**诊断——最高层的那个问题。若两层都设了同一个在本
        // build 不存在的旋钮，"requires feature `jit`" 刷两遍毫无新信息；被压过
        // 的问题仍然进 `ignored`，`--show-config` 一次全看得到。
        let mut diagnosed = false;

        for &layer in Layer::INPUT_ORDER {
            let Some(raw) = read_layer(spec, layer, get, inputs) else { continue };
            // 更高层已定 → 一律 Overridden，不再做可用性/类型判定，也不诊断：
            // 对一个反正用不上的值报"不可用"是纯噪音。
            if chosen.is_some() {
                ignored.push(IgnoredValue { layer, value: raw, reason: IgnoreReason::Overridden });
                continue;
            }
            if let Err(rej) = evaluate(spec, layer, ctx) {
                if !diagnosed {
                    diagnosed = true;
                    out.diagnostics.push(Diagnostic { layer, message: rej.render(spec, layer, ctx) });
                }
                ignored.push(IgnoredValue { layer, value: raw, reason: IgnoreReason::Unavailable(rej) });
                continue;
            }
            if let Err(why) = validate(spec.value, &raw) {
                if !diagnosed {
                    diagnosed = true;
                    out.diagnostics.push(Diagnostic {
                        layer,
                        message: format!(
                            "z42: knob `{}` ({}, from [{}]) got {raw:?}: {why}.\n     -> value ignored; using default ({}).",
                            display_key(spec), spec.name, layer.label(), spec.default_hint
                        ),
                    });
                }
                ignored.push(IgnoredValue { layer, value: raw, reason: IgnoreReason::Invalid(why) });
                continue;
            }
            chosen = Some((layer, raw));
        }

        let (source, raw) = match chosen {
            Some((l, v)) => (l, Some(v)),
            None => (Layer::Default, None),
        };
        out.knobs.push(ResolvedKnob { name: spec.name, raw, source, ignored });
    }
    out
}

/// 读某一层给该旋钮的原始值。空串 / 纯空白视为该层未设（与既有 env 语义一致，
/// 也让 `--set gc-mode=` 成为「显式清空、回落下一层」）。
fn read_layer<F>(spec: &KnobSpec, layer: Layer, get: &F, inputs: &Inputs) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    let raw = match layer {
        Layer::Cli => inputs.cli.get(spec.name).cloned(),
        Layer::Env => get(spec.name),
        Layer::UserConfig => table_value(inputs.user_config, spec),
        Layer::AppConfig => table_value(inputs.app_config, spec),
        Layer::Default => None,
    }?;
    (!raw.trim().is_empty()).then_some(raw)
}

fn table_value(table: Option<&toml::Table>, spec: &KnobSpec) -> Option<String> {
    if spec.toml_key.is_empty() {
        return None; // 元旋钮不是配置文件里的值键
    }
    table?.get(spec.toml_key).and_then(toml_scalar_to_string)
}

fn display_key(spec: &KnobSpec) -> &'static str {
    if spec.toml_key.is_empty() { spec.name } else { spec.toml_key }
}

/// **类型**校验（范围不管——见模块文档）。
pub fn validate(kind: ValueKind, raw: &str) -> Result<(), String> {
    let v = raw.trim();
    match kind {
        ValueKind::Bool => parse_bool(v)
            .map(|_| ())
            .ok_or_else(|| "expected a boolean (true/false, 1/0, yes/no, on/off)".to_string()),
        ValueKind::Int { .. } => v
            .parse::<i64>()
            .map(|_| ())
            .map_err(|_| "expected an integer".to_string()),
        ValueKind::Float { .. } => v
            .parse::<f64>()
            .map(|_| ())
            .map_err(|_| "expected a number".to_string()),
        // Flag: presence is the value — any string (including "0") enables.
        ValueKind::Flag | ValueKind::Str | ValueKind::Path | ValueKind::PathList => Ok(()),
        ValueKind::Enum(allowed) => allowed
            .contains(&v)
            .then_some(())
            .ok_or_else(|| format!("expected one of: {}", allowed.join(", "))),
    }
}

/// 布尔旋钮的取值。宽松（TOML 写 `true`、shell 习惯写 `1`、人写 `on`）但**封闭**
/// ——表外的字符串是 `Invalid`，不是"非空即真"。
pub fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

// 未知 key 检测**只做 z42vm 拥有命名空间的那两层**：`--set` 的 key（P2）与
// `[runtime]` 表的 key。**不扫环境变量**——`Z42_` 前缀是全生态共享的：launcher
// 用 `Z42_HOME` / `Z42_PORTABLE_VM` / `Z42_TOOLCHAIN`，测试框架用 `Z42_TEST_*` /
// `Z42_VM_JOBS`，嵌入方还会有自己的。VM 对着这些喊"未知旋钮"是纯误报，而且是
// 每次 `z42 run` 都喊。要抓 `Z42_GC_MOD=` 这类 typo，靠的是把值写进 `[runtime]`
// 或 `--set`（那两层里每个 key 都必须是旋钮）。

/// `[runtime]` 表里不认识的 key。
pub fn unknown_table_keys(table: &toml::Table) -> Vec<String> {
    let mut out: Vec<String> = table
        .keys()
        .filter(|k| knob_by_key(k).is_none())
        .cloned()
        .collect();
    out.sort();
    out
}

/// 把未知 key 渲成诊断（严重度同样按层：CLI 层的未知 key 在 `main.rs` 直接报错）。
pub fn unknown_key_diagnostic(layer: Layer, key: &str) -> Diagnostic {
    Diagnostic {
        layer,
        message: format!(
            "z42: unknown runtime knob `{key}` from [{}] — not in the knob registry; ignored.\n     \
             Run `z42vm --list-knobs --all` to see every knob.",
            layer.label()
        ),
    }
}
