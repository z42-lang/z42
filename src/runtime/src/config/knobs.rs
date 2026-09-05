//! 旋钮 schema 的**类型层**：`KnobSpec` 及其五个声明式维度
//! （类型 / 可接受输入层 / build profile / feature 依赖 / 平台 / 支持级别）。
//!
//! 实际的旋钮登记表在 [`super::knob_table::KNOWN_KNOBS`]；求值在
//! [`super::availability`]。三者分开是因为登记表是**数据**（每加一个旋钮改一行）、
//! 类型是**词汇表**（很少变）、求值是**逻辑**（要单测注入假环境）。
//!
//! CoreCLR 对照：`inc/clrconfigvalues.h` 用宏前缀在编译期表达同样的分级——
//! `RETAIL_CONFIG_*` vs `CONFIG_*`（对应本文件的 [`BuildAvail`]）、
//! `EXTERNAL_/UNSUPPORTED_/INTERNAL_` 符号前缀（对应 [`Tier`]）。z42 把它们做成
//! **可枚举的字段**而非宏，因为 `--list-knobs --json` 要把这些约束输出给工具消费，
//! 宏在编译后就没了。
//!
//! refactor-split-config（2026-09-03）自 config.rs 搬出；
//! complete-runtime-settings P0（2026-09-05）扩为完整 schema + 表分离到 knob_table.rs。

#![allow(unused_imports)]
use super::*;
use crate::gc::GcMode;
use std::path::PathBuf;
use std::sync::OnceLock;

// ── 输入层 ───────────────────────────────────────────────────────────────────

/// 一个旋钮值的来源层。优先级 `Cli > Env > UserConfig > AppConfig > Default`。
///
/// `UserConfig` = `Z42_CONFIG` 指向的文件（用户手写）；
/// `AppConfig` = `Z42_APP_CONFIG` 指向的应用侧车（build 生成，随产物分发）。
/// 两者格式与解析器相同，区别只在**谁写的**，所以逐 key 叠加而非互斥。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    Cli,
    Env,
    UserConfig,
    AppConfig,
    Default,
}

impl Layer {
    /// 渲染 / `__cfg_source` 用的稳定标签。
    pub const fn label(self) -> &'static str {
        match self {
            Layer::Cli => "cli",
            Layer::Env => "env",
            Layer::UserConfig => "user-config",
            Layer::AppConfig => "app-config",
            Layer::Default => "default",
        }
    }

    /// 该层在 [`LayerMask`] 里的位。`Default` 不参与掩码（它不是"输入"，
    /// 是所有输入都缺席时的兜底），返回空掩码。
    pub const fn mask(self) -> LayerMask {
        match self {
            Layer::Cli => LayerMask::CLI,
            Layer::Env => LayerMask::ENV,
            Layer::UserConfig => LayerMask::USER_CONFIG,
            Layer::AppConfig => LayerMask::APP_CONFIG,
            Layer::Default => LayerMask::NONE,
        }
    }

    /// 按优先级从高到低的四个**输入**层（不含 `Default`）。
    pub const INPUT_ORDER: &'static [Layer] =
        &[Layer::Cli, Layer::Env, Layer::UserConfig, Layer::AppConfig];
}

/// 一个旋钮允许从哪些层设置的位集。
///
/// 不是所有旋钮都该从所有层设置：元旋钮（`Z42_CONFIG` / `Z42_APP_CONFIG` /
/// `Z42_STRICT_CONFIG`）只接受 [`LayerMask::CLI_ENV`]——它们决定**读哪个文件**和
/// **诊断有多严格**，写在配置文件里会自指（一个文件指定要读哪个文件，或把自身
/// 的错误从 error 降成 warn）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerMask(u8);

impl LayerMask {
    pub const NONE: LayerMask = LayerMask(0);
    pub const CLI: LayerMask = LayerMask(1 << 0);
    pub const ENV: LayerMask = LayerMask(1 << 1);
    pub const USER_CONFIG: LayerMask = LayerMask(1 << 2);
    pub const APP_CONFIG: LayerMask = LayerMask(1 << 3);

