//! PIC probes, interface-slot keys and module-local virtual target lookup.
use anyhow::{bail, Result};
use crate::object::Value;
use crate::vm::runtime::{Function, Module};
use crate::vm::runtime::dispatch::ic::{vcall_ic_lookup, VCallIC};
use crate::vm::runtime::ids::tokens::UNRESOLVED;
use crate::vm::runtime::dispatch::receiver::value_synthetic_type_id;

/// Receiver type id used to key the PIC: real `TypeDesc.id` for objects, synthetic id for
/// primitives / arrays; `None` for boxes and null (not cacheable).
#[inline]
pub(crate) fn receiver_type_id(obj_val: &Value) -> Option<u32> {
    match obj_val {
        Value::Object(rc) => Some(rc.type_desc().id.0),
        other => value_synthetic_type_id(other),
    }
}

/// PIC fast path shared by both engines: hit → the callee's `FnId` under the site's module
/// (resolve it with `functions::fn_by_id` / the backend slot table, both lock-free). `None` on
/// miss, on a non-cacheable receiver, or when the cached target is `UNRESOLVED`.
///
/// Publication: the PIC entry is read `Relaxed`. A lazily loaded `FnId` is only usable through
/// `FuncTable::get`, whose Acquire loads (published length, then the slot's function pointer)
/// pair with the registration's Release stores — that, not the IC, orders the slot contents.
/// A reader that sees the id before the registration is visible gets `None` from the table and
/// takes the slow path; it can never read a half-built slot.
#[inline]
pub(crate) fn vcall_ic_hit(ic: Option<&VCallIC>, obj_val: &Value) -> Option<usize> {
    let ic = ic?;
    let recv_type = receiver_type_id(obj_val)?;
    let fn_id = vcall_ic_lookup(ic, recv_type)?;
    if fn_id == UNRESOLVED { return None; }
    Some(fn_id as usize)
}

/// The interface method name of an interface-slot key (`Demo.IFoo<int>#Get` → `Get`);
/// `None` for an ordinary method key. `#` never occurs in a type name or a method key
/// (overload mangling uses `$`, specializations `:`, nested types `+`), so its presence is
/// the whole test.
#[inline]
pub(crate) fn iface_slot_method(method: &str) -> Option<&str> {
    method.rfind('#').map(|i| &method[i + 1..])
}

/// The erased form of an interface-slot key: type arguments dropped from the interface part
/// (`Demo.IFoo<int>#Get` → `Demo.IFoo#Get`). `None` when the key carries none. A class that
/// implements a generic interface whose arguments mention its own type parameters only
/// registers the erased key, and a shared generic body calling through `IFoo<T>` only emits
/// it.
pub(crate) fn erased_iface_slot_key(key: &str) -> Option<String> {
    let hash = key.rfind('#')?;
    let lt = key[..hash].find('<')?;
    Some(format!("{}{}", &key[..lt], &key[hash..]))
}

/// Fallback linear walk used when TypeDesc is missing (e.g. stdlib stubs).
pub fn resolve_virtual<'m>(module: &'m Module, class_name: &str, method: &str) -> Result<&'m Function> {
    let mut cur = class_name;
    loop {
        let qualified = format!("{}.{}", cur, method);
        if let Some(f) = module.func_index.get(qualified.as_str()).and_then(|&i| module.functions.get(i)) {
            return Ok(f);
        }
        match module.classes.iter().find(|c| c.name == cur).and_then(|c| c.base_class.as_deref()) {
            Some(base) => cur = base,
            None => bail!("VCall: no implementation of `{}` in hierarchy of `{}`", method, class_name),
        }
    }
}

/// The method key installed for an interface slot, exact form before erased form.
#[inline]
pub(crate) fn iface_slot_implementation(
    td: &crate::object::TypeDesc, key: &str, erased: Option<&str>,
) -> Option<String> {
    let at = |k: &str| td.vtable_index.get(k).and_then(|&slot| td.vtable.get(slot)).map(|(simple, _)| simple.clone());
    at(key).or_else(|| erased.and_then(at))
}
