//! 旋钮可用性求值 + 拒绝诊断渲染。
//!
//! 一个旋钮值要生效，必须四项全通过：
//!   1. `sources` 允许它来自的那一层
//!   2. `build`（`DebugOnly` 要求 debug build）
//!   3. `requires` 里每个 cargo feature 都编进了本二进制
//!   4. `platforms` 允许当前 OS
//!
//! 任一不满足 → [`Rejection`]，由 [`Rejection::render`] 渲成一段人类可读诊断。
//! **严重度不在这里决定**——CLI 层致命 / 其余 warn 的分层策略属于 `resolve.rs`
//! （complete-runtime-settings design.md Decision 3）。本模块只回答"能不能用、为什么"。
//!
//! CoreCLR 对照：它对 release build 里的 debug-only 旋钮是**完全静默忽略**（宏在
//! `#ifdef _DEBUG` 外根本不生成符号）。z42 选择"忽略但明说"——成本是一行 stderr，
//! 收益是用户不必读源码就知道旋钮为什么不生效。
//!
//! complete-runtime-settings P0（2026-09-05）。

use super::knobs::*;

/// 本二进制**可能**声明的 cargo feature 全集。
///
/// 手写是必须的：feature 名在 `cfg!(feature = "..")` 里只能是编译期字面量，无法
/// 反射。单测 `feature_table_covers_cargo_features` 拿它与 `Cargo.toml [features]`
/// 的期望列表对账，加 feature 忘了同步这里就会红。
pub const KNOWN_FEATURES: &[&str] = &[
    "android",
    "aot",
    "bundled-compression",
    "dhat-heap",
    "interp-only",
    "ios",
    "jit",
    "mimalloc-alloc",
    "native-interop",
    "profile-contention",
    "wasm",
];

/// 某个 cargo feature 是否编进了本二进制。表外的名字一律 `false`（保守：
/// 未知依赖 = 不可用，而不是默默放行）。
pub fn feature_enabled(name: &str) -> bool {
    match name {
        "android" => cfg!(feature = "android"),
        "aot" => cfg!(feature = "aot"),
        "bundled-compression" => cfg!(feature = "bundled-compression"),
        "dhat-heap" => cfg!(feature = "dhat-heap"),
        "interp-only" => cfg!(feature = "interp-only"),
        "ios" => cfg!(feature = "ios"),
        "jit" => cfg!(feature = "jit"),
        "mimalloc-alloc" => cfg!(feature = "mimalloc-alloc"),
        "native-interop" => cfg!(feature = "native-interop"),
        "profile-contention" => cfg!(feature = "profile-contention"),
        "wasm" => cfg!(feature = "wasm"),
        _ => false,
    }
}

/// 当前 OS 标识。**归一化**：wasm 目标下 `std::env::consts::OS` 是 `"unknown"`
/// （wasm32-unknown-unknown）或 `"wasi"`，都不便在旋钮表里书写；这里统一报
/// `"wasm"`，与 `Std.Platform` 的 `OSKind::Wasm` 口径一致。
pub fn current_os() -> &'static str {
    if cfg!(target_arch = "wasm32") {
        "wasm"
    } else {
        std::env::consts::OS
    }
}

/// 求值所处的构建/运行环境。单测注入假值，避免断言随 build preset 漂移。
#[derive(Debug, Clone)]
pub struct BuildCtx {
    /// `cfg!(debug_assertions)`。
    pub debug: bool,
    /// 本二进制启用的 feature（[`KNOWN_FEATURES`] 的子集）。
    pub features: Vec<&'static str>,
    /// 归一化后的 OS（见 [`current_os`]）。
    pub os: &'static str,
}

impl BuildCtx {
    /// 本进程的真实环境。
    pub fn current() -> Self {
        Self {
            debug: cfg!(debug_assertions),
            features: KNOWN_FEATURES
                .iter()
                .copied()
                .filter(|f| feature_enabled(f))
                .collect(),
            os: current_os(),
        }
    }

    fn has_feature(&self, name: &str) -> bool {
        self.features.iter().any(|f| *f == name)
    }

