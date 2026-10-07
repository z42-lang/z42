//! Shared `ObjNew` **target resolution** for the interpreter (`exec_object::obj_new`) and
//! the JIT helper (`jit/helpers/object.rs::jit_obj_new`): which class descriptor to
//! allocate and which constructor to run. Each engine only decides how to run the ctor.
//!
//! P1-2 PR 5: the site cache (`ResolvedTokens::obj_new`, [`ObjNewSite`]) stores resolved
//! identities — the class `Arc<TypeDesc>` and the ctor's `FnId` (same id space as `Call`
//! tokens: entry-module functions `0..n`, lazily loaded functions after that). After the
//! first allocation at a site, neither the class name nor the ctor name is hashed and no
//! lock is taken; a class / ctor in a lazily loaded package caches exactly like an
//! entry-module one. Before, every `new` hashed the class name (module registry, then the
//! loader), the ctor name (`func_index`, then the loader's read lock + hash for a
//! cross-package ctor; the JIT a third time through its slot lookup by name).
//!
//! The cache is only used when the function runs against the VM's entry module
//! ([`site_for`]) — every production run. Against another module (unit tests on a bare
//! `VmContext`) every allocation resolves by name, as before.
//!
//! Order of the checks is unchanged: class (registry → loader → definite-missing /
//! fallback descriptor → base-merged copy) → ambiguous type → missing base; then the
//! caller runs the package / type initializer barriers; then the ctor (module → loader,
//! arity check on first binding, definite-missing ctor). The ambiguous-type check and the
//! initializer barriers run on every allocation — cache hit or not.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::metadata::resolver::{ctorless_hit, ctorless_note, ObjNewSite};
use crate::metadata::tokens::UNRESOLVED;
use crate::metadata::{Function, Module, TypeDesc, Value};
use crate::vm_context::symres;
use crate::vm_context::VmContext;

use super::exec_call::{fn_by_id, lazy_call_id};

/// The site cache to use for `module`: `site` when `module` is the VM's entry module (the
/// id space `ctor` is stored in), else `None` (resolve by name, cache nothing).
#[inline]
pub(crate) fn site_for<'s>(
    ctx: &VmContext, module: &Module, site: Option<&'s ObjNewSite>,
) -> Option<&'s ObjNewSite> {
    site.filter(|_| ctx.funcs().is_entry(module))
}

/// The class to allocate. `Err` carries the exception to throw (missing type, ambiguous
/// type, missing base). Hit: the cached descriptor, no hashing.
#[inline]
pub(crate) fn resolve_class(
    ctx: &VmContext, module: &Module, class_name: &str, site: Option<&ObjNewSite>,
) -> Result<Arc<TypeDesc>, Value> {
    if let Some(td) = site.and_then(|s| s.class.get()) {
        // runtime-ambiguous-use-site: a second package declaring the class may load at any
        // time, so this stays per allocation (one relaxed load while none was ever seen).
        if let Some(exc) = symres::ambiguous_type_exception(ctx, module, class_name) {
            return Err(exc);
        }
        return Ok(Arc::clone(td));
    }
    resolve_class_slow(ctx, module, class_name, site)
}

#[inline(never)]
fn resolve_class_slow(
    ctx: &VmContext, module: &Module, class_name: &str, site: Option<&ObjNewSite>,
) -> Result<Arc<TypeDesc>, Value> {
    // L3-G4d: for imported classes (e.g. Std.Collections.Stack) the TypeDesc may only exist
    // in the lazy loader until first use; probe it before falling back to a blank
    // synthetic descriptor.
    let resolved = module.type_registry.get(class_name).cloned()
        .or_else(|| ctx.try_lookup_type(class_name));
    let (type_desc, cacheable) = match resolved {
        Some(td) => (td, true),
        None => {
            // 站点 ② fix-silent-symbol-resolution：合成空描述符是**静默数据损坏**的温床（没有
            // 字段槽 ⇒ 构造器的 FieldSet 被丢弃、后续 FieldGet 全读 Null）。`missing_type_exception`
            // 判定确定不存在就抛；回落描述符只留给它的正当用途（合并模块带 ClassDesc / 编译器
            // 合成的本地类）。回落描述符每次现建，不进缓存（与此前行为一致）。
            if let Some(exc) = symres::missing_type_exception(ctx, module, class_name) {
                return Err(exc);
            }
            (Arc::new(super::dispatch::make_fallback_type_desc(module, class_name)), false)
        }
    };
    // fix-crosspkg-base-fields-in-eager-module：急切加载的主模块注册表在**构建期**合不进跨包
    // 基类（那时依赖还没加载），这份描述符「只有自己的字段」；惰性加载器那份被
    // `try_fixup_inheritance` 补齐过（`Arc::make_mut` 写时复制只让惰性注册表拿到修好的副本）。
    // 旗子在手，这里换成修好的那份。
    let type_desc = if type_desc.base_unmerged() {
        match ctx.try_lookup_type(class_name) {
            Some(fixed) if !fixed.base_unmerged() => fixed,
            _ => type_desc,
        }
    } else { type_desc };
    // runtime-ambiguous-use-site：**用**一个被两个包各自声明的类型 → 报错。
    if let Some(exc) = symres::ambiguous_type_exception(ctx, module, class_name) {
        return Err(exc);
    }
    // 站点 ④ fix-silent-symbol-resolution：惰性解析都走完了还是残缺 ⇒ 基类确定不存在。
    if let Some(exc) = symres::missing_base_exception(ctx, module, &type_desc) {
        return Err(exc);
    }
    // Only a complete descriptor the registries hold is cached: a fallback one is rebuilt
    // per allocation as before, and an unmerged one may still be fixed up in place.
    if cacheable && !type_desc.base_unmerged() {
        if let Some(s) = site {
            let _ = s.class.set(Arc::clone(&type_desc));
        }
    }
    Ok(type_desc)
}