    /// 四层全收——绝大多数旋钮。
    pub const ALL: LayerMask = LayerMask(0b1111);
    /// 只从命令行 / 环境变量——元旋钮（不能写进它自己指向的文件）。
    pub const CLI_ENV: LayerMask = LayerMask(0b0011);
    /// 只从环境变量——测试脚手架（不进用户 CLI 表面，也不随产物分发）。
    pub const ENV_ONLY: LayerMask = LayerMask(0b0010);
    /// 两个配置文件层。
    pub const FILES: LayerMask = LayerMask(0b1100);

    pub const fn contains(self, other: LayerMask) -> bool {
        self.0 & other.0 == other.0 && other.0 != 0
    }

    /// 渲染用：按优先级顺序列出该掩码允许的层标签。
    pub fn labels(self) -> Vec<&'static str> {
        Layer::INPUT_ORDER
            .iter()
            .filter(|l| self.contains(l.mask()))
            .map(|l| l.label())
            .collect()
    }
}

// ── 值类型 ───────────────────────────────────────────────────────────────────

/// 旋钮值的类型与取值域。用于 CLI/文件层的**输入校验**（越界 / 非法值在解析期
/// 就被记为 `Invalid` 并诊断），以及 `--list-knobs --json` 的 schema 输出。
///
/// 注意：这里的范围是**声明**，各 `parse_*` 函数仍持有自己的钳制/回落逻辑
/// （历史行为，非破坏）；两者应一致，由单测守。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValueKind {
    Bool,
    /// **存在即启用**：任何值（含 `0` / `false`）都算开。与 [`Bool`] 的区别是
    /// 语义不同、不是宽松版——`Z42_NO_FUSION=0` 今天是**关闭 fusion**，不是开启。
    /// 只用于历史上以 `env::var(..).is_ok()` 实现的旋钮，登记时如实标注。
    ///
    /// [`Bool`]: ValueKind::Bool
    Flag,
    Int { min: i64, max: i64 },
    Float { min: f64, max: f64 },
    Str,
    Path,
    /// 平台分隔符（unix `:` / windows `;`）分隔的路径列表。
    PathList,
    /// 固定取值集合（含别名写法）。
    Enum(&'static [&'static str]),
}

impl ValueKind {
    /// `--list-knobs` 的类型列。
    pub fn label(self) -> String {
        match self {
            ValueKind::Bool => "bool".to_string(),
            ValueKind::Flag => "flag (presence enables)".to_string(),
            ValueKind::Int { min, max } => format!("int[{min}..{max}]"),
            ValueKind::Float { min, max } => format!("float[{min}..{max}]"),
            ValueKind::Str => "string".to_string(),
            ValueKind::Path => "path".to_string(),
            ValueKind::PathList => "path-list".to_string(),
            ValueKind::Enum(vs) => format!("enum({})", vs.join("|")),
        }
    }
}

// ── 可用性三轴 + 支持级别 ────────────────────────────────────────────────────

/// 旋钮在哪种 build profile 下存在。对应 CoreCLR 的 `RETAIL_CONFIG_*`（=[`Always`]）
/// vs 无前缀的 `CONFIG_*`（=[`DebugOnly`]，只在 `#ifdef _DEBUG` 生成）。
///
/// [`Always`]: BuildAvail::Always
/// [`DebugOnly`]: BuildAvail::DebugOnly
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildAvail {
    Always,
    DebugOnly,
}

/// 旋钮在哪些平台上有意义。`Except` 而非只有 `Only`，是因为多数平台约束的自然
/// 表达是"除了 wasm"（采样 profiler 要后台线程、native 扩展要 dlopen）；列正面
/// 清单会在每加一个目标三元组时漏掉。取值用 `std::env::consts::OS` 的字符串。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformAvail {
    All,
    Only(&'static [&'static str]),
    Except(&'static [&'static str]),
}

/// 支持级别。对应 CoreCLR 的 `EXTERNAL_/UNSUPPORTED_/INTERNAL_` 符号前缀。
/// `--list-knobs` 默认只列 [`Public`]；`--all` 才列全部。
///
/// [`Public`]: Tier::Public
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// 公开、有文档、面向所有用户。
    Public,
    /// 可用但不保证稳定——调优旋钮，可能随版本改语义/默认值。
    Unsupported,
    /// 机制内部件 / 测试脚手架 / 占位。默认视图隐藏。
    Internal,
}

impl Tier {
    pub const fn label(self) -> &'static str {
        match self {
            Tier::Public => "public",
            Tier::Unsupported => "unsupported",
            Tier::Internal => "internal",
        }
    }
}

