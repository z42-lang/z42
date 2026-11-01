//! Load-context builtins — backing `Std.Runtime.AssemblyLoadContext`,
//! `Std.Reflection.Assembly`, and the `Std.Type` collectibility surface
//! (add-load-context-model).
//!
//! Mechanism:
//!   - `AssemblyLoadContext` / `Assembly` z42 objects carry a `ContextId` / `AssemblyId`
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

use super::abi::BuiltinError;

use super::reflection::make_type_object;
use crate::metadata::context::{AssemblyId, ContextId, ContextState, UnloadOutcome};
use crate::object::NativeData;
use crate::object::Value;
use crate::vm_context::VmContext;

const STD_LOADCONTEXT: &str = "Std.Runtime.AssemblyLoadContext";
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
        Some(Value::Object(rc)) => match &rc.borrow().native() {
            NativeData::LoadContextHandle(cid) => Ok(*cid),
            _ => bail!("expected a Std.Runtime.AssemblyLoadContext receiver"),
        },
        _ => bail!("expected a Std.Runtime.AssemblyLoadContext receiver"),
    }
}

fn asm_handle(args: &[Value]) -> Result<AssemblyId> {
    match args.first() {
        Some(Value::Object(rc)) => match &rc.borrow().native() {
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
            if let Value::I64(v) = o.field_value(i) {
                return AssemblyId(v as u32);
            }
        }
    }
    AssemblyId::ROOT
}

/// Stamp the `__asmId` slot on a freshly-built `Std.Type` object so its
/// `IsCollectible` / `Assembly` resolve to this assembly.
fn set_type_asm_id(tv: &Value, aid: AssemblyId) {
    if let Value::Object(rc) = tv {
        let o = rc.borrow();
        let idx = o.type_desc.field_index.get(ASM_ID_SLOT).copied();
        if let Some(i) = idx {
            o.set_field_value(i, &Value::I64(aid.0 as i64));
        }
    }
}

fn ctx_handle_of(this: &Value) -> Result<ContextId, BuiltinError> {
    ctx_handle(std::slice::from_ref(this)).map_err(BuiltinError::Legacy)
}

