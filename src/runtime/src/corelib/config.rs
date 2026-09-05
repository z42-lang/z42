//! `Std.Runtime.RuntimeConfig` 的 builtin —— z42 脚本**只读**查询运行时设置。
//!
//! # 为什么只读
//!
//! 1. 配置在 `OnceLock` 里，boot 后物理不可变。加 setter 就要换成 `RwLock`，给
//!    每个热路径读（`safepoint_throttle` 在每次 safepoint 都读）加锁开销——为一个
//!    边缘能力惩罚主路径。
//! 2. 多数旋钮**只在 boot 期被消费一次**（`Z42_LIBS` 定位、`Z42_SAMPLE_HZ` 决定
//!    是否起采样线程、`Z42_GC_MODE` 决定建哪种堆）。运行中改它们要么无效、要么
//!    要重建子系统；"能设但不生效"比"不能设"更坏——.NET 的 `AppContext.SetSwitch`
//!    正是这个坑（组件早把值缓存了）。
//! 3. 真正需要运行期可调的能力（触发 GC、调堆上限）有专门 API（`Std.GC`）。
//!
//! # 为什么返回扁平 `string[]`
//!
//! z42 当前没有稳定的 `Map<string,string>` marshal 通路；`Environment
//! .GetEnvironmentVariables()` 已经确立了"扁平数组 + 调用方切分"的约定，这里沿用。
//!
//! complete-runtime-settings P4（2026-09-05）。

use crate::config::{is_available, knob_by_key, runtime_config, BuildCtx, KNOWN_KNOBS};
use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{anyhow, Result};

fn arg_str(args: &[Value], what: &str) -> Result<String> {
    match args.first() {
        Some(Value::Str(s)) => Ok(s.to_string()),
        _ => Err(anyhow!("{what} expects a string argument")),
    }
}

/// 旋钮的对外 key：普通旋钮是 `toml_key`，元旋钮没有 key 形式，用 env 名。
fn public_key(spec: &crate::config::KnobSpec) -> &'static str {
    if spec.toml_key.is_empty() { spec.name } else { spec.toml_key }
}

fn find(key: &str) -> Option<&'static crate::config::KnobSpec> {
    knob_by_key(key).or_else(|| KNOWN_KNOBS.iter().find(|k| k.name == key))
}

/// `__cfg_get(string key) -> string?` —— 分层解析后的生效值；取默认时返回 null。
pub fn builtin_cfg_get(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let key = arg_str(args, "RuntimeConfig.Get")?;
    let Some(spec) = find(&key) else { return Ok(Value::Null) };
    let cfg = runtime_config();
    Ok(cfg
        .resolved
        .iter()
        .find(|r| r.name == spec.name)
        .and_then(|r| r.raw.clone())
        .map_or(Value::Null, |v| Value::Str(v.into())))
}

/// `__cfg_source(string key) -> string` —— `"cli"|"env"|"user-config"|"app-config"|"default"`。
/// 未知 key 返回 `"unknown"`。
pub fn builtin_cfg_source(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let key = arg_str(args, "RuntimeConfig.Source")?;
    let Some(spec) = find(&key) else { return Ok(Value::Str("unknown".to_string().into())) };
    let label = runtime_config()
        .resolved
        .iter()
        .find(|r| r.name == spec.name)
        .map_or("default", |r| r.source.label());
    Ok(Value::Str(label.to_string().into()))
}

/// `__cfg_names() -> string[]` —— 全部旋钮的对外 key。
pub fn builtin_cfg_names(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    let names: Vec<Value> = KNOWN_KNOBS
        .iter()
        .map(|k| Value::Str(public_key(k).to_string().into()))
        .collect();
    Ok(ctx.heap().alloc_array(names))
}

/// `__cfg_dump() -> string[]` —— `"key=value|source"` 扁平条目。value 为空表示取默认。
/// 调用方按**第一个** `=` 与**最后一个** `|` 切分。
pub fn builtin_cfg_dump(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    let cfg = runtime_config();
    let entries: Vec<Value> = KNOWN_KNOBS
        .iter()
        .map(|k| {
            let r = cfg.resolved.iter().find(|r| r.name == k.name);
            let value = r.and_then(|r| r.raw.as_deref()).unwrap_or("");
            let source = r.map_or("default", |r| r.source.label());
            Value::Str(format!("{}={value}|{source}", public_key(k)).into())
        })
        .collect();
    Ok(ctx.heap().alloc_array(entries))
}

/// `__cfg_describe(string key) -> string?` —— 一行说明；未知 key 返回 null。
pub fn builtin_cfg_describe(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let key = arg_str(args, "RuntimeConfig.Describe")?;
    Ok(find(&key).map_or(Value::Null, |s| Value::Str(s.description.to_string().into())))
}

/// `__cfg_available(string key) -> bool` —— 该旋钮在**本 build / 本平台**是否存在。
/// 未知 key 返回 false。
pub fn builtin_cfg_available(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let key = arg_str(args, "RuntimeConfig.IsAvailable")?;
    let ctx = BuildCtx::current();
    Ok(Value::Bool(find(&key).is_some_and(|s| is_available(s, &ctx))))
}