// ── KnobSpec ─────────────────────────────────────────────────────────────────

/// 单个 `Z42_*` 旋钮的完整声明。`KNOWN_KNOBS` 是它的唯一登记处——新增旋钮 =
/// 表里加一行 + 在 `consumed_by` 处读它；CLI / env / 两个文件层 / 查询表面 /
/// 脚本表面全部自动跟随。
#[derive(Debug, Clone, Copy)]
pub struct KnobSpec {
    /// 环境变量名（如 `"Z42_LIBS"`）。表按此字段字母序排列。
    pub name: &'static str,
    /// `[runtime]` 表里的 key，也是 `--set` 的 key——kebab-case、去 `Z42_`
    /// 前缀小写（`Z42_GC_MODE` → `"gc-mode"`）。空串 = 元旋钮，不是配置文件里
    /// 的值键（如 `Z42_CONFIG` 命名文件本身，装不进它自己）。
    pub toml_key: &'static str,
    /// `--set` 额外接受的短名。**首切全空**——不发明短名，也不把 `Z42_*` env
    /// 名当作隐式等价写法（那会在 env 名与 kebab key 将来不同步时产生歧义）。
    pub aliases: &'static [&'static str],
    /// 值类型与取值域。
    pub value: ValueKind,
    /// 允许从哪些层设置。
    pub sources: LayerMask,
    /// 在哪种 build profile 下存在。
    pub build: BuildAvail,
    /// 需要的 cargo feature——**全部**启用才可用。名字须在
    /// [`super::availability::KNOWN_FEATURES`] 内（单测守）。
    pub requires: &'static [&'static str],
    /// 平台约束。
    pub platforms: PlatformAvail,
    /// 支持级别。
    pub tier: Tier,
    /// 一行人类描述，`--info` / `--list-knobs` / `RuntimeConfig.Describe` 共用。
    pub description: &'static str,
    /// 未设时的默认说明（如 `"unset; falls back to ..."`）。
    pub default_hint: &'static str,
    /// 真正消费该旋钮的位置——`src/runtime/src/` 下的文件路径。
    pub consumed_by: &'static str,
}

impl KnobSpec {
    /// `--set` / `[runtime]` 表里指代该旋钮的 key（即 `toml_key`）。元旋钮返回
    /// 空串——它们只能从 CLI/env 设，没有配置文件 key。
    pub const fn cli_key(&self) -> &'static str {
        self.toml_key
    }

    /// `key` 是否指代本旋钮（完整 key 或显式声明的 alias）。
    /// **不**接受 `Z42_*` env 名形式——那是 env 层的写法。
    pub fn matches_key(&self, key: &str) -> bool {
        if !self.toml_key.is_empty() && self.toml_key == key {
            return true;
        }
        self.aliases.iter().any(|a| *a == key)
    }

    /// 元旋钮（命名文件 / 控制诊断严重度本身，不能写进配置文件）。
    pub const fn is_meta(&self) -> bool {
        self.toml_key.is_empty()
    }
}

// ── 表项基线 ─────────────────────────────────────────────────────────────────
//
// `KNOWN_KNOBS` 用 const 函数式记录更新（`..PUBLIC` 等）书写：每条只写偏离基线
// 的字段，读表时一眼看到的就是"这个旋钮哪里特殊"。基线本身即最常见形态。

/// 常规公开旋钮：四层全收、任何 build、无 feature 要求、全平台。
pub(super) const PUBLIC: KnobSpec = KnobSpec {
    name: "",
    toml_key: "",
    aliases: &[],
    value: ValueKind::Str,
    sources: LayerMask::ALL,
    build: BuildAvail::Always,
    requires: &[],
    platforms: PlatformAvail::All,
    tier: Tier::Public,
    description: "",
    default_hint: "",
    consumed_by: "",
};

/// 调优旋钮：同 [`PUBLIC`]，但 `tier = Unsupported`（默认视图隐藏，语义可能随版本变）。
pub(super) const TUNING: KnobSpec = KnobSpec { tier: Tier::Unsupported, ..PUBLIC };

/// 元旋钮：只从 CLI/env 设、`toml_key` 空、`tier = Internal`。
pub(super) const META: KnobSpec = KnobSpec {
    sources: LayerMask::CLI_ENV,
    tier: Tier::Internal,
    ..PUBLIC
};
