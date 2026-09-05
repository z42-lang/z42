//! 配置文件层的加载：用户配置（`Z42_CONFIG`）与应用侧车（`Z42_APP_CONFIG`）。
//!
//! # 两个文件层，不是一个
//!
//! 它们格式相同、解析器相同，区别只在**谁写的**：L3 是用户手写（"我这台机器上
//! 想改这个"），L4 由 build 生成、随产物分发（"这个应用需要这样跑"）。所以它们
//! **逐 key 叠加**（用户赢），而不是二选一。
//!
//! 修的是一个真实缺陷：launcher 从前用 `Z42_CONFIG` 一个通道同时表达两者，且只在
//! 用户没设时才塞侧车路径——于是用户一旦 `export Z42_CONFIG=my.toml`，应用自带的
//! `<app>.runtimeconfig.toml` 就被**整份丢弃**，而不是被逐 key 覆盖。
//!
//! # 只有 TOML 一种格式
//!
//! 不引入 JSON、也不为 JSON 留抽象层：unify-run-modes 已经裁决过把 .NET 风格的
//! JSON 侧车收编成 TOML（D5），z42 全仓配置格式已统一（`z42.toml` manifest /
//! `~/.z42/config.toml` / `.runtimeconfig.toml`）。为一个已被否决的方向留
//! `ConfigSource` trait 是付抽象税。唯一的关照是 `.json` 路径给一行**迁移提示**
//! 错误，而不是让从 .NET 迁来的人对着一个被静默忽略的文件 debug 半小时。
//!
//! complete-runtime-settings P4（2026-09-05）：自 config.rs 迁出并泛化为按路径读。

use std::path::Path;

/// 读一个运行配置文件的 `[runtime]` 表。
///
/// - 文件不存在 → `Ok(None)` + 一行 warn（不致命：env / 默认仍然适用）。
/// - `.json` 后缀 → `Err`（迁移提示；**不**静默忽略）。
/// - 非法 TOML，或 `runtime` 存在但不是表 → `Err`（显式，绝不静默降级为默认）。
/// - 合法但没有 `[runtime]` 段 → `Ok(None)`。
pub fn load_config_file(path: &Path, var: &str) -> Result<Option<toml::Table>, String> {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("app");
        return Err(format!(
            "{var}={} — z42 runtime config is TOML, not JSON.\n     \
             Use {stem}.toml with a `[runtime]` table (e.g. `[runtime]\\ngc-mode = \"concurrent\"`); \
             see docs/book/src/runtime/runtime-settings.md.",
            path.display()
        ));
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // eprintln (not tracing) — this runs before the subscriber is installed.
            eprintln!("z42: {var}={} not found; ignoring that config layer", path.display());
            return Ok(None);
        }
        Err(e) => return Err(format!("{var}={}: {e}", path.display())),
    };
    let doc: toml::Table = toml::from_str(&text)
        .map_err(|e| format!("{var}={}: invalid TOML: {e}", path.display()))?;
    match doc.get("runtime") {
        Some(toml::Value::Table(t)) => Ok(Some(t.clone())),
        Some(_) => Err(format!("{var}={}: [runtime] must be a table", path.display())),
        None => Ok(None),
    }
}

/// 读 `var` 命名的配置文件层。变量未设 / 空 → `Ok(None)`（该层不存在）。
pub fn load_layer<F>(get: &F, var: &str) -> Result<Option<toml::Table>, String>
where
    F: Fn(&str) -> Option<String>,
{
    let Some(path) = get(var).filter(|s| !s.trim().is_empty()) else {
        return Ok(None);
    };
    load_config_file(Path::new(path.trim()), var)
}

/// 用户配置层（`Z42_CONFIG`）。历史名字，保留给既有调用方。
pub fn load_runtime_toml<F>(get: F) -> Result<Option<toml::Table>, String>
where
    F: Fn(&str) -> Option<String>,
{
    load_layer(&get, "Z42_CONFIG")
}

/// 应用侧车层（`Z42_APP_CONFIG`）——由 build 生成、launcher 传入。
pub fn load_app_config<F>(get: &F) -> Result<Option<toml::Table>, String>
where
    F: Fn(&str) -> Option<String>,
{
    load_layer(get, "Z42_APP_CONFIG")
}