/// The constructor to run: `Ok(Some((ctor, id)))`, or `Ok(None)` for a class without one
/// (the object stays default-initialised). `id` is the ctor's `FnId` under `module` (its
/// JIT slot id); `None` only for a lazily loaded ctor when `module` is not the entry
/// module. `phys_argc` counts `this`. `Err` carries the exception to throw.
///
/// Hit: the cached `FnId` read back lock-free (`fn_by_id`), or a still-valid ctorless
/// mark. The arity check ran when the id was cached and is not repeated (as for `Call`).
#[inline]
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_ctor<'a>(
    ctx: &'a VmContext, module: &'a Module,
    class_name: &str, ctor_name: &str, phys_argc: usize, ctor_known: bool,
    site: Option<&ObjNewSite>,
    holder: &'a mut Option<Arc<Function>>,
) -> Result<Option<(&'a Function, Option<usize>)>, Value> {
    if let Some(s) = site {
        let id = s.ctor.load(Ordering::Relaxed);
        if id != UNRESOLVED {
            if let Some(f) = fn_by_id(ctx, module, id) {
                return Ok(Some((f, Some(id as usize))));
            }
        } else if ctorless_hit(Some(&s.ctorless), ctx.fn_registration_mark()) {
            // cache-ctorless-objnew: this site proved the ctor resolves nowhere (module and
            // loader) and nothing has been registered since. A ctor in the module would have
            // been cached above instead, so the module need not be probed first.
            return Ok(None);
        }
    }
    bind_ctor(ctx, module, class_name, ctor_name, phys_argc, ctor_known, site, holder)
}

/// The cold half of [`resolve_ctor`]: bind `ctor_name` by name — the module's `func_index`
/// first (TypeChecker already overload-resolved it; no name inference), then the lazy
/// loader (L3-G4d: a stdlib / cross-package ctor) — check the arity, cache the `FnId`.
#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn bind_ctor<'a>(
    ctx: &'a VmContext, module: &'a Module,
    class_name: &str, ctor_name: &str, phys_argc: usize, ctor_known: bool,
    site: Option<&ObjNewSite>,
    holder: &'a mut Option<Arc<Function>>,
) -> Result<Option<(&'a Function, Option<usize>)>, Value> {
    let mark = ctx.fn_registration_mark();
    if let Some(f) = module.func_index.get(ctor_name).and_then(|&i| module.functions.get(i).map(|f| (i, f))) {
        let (idx, f) = f;
        // 站点 ⑤ fix-ctor-arity-skew：解析**成功**也要查——裸键在版本 skew 下会命中**错的**
        // 构造器（单构造器必是 primary、必占裸键），而建帧不做 arity 校验。不符不缓存。
        if let Some(exc) = symres::wrong_arity_exception(
            ctx, module, ctor_name, symres::call_arity(f), phys_argc,
        ) {
            return Err(exc);
        }
        if let Some(s) = site { s.ctor.store(idx as u32, Ordering::Relaxed); }
        return Ok(Some((f, Some(idx))));
    }
    match ctx.try_lookup_function(ctor_name) {
        Some(lazy) => {
            // 站点 ⑤：惰性加载来的构造器同样查（跨包构造器正是 skew 的主战场）。
            if let Some(exc) = symres::wrong_arity_exception(
                ctx, module, ctor_name, symres::call_arity(lazy.as_ref()), phys_argc,
            ) {
                return Err(exc);
            }
            let id = lazy_call_id(ctx, module, &lazy);
            if let (Some(s), Some(id)) = (site, id) { s.ctor.store(id as u32, Ordering::Relaxed); }
            Ok(Some((&**holder.insert(lazy), id)))
        }
        None => {
            // 站点 ③ fix-silent-symbol-resolution：带实参却解析不到构造器 = 定案缺失，不能把
            // 「未经构造」的对象写进 dst（字段全零值，错误现场离根因十万八千里）。
            if let Some(exc) = symres::missing_ctor_exception(
                ctx, module, class_name, ctor_name, phys_argc - 1, ctor_known,
            ) {
                return Err(exc);
            }
            // Mark read **before** the resolve: a registration racing in makes it stale.
            ctorless_note(site.map(|s| &s.ctorless), mark);
            Ok(None)
        }
    }
}

#[cfg(test)]
#[path = "obj_new_resolve_tests.rs"]
mod obj_new_resolve_tests;