fn asm_handle_of(this: &Value) -> Result<AssemblyId, BuiltinError> {
    asm_handle(std::slice::from_ref(this)).map_err(BuiltinError::Legacy)
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

// ── AssemblyLoadContext builtins ─────────────────────────────────────────────

/// `AssemblyLoadContext.Default` (static) → the永驻 root context.
pub fn lctx_default(ctx: &VmContext) -> Result<Value, BuiltinError> {
    build_loadcontext(ctx, ContextId::ROOT).map_err(BuiltinError::Legacy)
}

/// `AssemblyLoadContext.CreateCollectible(string name)` (static) → a new collectible context.
pub fn lctx_create_collectible(ctx: &VmContext, name: &crate::object::string::vstr::Str) -> Result<Value, BuiltinError> {
    let name = name.as_ref();
    let cid = ctx.core.context_registry.lock().create_collectible(name);
    build_loadcontext(ctx, cid).map_err(BuiltinError::Legacy)
}

/// `ctx.Load(string zpkgPath)` (instance) → the loaded `Assembly`.
/// Phase 1: loads the zpkg into the context's private arena, reflection-visible;
/// the assembly's functions are NOT yet cross-context executable.
pub fn lctx_load(ctx: &VmContext, this: &Value, path: &crate::object::string::vstr::Str) -> Result<Value, BuiltinError> {
    let cid = ctx_handle_of(this)?;
    let path = path.as_ref();
    // Phase 2: a context being unloaded no longer accepts new assemblies.
    if ctx.core.context_registry.lock().context_state(cid) != Some(ContextState::Active) {
        return Err(BuiltinError::InvalidOperation(
            "AssemblyLoadContext.Load: context is unloading (cannot load new assemblies)".into()));
    }
    let artifact = crate::metadata::loader::load_artifact(&**ctx.vfs(), path)
        .map_err(|e| BuiltinError::Legacy(anyhow::anyhow!("AssemblyLoadContext.Load(\"{path}\"): {e}")))?;
    let name = assembly_name_from_path(path);
    let aid = ctx
        .core
        .context_registry
        .lock()
        .load_into(cid, &name, artifact.module);
    build_assembly(ctx, aid).map_err(BuiltinError::Legacy)
}

/// `ctx.__unload()` (instance) — mark this collectible context `Unloading`
/// (add-lazy-context-unload). Idempotent. Root is filtered in z42
/// (`Unload()` throws `InvalidOperationException` before reaching here);
/// `RootRejected` here is a defensive backstop. Actual arena reclamation is
/// GC-driven (see `ContextRegistry::reclaim`).
pub fn lctx_unload(ctx: &VmContext, this: &Value) -> Result<(), BuiltinError> {
    let cid = ctx_handle_of(this)?;
    match ctx.core.context_registry.lock().unload(cid) {
        UnloadOutcome::Marked | UnloadOutcome::AlreadyUnloading => Ok(()),
        UnloadOutcome::RootRejected => Err(BuiltinError::InvalidOperation(
            "AssemblyLoadContext.Unload: the root context cannot be unloaded".into())),
        UnloadOutcome::NotFound => Err(BuiltinError::InvalidOperation(
            "AssemblyLoadContext.Unload: unknown context".into())),
    }
}

/// `ctx.Name` (instance getter).
pub fn lctx_name(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> {
    let cid = ctx_handle_of(this)?;
    let name = ctx
        .core
        .context_registry
        .lock()
        .context_name(cid)
        .unwrap_or_default();
    Ok(Value::Str(name.into()))
}

/// `ctx.IsCollectible` (instance getter).
pub fn lctx_is_collectible(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> {
    let cid = ctx_handle_of(this)?;
    let b = ctx
        .core
        .context_registry
        .lock()
        .context_is_collectible(cid)
        .unwrap_or(false);
    Ok(Value::Bool(b))
}

/// `ctx.GetAssemblies()` (instance) → the assemblies loaded into this context.
pub fn lctx_assemblies(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> {
    let cid = ctx_handle_of(this)?;
    let aids = ctx.core.context_registry.lock().context_assemblies(cid);
    let mut out = Vec::with_capacity(aids.len());
    for aid in aids {
        out.push(build_assembly(ctx, aid).map_err(BuiltinError::Legacy)?);
    }
    Ok(ctx.heap().alloc_array(out))
}

// ── Assembly builtins ────────────────────────────────────────────────────────

/// `asm.Name` (instance getter).
pub fn asm_name(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> {
    let aid = asm_handle_of(this)?;
    let name = ctx
        .core
        .context_registry
        .lock()
        .assembly_name(aid)
        .unwrap_or_default();
    Ok(Value::Str(name.into()))
}

/// `asm.IsCollectible` (instance getter).
pub fn asm_is_collectible(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> {
    let aid = asm_handle_of(this)?;
    let b = ctx.core.context_registry.lock().assembly_is_collectible(aid);
    Ok(Value::Bool(b))
}

/// `asm.LoadContext` (instance getter) → the owning context.
pub fn asm_loadcontext(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> {
    let aid = asm_handle_of(this)?;
    let cid = ctx
        .core
        .context_registry
        .lock()
        .assembly_context(aid)
        .unwrap_or(ContextId::ROOT);
    build_loadcontext(ctx, cid).map_err(BuiltinError::Legacy)
}

/// `asm.GetTypes()` (instance) → the types defined by this assembly, each
/// stamped with its `__asmId` so `IsCollectible` / `Assembly` resolve.
pub fn asm_get_types(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> {
    let aid = asm_handle_of(this)?;
    let tds = ctx.core.context_registry.lock().assembly_types(aid);
    let mut out = Vec::with_capacity(tds.len());
    for td in tds {
        let tv = make_type_object(ctx, td);
        set_type_asm_id(&tv, aid);
        out.push(tv);
    }
    Ok(ctx.heap().alloc_array(out))
}

/// `Assembly.GetFunctions()` → the assembly's free (top-level) functions as `MethodInfo[]`
/// (sorted by qualified name). Each `MethodInfo` is resolved by qualified name against the
/// live VM, so its attributes / signature / `Invoke` are available once the module is also
/// loaded executable (`Std.Runtime.ModuleLoader.Load`); a reflect-only projection still
/// lists the names.
pub fn asm_get_functions(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> {
    let aid = asm_handle_of(this)?;
    let names = ctx.core.context_registry.lock().assembly_function_names(aid);
    let mut out = Vec::with_capacity(names.len());
    for q in &names {
        let simple = q.rsplit('.').next().unwrap_or(q);
        out.push(super::reflection::build_free_function_info(ctx, simple, q).map_err(BuiltinError::Legacy)?);
    }
    Ok(ctx.heap().alloc_array(out))
}

// ── Type collectibility builtins ─────────────────────────────────────────────

/// `Type.IsCollectible` (instance getter).
pub fn type_is_collectible(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> {
    let aid = type_asm_id(std::slice::from_ref(this));
    let b = ctx.core.context_registry.lock().assembly_is_collectible(aid);
    Ok(Value::Bool(b))
}

/// `Type.Assembly` (instance getter) → the assembly that defines this type.
pub fn type_assembly(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> {
    let aid = type_asm_id(std::slice::from_ref(this));
    build_assembly(ctx, aid).map_err(BuiltinError::Legacy)
}

#[cfg(test)]
#[path = "assemblyloadcontext_tests.rs"]
mod assemblyloadcontext_tests;

/// 装载上下文域迁 builtin ABI v2(P1-8 批次 7):除 `__lctx_load`(装载包,保守
/// `reenters`)外都是纯查询 / 合成(注册表读 + 句柄对象构造),不执行 z42 代码。
#[z42_macros::builtins]
pub mod typed {
    use super::*;

    #[builtin(name = "__lctx_default")]
    pub fn default_ctx(ctx: &VmContext) -> Result<Value, BuiltinError> { lctx_default(ctx) }

    #[builtin(name = "__lctx_create_collectible")]
    pub fn create_collectible(ctx: &VmContext, name: &crate::object::string::vstr::Str) -> Result<Value, BuiltinError> {
        lctx_create_collectible(ctx, name)
    }

    #[builtin(name = "__lctx_load", reenters)]
    pub fn load(ctx: &VmContext, this: &Value, path: &crate::object::string::vstr::Str) -> Result<Value, BuiltinError> {
        lctx_load(ctx, this, path)
    }

    #[builtin(name = "__lctx_unload")]
    pub fn unload(ctx: &VmContext, this: &Value) -> Result<(), BuiltinError> { lctx_unload(ctx, this) }

    #[builtin(name = "__lctx_name")]
    pub fn name(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> { lctx_name(ctx, this) }

    #[builtin(name = "__lctx_is_collectible")]
    pub fn is_collectible(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> {
        lctx_is_collectible(ctx, this)
    }

    #[builtin(name = "__lctx_assemblies")]
    pub fn assemblies(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> {
        lctx_assemblies(ctx, this)
    }

    #[builtin(name = "__asm_name")]
    pub fn asm_name_of(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> { asm_name(ctx, this) }

    #[builtin(name = "__asm_is_collectible")]
    pub fn asm_is_coll(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> { asm_is_collectible(ctx, this) }

    #[builtin(name = "__asm_loadcontext")]
    pub fn asm_lctx(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> { asm_loadcontext(ctx, this) }

    #[builtin(name = "__asm_get_types")]
    pub fn asm_types(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> { asm_get_types(ctx, this) }

    #[builtin(name = "__asm_get_functions")]
    pub fn asm_fns(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> { asm_get_functions(ctx, this) }

    #[builtin(name = "__type_is_collectible")]
    pub fn type_is_coll(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> {
        type_is_collectible(ctx, this)
    }

    #[builtin(name = "__type_assembly")]
    pub fn type_asm(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> { type_assembly(ctx, this) }
}
