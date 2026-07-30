//! Load-context builtins — backing `Std.Runtime.LoadContext`,
//! `Std.Reflection.Assembly`, and the `Std.Type` collectibility surface
//! (add-load-context-model, 2026-07-30).
//!
//! Mechanism:
//!   - `LoadContext` / `Assembly` z42 objects carry a `ContextId` / `AssemblyId`
//!     in `NativeData` (like `Std.Type`'s `TypeHandle`). The builtins here read
//!     that handle and consult `VmCore.context_registry`.
//!   - `Std.Type.IsCollectible` / `.Assembly` resolve via the Type object's
//!     `__asmId` slot (Null / 0 ⇒ root ⇒ not collectible). `Assembly.GetTypes`
//!     stamps `__asmId` on each Type object it builds; `typeof(T)` leaves it
//!     Null (root). This keeps `TypeDesc` unmutated (design D5).
//!
//! Phase 1: boundary + reflection identity only. No unload (that throws z42-side
//! `NotSupportedException`), no cross-context execution.

use anyhow::{bail, Result};

use super::reflection::make_type_object;
use crate::metadata::context::{AssemblyId, ContextId};
use crate::metadata::types::NativeData;
use crate::metadata::Value;
use crate::vm_context::VmContext;

const STD_LOADCONTEXT: &str = "Std.Runtime.LoadContext";
const STD_ASSEMBLY: &str = "Std.Reflection.Assembly";
const ASM_ID_SLOT: &str = "__asmId";

// ── object builders ─────────────────────────────────────────────────────────

/// Allocate a native-handle-backed z42 object (no data slots written; the class
/// exposes everything via `[Native]` methods).
fn alloc_native(ctx: &VmContext, type_name: &str, native: NativeData) -> Result<Value> {
    let td = ctx.try_lookup_type(type_name).ok_or_else(|| {
        anyhow::anyhow!("load-context: {type_name} not loaded (z42.core missing?)")
    })?;
    let slots = vec![Value::Null; td.fields.len()];
    Ok(ctx.heap().alloc_object(td, slots, native))
}

fn build_loadcontext(ctx: &VmContext, cid: ContextId) -> Result<Value> {
    alloc_native(ctx, STD_LOADCONTEXT, NativeData::LoadContextHandle(cid))
}

fn build_assembly(ctx: &VmContext, aid: AssemblyId) -> Result<Value> {
    alloc_native(ctx, STD_ASSEMBLY, NativeData::AssemblyHandle(aid))
}

// ── handle / arg readers ────────────────────────────────────────────────────

fn ctx_handle(args: &[Value]) -> Result<ContextId> {
    match args.first() {
        Some(Value::Object(rc)) => match &rc.borrow().native {
            NativeData::LoadContextHandle(cid) => Ok(*cid),
            _ => bail!("expected a Std.Runtime.LoadContext receiver"),
        },
        _ => bail!("expected a Std.Runtime.LoadContext receiver"),
    }
}

fn asm_handle(args: &[Value]) -> Result<AssemblyId> {
    match args.first() {
        Some(Value::Object(rc)) => match &rc.borrow().native {
            NativeData::AssemblyHandle(aid) => Ok(*aid),
            _ => bail!("expected a Std.Reflection.Assembly receiver"),
        },
        _ => bail!("expected a Std.Reflection.Assembly receiver"),
    }
}

/// Read the `__asmId` slot off a `Std.Type` object. Null / absent ⇒ root
/// (`typeof(T)` for a root type never stamps it).
fn type_asm_id(args: &[Value]) -> AssemblyId {
    if let Some(Value::Object(rc)) = args.first() {
        let o = rc.borrow();
        if let Some(&i) = o.type_desc.field_index.get(ASM_ID_SLOT) {
            if let Some(Value::I64(v)) = o.slots.get(i) {
                return AssemblyId(*v as u32);
            }
        }
    }
    AssemblyId::ROOT
}

/// Stamp the `__asmId` slot on a freshly-built `Std.Type` object so its
/// `IsCollectible` / `Assembly` resolve to this assembly.
fn set_type_asm_id(tv: &Value, aid: AssemblyId) {
    if let Value::Object(rc) = tv {
        let mut o = rc.borrow_mut();
        let idx = o.type_desc.field_index.get(ASM_ID_SLOT).copied();
        if let Some(i) = idx {
            o.slots[i] = Value::I64(aid.0 as i64);
        }
    }
}

fn str_arg(args: &[Value], i: usize) -> Result<&str> {
    match args.get(i) {
        Some(Value::Str(s)) => Ok(s.as_ref()),
        _ => bail!("load-context builtin: expected a string argument at position {i}"),
    }
}