    /// 诊断里"本 build 编译时启用的是：…"那一行。
    fn feature_list(&self) -> String {
        if self.features.is_empty() {
            "(none)".to_string()
        } else {
            self.features.join(", ")
        }
    }
}

/// 一个旋钮值被拒的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    /// 该旋钮不接受来自这一层的设置（`sources` 掩码不含）。
    NotAcceptedFrom { layer: Layer, accepted: LayerMask },
    /// 该旋钮只在 debug build 存在。
    DebugOnly,
    /// 缺少必需的 cargo feature（列出缺的那些）。
    MissingFeatures(Vec<&'static str>),
    /// 当前平台不支持。
    WrongPlatform,
}

/// 判定 `spec` 的一个来自 `layer` 的值在 `ctx` 下能否生效。
pub fn evaluate(spec: &KnobSpec, layer: Layer, ctx: &BuildCtx) -> Result<(), Rejection> {
    if !spec.sources.contains(layer.mask()) {
        return Err(Rejection::NotAcceptedFrom { layer, accepted: spec.sources });
    }
    if spec.build == BuildAvail::DebugOnly && !ctx.debug {
        return Err(Rejection::DebugOnly);
    }
    let missing: Vec<&'static str> = spec
        .requires
        .iter()
        .copied()
        .filter(|f| !ctx.has_feature(f))
        .collect();
    if !missing.is_empty() {
        return Err(Rejection::MissingFeatures(missing));
    }
    if !platform_allows(spec.platforms, ctx.os) {
        return Err(Rejection::WrongPlatform);
    }
    Ok(())
}

/// 该旋钮在 `ctx` 下是否**存在**（忽略来源层——用于 `--list-knobs` 与
/// `RuntimeConfig.IsAvailable`，它们问的是"这个 build 有没有这个旋钮"）。
pub fn is_available(spec: &KnobSpec, ctx: &BuildCtx) -> bool {
    (spec.build != BuildAvail::DebugOnly || ctx.debug)
        && spec.requires.iter().all(|f| ctx.has_feature(f))
        && platform_allows(spec.platforms, ctx.os)
}

fn platform_allows(p: PlatformAvail, os: &str) -> bool {
    match p {
        PlatformAvail::All => true,
        PlatformAvail::Only(list) => list.iter().any(|o| *o == os),
        PlatformAvail::Except(list) => !list.iter().any(|o| *o == os),
    }
}

impl Rejection {
    /// 一行标题 + 缩进详情的诊断文本（grep 友好）。调用方按层的严重度决定它是
    /// `eprintln!` 的 warn 还是 `anyhow` 的致命错误——文本本身相同。
    ///
    /// `fallback` 是被拒后实际会用的值的说明（通常取 `spec.default_hint`）。
    pub fn render(&self, spec: &KnobSpec, layer: Layer, ctx: &BuildCtx) -> String {
        let key = if spec.toml_key.is_empty() { spec.name } else { spec.toml_key };
        let head = format!("z42: knob `{key}` ({}, from [{}])", spec.name, layer.label());
        let detail = match self {
            Rejection::NotAcceptedFrom { accepted, .. } => format!(
                "cannot be set from [{}]; accepted layers: {}.",
                layer.label(),
                accepted.labels().join(", ")
            ),
            Rejection::DebugOnly => {
                "exists only in debug builds; this z42vm is a release build.".to_string()
            }
            Rejection::MissingFeatures(missing) => format!(
                "is unavailable in this build: requires feature{} `{}`; this z42vm was built with: {}.",
                if missing.len() == 1 { "" } else { "s" },
                missing.join("`, `"),
                ctx.feature_list()
            ),
            Rejection::WrongPlatform => format!(
                "is unavailable on this platform ({}); supported: {}.",
                ctx.os,
                platform_label(spec.platforms)
            ),
        };
        format!(
            "{head} {detail}\n     -> value ignored; using default ({}).\n     Run `z42vm --list-knobs --all` to see every knob's availability.",
            spec.default_hint
        )
    }
}

fn platform_label(p: PlatformAvail) -> String {
    match p {
        PlatformAvail::All => "all platforms".to_string(),
        PlatformAvail::Only(list) => list.join(", "),
        PlatformAvail::Except(list) => format!("all except {}", list.join(", ")),
    }
}
