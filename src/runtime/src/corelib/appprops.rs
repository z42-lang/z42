//! `Std.Runtime.AppProperties` 的 builtin —— 应用自定义配置属性，只读。
//!
//! 这些**不是运行时旋钮**：VM 不认识它们的含义、不校验、未知键不是错误。它们来自
//! app 侧车的 `[properties]` 段，供 app 自己读（对照 .NET 的
//! `runtimeOptions.configProperties` + `AppContext.GetData`）。与 `RuntimeConfig`
//! 分开是因为两者的**保证**完全不同：旋钮有登记表、类型、可用性判定、诊断；属性就是
//! 一张原样搬运的表。混进一个 API，"返回 null"会同时意味着"取默认值"和"这个旋钮压根
//! 不存在"，调用方无法区分。
//!
//! # 完整 TOML 类型怎么支持
//!
//! [`builtin_app_prop`] 只查**顶层标量**——覆盖 90% 的场景，且 app 不需要依赖
//! `z42.toml`。数组 / 嵌套表走 [`builtin_app_props_toml`]：把整段 `[properties]`
//! 重新序列化成 TOML 文本交给脚本，脚本用现成的 `Std.Toml` 解析。
//!
//! 这样"完整类型"是**零新增 ABI** 得到的——TOML 有什么就支持什么，将来不需要为新的
//! 值类型再扩展一次 marshal。发明结构化 ABI 或路径 mini-language 成本高、表达力还更差。
//!
//! add-app-properties（2026-09-05）。

use crate::config::runtime_config;
use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{anyhow, Result};

fn arg_str(args: &[Value], what: &str) -> Result<String> {
    match args.first() {
        Some(Value::Str(s)) => Ok(s.to_string()),
        _ => Err(anyhow!("{what} expects a string argument")),
    }
}

fn table() -> Option<&'static toml::Table> {
    runtime_config().app_properties.as_ref()
}

/// 顶层标量渲染成字符串。非标量（数组 / 表）→ `None`：那些走 `Raw()` + `Std.Toml`。
fn scalar(v: &toml::Value) -> Option<String> {
    match v {
        toml::Value::String(s) => Some(s.clone()),
        toml::Value::Integer(i) => Some(i.to_string()),
        toml::Value::Float(f) => Some(f.to_string()),
        toml::Value::Boolean(b) => Some(b.to_string()),
        toml::Value::Datetime(d) => Some(d.to_string()),
        _ => None,
    }
}

/// `__app_prop(string key) -> string?` —— 顶层标量属性。不存在 / 非标量 → null。
pub fn builtin_app_prop(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let key = arg_str(args, "AppProperties.Get")?;
    Ok(table()
        .and_then(|t| t.get(&key))
        .and_then(scalar)
        .map_or(Value::Null, |s| Value::Str(s.into())))
}

/// `__app_prop_has(string key) -> bool` —— 顶层是否存在该键（含非标量值）。
pub fn builtin_app_prop_has(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let key = arg_str(args, "AppProperties.Has")?;
    Ok(Value::Bool(table().is_some_and(|t| t.contains_key(&key))))
}

/// `__app_prop_names() -> string[]` —— 全部顶层键。
pub fn builtin_app_prop_names(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    let names: Vec<Value> = table()
        .map(|t| t.keys().map(|k| Value::Str(k.clone().into())).collect())
        .unwrap_or_default();
    Ok(ctx.heap().alloc_array(names))
}

/// `__app_props_toml() -> string?` —— 整段 `[properties]` 的 TOML 文本。
/// 没有属性时返回 null（而不是空串），让调用方能区分"没有"与"空表"。
pub fn builtin_app_props_toml(_ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    Ok(table().map_or(Value::Null, |t| {
        // 重新序列化而不是保留原始切片——避免为了几个属性把整份侧车文本一直持有。
        Value::Str(toml::to_string(t).unwrap_or_default().into())
    }))
}