/// Derive an assembly's logical name from a zpkg/zbc path (basename, no extension).
fn assembly_name_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}

// ── LoadContext builtins ─────────────────────────────────────────────────────

/// `LoadContext.Default` (static) → the永驻 root context.
pub fn builtin_lctx_default(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    build_loadcontext(ctx, ContextId::ROOT)
}

/// `LoadContext.CreateCollectible(string name)` (static) → a new collectible context.
pub fn builtin_lctx_create_collectible(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let name = str_arg(args, 0)?;
    let cid = ctx.core.context_registry.lock().create_collectible(name);
    build_loadcontext(ctx, cid)
}

/// `ctx.Load(string zpkgPath)` (instance) → the loaded `Assembly`.
/// Phase 1: loads the zpkg into the context's private arena, reflection-visible;
/// the assembly's functions are NOT yet cross-context executable.
pub fn builtin_lctx_load(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let cid = ctx_handle(args)?;
    let path = str_arg(args, 1)?;
    let artifact = crate::metadata::loader::load_artifact(path)
        .map_err(|e| anyhow::anyhow!("LoadContext.Load(\"{path}\"): {e}"))?;
    let name = assembly_name_from_path(path);
    let aid = ctx
        .core
        .context_registry
        .lock()
        .load_into(cid, &name, artifact.module);
    build_assembly(ctx, aid)
}

/// `ctx.Name` (instance getter).
pub fn builtin_lctx_name(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let cid = ctx_handle(args)?;
    let name = ctx
        .core
        .context_registry
        .lock()
        .context_name(cid)
        .unwrap_or_default();
    Ok(Value::Str(name.into()))
}

/// `ctx.IsCollectible` (instance getter).
pub fn builtin_lctx_is_collectible(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let cid = ctx_handle(args)?;
    let b = ctx
        .core
        .context_registry
        .lock()
        .context_is_collectible(cid)
        .unwrap_or(false);
    Ok(Value::Bool(b))
}

/// `ctx.GetAssemblies()` (instance) → the assemblies loaded into this context.
pub fn builtin_lctx_assemblies(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let cid = ctx_handle(args)?;
    let aids = ctx.core.context_registry.lock().context_assemblies(cid);
    let mut out = Vec::with_capacity(aids.len());
    for aid in aids {
        out.push(build_assembly(ctx, aid)?);
    }
    Ok(ctx.heap().alloc_array(out))
}

// ── Assembly builtins ────────────────────────────────────────────────────────

/// `asm.Name` (instance getter).
pub fn builtin_asm_name(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let aid = asm_handle(args)?;
    let name = ctx
        .core
        .context_registry
        .lock()
        .assembly_name(aid)
        .unwrap_or_default();
    Ok(Value::Str(name.into()))
}

/// `asm.IsCollectible` (instance getter).
pub fn builtin_asm_is_collectible(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let aid = asm_handle(args)?;
    let b = ctx.core.context_registry.lock().assembly_is_collectible(aid);
    Ok(Value::Bool(b))
}

/// `asm.LoadContext` (instance getter) → the owning context.
pub fn builtin_asm_loadcontext(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let aid = asm_handle(args)?;
    let cid = ctx
        .core
        .context_registry
        .lock()
        .assembly_context(aid)
        .unwrap_or(ContextId::ROOT);
    build_loadcontext(ctx, cid)
}

/// `asm.GetTypes()` (instance) → the types defined by this assembly, each
/// stamped with its `__asmId` so `IsCollectible` / `Assembly` resolve.
pub fn builtin_asm_get_types(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let aid = asm_handle(args)?;
    let tds = ctx.core.context_registry.lock().assembly_types(aid);
    let mut out = Vec::with_capacity(tds.len());
    for td in tds {
        let tv = make_type_object(ctx, td);
        set_type_asm_id(&tv, aid);
        out.push(tv);
    }
    Ok(ctx.heap().alloc_array(out))
}

// ── Type collectibility builtins ─────────────────────────────────────────────

/// `Type.IsCollectible` (instance getter).
pub fn builtin_type_is_collectible(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let aid = type_asm_id(args);
    let b = ctx.core.context_registry.lock().assembly_is_collectible(aid);
    Ok(Value::Bool(b))
}

/// `Type.Assembly` (instance getter) → the assembly that defines this type.
pub fn builtin_type_assembly(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let aid = type_asm_id(args);
    build_assembly(ctx, aid)
}

#[cfg(test)]
#[path = "loadcontext_tests.rs"]
mod loadcontext_tests;
