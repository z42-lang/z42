use crate::object::{NativeData, TypeDesc, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Result};
use crate::corelib::abi::BuiltinError;
use std::sync::Arc;

// ── Object protocol ───────────────────────────────────────────────────────────

/// Returns a `Std.Type` for the runtime class of the argument.
///
/// add-reflection-mvp: user objects now carry the real
/// `Arc<TypeDesc>` in `NativeData::TypeHandle`, so the returned Type responds to
/// `GetFields()` / `GetMethods()` / `BaseType` / `GetGenericArguments()`.
/// Primitives / arrays / strings get a handle-less `Std.Type` (member queries
/// reflect empty) — element-typed array reflection deferred (reflection.md).
pub fn obj_get_type(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> {
    obj_get_type_impl(ctx, this).map_err(BuiltinError::Legacy)
}

fn obj_get_type_impl(ctx: &VmContext, this: &Value) -> Result<Value> {
    use crate::corelib::reflection::make_type_from_name;
    match this {
        // A generic instance is either an instantiation descriptor (`Demo.Box<int>`: a generic
        // instantiated in its own package — the name carries the arguments) or an erased
        // definition descriptor plus per-instance arguments (an imported generic: `Std.Collections.List`
        // + `[int]`). Both answer with the same Type — the one `typeof` builds from the canonical
        // name (`instance_type`), so `typeof(Box<int>) == o.GetType()` and a lib-made and a
        // main-made `GBox<int>` report equal Types.
        Value::Object(rc) => {
            let td = rc.type_desc_arc().clone();
            Ok(instance_type(ctx, td, rc.type_args()))
        }
        Value::Array(rc) => {
            // add-reflection-array-element-type: the array value carries its
            // element type (non-erased). `<elem>[]` routes through
            // make_type_from_name's `[]` path → array Type with IsArray + element.
            // Empty element ("") → IsArray true, GetElementType() null.
            let element = rc.borrow().element_type().to_string();
            Ok(make_type_from_name(ctx, &format!("{element}[]")))
        }
        Value::Str(_) => {
            Ok(make_type_from_name(ctx, crate::metadata::index::well_known_names::STD_STRING))
        }
        // add-struct-object-boxing → unify Phase 2 R3: 装箱值类型（struct 或基元）→ 其**精确类型**
        // （type_desc.name：struct 类型 / 基元 wrapper 如 Std.Int64——两者统一为 BoxedStruct）。
        Value::BoxedStruct(b) => Ok(instance_type(ctx, b.type_desc_arc().clone(), b.type_args())),
        // 未装箱裸基元 → canonical stdlib 类（I64→Std.Int32；装箱后走上面精确类）。
        v @ (Value::I64(_) | Value::F64(_) | Value::Bool(_) | Value::Char(_)) => {
            match crate::interp::primitive_class_name(v) {
                Some(cn) => Ok(make_type_from_name(ctx, cn)),
                None => bail!("__obj_get_type: expected an object"),
            }
        }
        Value::Null => bail!("__obj_get_type: null reference"),
        _ => bail!("__obj_get_type: expected an object"),
    }
}

/// The `Std.Type` of a heap instance with descriptor `td` and per-instance type arguments
/// `args`: see `obj_get_type_impl`.
fn instance_type(ctx: &VmContext, td: std::sync::Arc<crate::object::TypeDesc>, args: &[String]) -> Value {
    use crate::corelib::reflection::{make_type_from_name, make_type_object};
    if args.is_empty() || !td.type_args().is_empty() {
        make_type_object(ctx, td)
    } else {
        make_type_from_name(ctx, &crate::metadata::index::type_names::constructed(&td.name, args))
    }
}

/// Reference equality: true iff both arguments point to the same heap allocation,
/// or both are null.
pub fn obj_ref_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => crate::object::ObjRef::ptr_eq(a, b),
        (Value::Array(a),  Value::Array(b))  => crate::object::ArrRef::ptr_eq(a, b),
        (Value::Str(a),    Value::Str(b))    => std::ptr::eq(a.as_ptr(), b.as_ptr()),
        (Value::Null, Value::Null)           => true,
        (Value::Null, _) | (_, Value::Null) => false,
        _                                  => false,
    }
}

/// expose-weak-ref-builtin (D-1a)：把 GC 的 `make_weak` 暴露给
/// stdlib。Object/Array 弱化返回 WeakHandle（包装 NativeData::WeakRef 的
/// ScriptObject）；原子值（I64 / Str / Bool / Char / FuncRef / Closure 等）
/// 不可弱化，返回 Null。
pub fn obj_make_weak(ctx: &VmContext, target: &Value) -> Value {
    let target = target;
    let Some(weak) = ctx.heap().make_weak(target) else {
        return Value::Null;
    };
    let type_desc = weak_handle_type_desc();
    ctx.heap().alloc_object(type_desc, Vec::new(), NativeData::WeakRef(weak))
}

/// D-1b: 提取 delegate 的 captured target（receiver 对象）。
/// 用于 `Std.WeakRef<TD>` 在 ctor 时拿到 target 后做 `MakeWeak` —— 让多播
/// 订阅器只持 listener 的弱引用，避免 lapsed listener 内存泄漏。
///
/// 语义（lenient，与 `__obj_make_weak` 同款）：
/// - `Closure { env: [first, ...] }` 当 `first is Object` → 返回该 Object
///   （instance method 组转换的 thunk 把 receiver 放在 env[0]，D-1b Phase 1）
/// - `Closure { env: [] }` / `env[0]` 非 Object（如基础类型 / Null）→ Null
/// - `FuncRef` → Null（free function 无 receiver）
/// - 非 delegate / Null → Null
pub fn delegate_target(delegate: &Value) -> Value {
    match delegate {
        Value::Closure(c) => {
            let env_ref = crate::object::closure_data_of(c).env.borrow();
            match env_ref.first() {
                Some(Value::Object(o)) => Value::Object(o.clone()),
                Some(Value::Array(a)) => Value::Array(a.clone()),
                _ => Value::Null,
            }
        }
        _ => Value::Null,
    }
}

/// D-1b: 提取 Closure 的 fn_name（thunk 函数全限定名）。
/// P1-2 PR 8：值里存的是目标 `FnId`，名字从 id 指向的 `Function` 取，每次新分配一个 GC 串
/// （冷路径：`WeakRef` 构造时调一次）。
/// 与 `__delegate_target` 配套使用 —— `Std.WeakRef<TD>` 不强持原 handler
/// （那会反向锁住 receiver），而是存 `(WeakHandle, fnName)`，Get 时用
/// `__make_closure(fnName, [upgradedTarget])` 重建一个新 Closure。
///
/// 语义：
/// - `Closure` / `FuncRef` → 目标函数的全限定名
/// - 其他 → Null
pub fn delegate_fn_name(ctx: &VmContext, delegate: &Value) -> Result<Value, BuiltinError> {
    let Some(id) = delegate.callable_id() else { return Ok(Value::Null) };
    match ctx.callable_fn(id) {
        Some(f) => Ok(Value::Str(ctx.heap().alloc_str(&f.name))),
        // A callable's id is bound by this VM: an unregistered one is a broken handle.
        None => Err(BuiltinError::Internal(format!(
            "__delegate_fn_name: function id {} is not registered in this VM", id.0))),
    }
}

/// D-1b: 用给定 fn_name + env 数组构造 heap-allocated Closure。
/// 与 `__delegate_target` / `__delegate_fn_name` 配套，让 `Std.WeakRef<TD>.Get()`
/// 在 receiver 仍存活时重建一个等价于原 handler 的 Closure。
///
/// 语义：
/// - `fn_name`: Value::Str
/// - `env`: Value::Array (env Vec<Value>)
/// - 非 string / 非 array / Null → Null（lenient）
/// - P1-2 PR 8：名字在这里解析成 `FnId`（与 `MkClos` 同一条 `bind_fn_ref`），解析不到就抛
///   （`MissingSymbolException`，与 `MkClos` 相同）
pub fn make_closure(ctx: &VmContext, fn_name: &Value, env: &Value) -> Result<Value, BuiltinError> {
    let Value::Str(fn_name) = fn_name else { return Ok(Value::Null) };
    let Value::Array(env_arr) = env else { return Ok(Value::Null) };
    let Some(module) = ctx.module() else {
        return Err(BuiltinError::Internal(format!(
            "__make_closure: no entry module to resolve `{}` against", &**fn_name)));
    };
    let fn_id = crate::interp::fn_ref_id(ctx, module, fn_name, None).map_err(BuiltinError::Thrown)?;
    // 把 Array 里的元素拷贝到 Vec<Value>，再走 heap.alloc_array 拿到新的 ArrRef。
    // 与 jit_mk_clos 同款路径：env 是 GC 管理的 Array → Closure 持其 ArrRef。
    let env_vec: Vec<Value> = env_arr.borrow().iter_boxed().collect();
    let Value::Array(env) = crate::object::ArrayObj::alloc_frozen(ctx.heap(), &env_vec) else {
        return Ok(Value::Null);  // unreachable but lenient
    };
    // unify-gc-heap PR-2: the ClosureData lives in the GC variable-length region.
    Ok(ctx.heap().alloc_closure(crate::object::ClosureData { env, fn_id }))
}

/// expose-weak-ref-builtin (D-1a)：升格 WeakHandle 弱引用。
/// 目标存活返回原 Object/Array；目标已被回收 / 输入非 WeakHandle / Null →
/// 返回 Null（lenient 处理，与 `__delegate_eq` 同款）。
pub fn obj_upgrade_weak(ctx: &VmContext, handle: &Value) -> Value {
    let Value::Object(o) = handle else {
        return Value::Null;
    };
    let weak = {
        let obj = o.borrow();
        match &obj.native() {
            NativeData::WeakRef(w) => w.clone(),
            _ => return Value::Null,
        }
    };
    ctx.heap().upgrade_weak(&weak).unwrap_or(Value::Null)
}

/// WeakHandle 的 TypeDesc 单例（slots 为空；仅 NativeData::WeakRef 携带状态）。
fn weak_handle_type_desc() -> Arc<TypeDesc> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Arc<TypeDesc>> = OnceLock::new();
    CACHE.get_or_init(|| Arc::new(TypeDesc {
        name: "Std.WeakHandle".to_string(),
        base_name: None,
        class_flags: 0,
        visibility: 0,
        fields: Vec::new(),
        field_index: crate::metadata::NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(),
        cold: None,
        id: crate::vm::runtime::ids::tokens::TypeId::UNRESOLVED,
    })).clone()
}

/// fix-delegate-reference-equality (D-5)：delegate reference
/// equality —— 两个 `Value` 变体（FuncRef / Closure）按
/// 各自身份语义比较。跨变体不等，非 delegate 值返回 false 不报错。
///
/// 语义参见 `delegates-events.md` 与本 spec design.md：
/// - `FuncRef(id)` —— 目标 `FnId` 相等（P1-2 PR 8 起；一个函数一个 id，等价于此前的名字相等）
/// - `Closure { env, fn_id }` —— fn_id 相等且 env ArrRef::ptr_eq
pub fn delegate_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        // fix-delegate-equality-operator: FuncRef / Closure identity now lives in
        // `Value`'s `PartialEq` (the `==` operator reaches the same rule), so defer to it
        // rather than keeping a second copy here.
        (Value::FuncRef(_), Value::FuncRef(_)) => a == b,
        (Value::Closure(_), Value::Closure(_)) => a == b,
        (Value::Null, Value::Null)   => true,
        _                            => false,
    }
}

/// Identity hash of a heap address: non-negative 31 bits, stable because the
/// GC never moves objects. The address itself is a poor hash — allocations
/// are aligned, so its low bits are constant, and `Dictionary` picks the
/// bucket from the low bits (`h & mask`). The 64-bit finalizer of MurmurHash3
/// (`fmix64`) lets every address bit reach every result bit: a single Fibonacci
/// multiply kept only the top bits of the product, whose lowest few barely move
/// when consecutive blocks sit a small power of two apart (16 / 32 B — what the
/// smallest objects and arrays are).
pub(crate) fn identity_hash(addr: usize) -> i64 {
    let mut x = addr as u64;
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^= x >> 33;
    (x & 0x7fff_ffff) as i64
}

/// Identity-based hash code derived from the object's address.
pub fn obj_hash_code(this: &Value) -> Result<i64, BuiltinError> {
    obj_hash_code_impl(this)
}

fn obj_hash_code_impl(this: &Value) -> Result<i64, BuiltinError> {
    match this {
        Value::Object(gc) => Ok(identity_hash(crate::object::ObjRef::as_ptr(gc) as *const _ as *const u8 as usize)),
        // add-array-base-class: identity hash for Value::Array
        Value::Array(gc) => Ok(identity_hash(crate::object::ArrRef::as_ptr(gc) as *const _ as *const u8 as usize)),
        Value::Null => Ok(0),
        other => Err(BuiltinError::Internal(format!(
            "__obj_hash_code: expected an object, got {other:?}"))),
    }
}

/// Value equality — defaults to reference equality.
/// args: [this, other]
pub fn obj_equals(this: &Value, other: &Value) -> bool {
    let result = match (this, other) {
        (Value::Object(a), Value::Object(b)) => crate::object::ObjRef::ptr_eq(a, b),
        // add-array-base-class: reference-equality for arrays (mirror Object)
        (Value::Array(a),  Value::Array(b))  => crate::object::ArrRef::ptr_eq(a, b),
        (Value::Null, Value::Null)           => true,
        (Value::Null, _) | (_, Value::Null) => false,
        _                                  => false,
    };
    result
}

/// Human-readable representation — returns the unqualified type name.
/// args: [this]
pub fn obj_to_str(this: &Value) -> Result<String, BuiltinError> {
    match this {
        Value::Object(rc) => {
            let td_name = &rc.type_desc().name;
            Ok(crate::metadata::index::type_names::unqualified(td_name).to_string())
        }
        // add-array-base-class: arrays ToString returns simple class name
        // ("Array"), matching Object protocol default. Element-typed ToString
        // (`int[]` etc.) is deferred to expand-type-metadata.
        Value::Array(_) => Ok("Array".to_string()),
        Value::Null => Ok("null".to_string()),
        other => Err(BuiltinError::Internal(format!(
            "__obj_to_str: expected an object, got {other:?}"))),
    }
}

// wave1-assert-script: 6 `builtin_assert_*` functions removed.
// `Std.Assert` is now pure z42 script in `z42.core/src/Assert.z42`.

#[cfg(test)]
#[path = "object_tests.rs"]
mod object_tests;

/// Object 协议迁 builtin ABI v2(P1-8 批次 11)。`GetType` 是纯合成;`ref_eq`
/// / `hash_code` / `equals` / `to_str` 是 Rust 默认(子类 override 走 z42 vtable,
/// 不经这里)——全部 PURE。委托协议(`DelegateOps`,批次 12b):`eq` / `target` 只读委托值
/// (PURE);`fn_name` / `make_closure` 分配 GC 串 / 闭包,`make_closure` 绑定失败时抛
/// `MkClos` 同款的 `MissingSymbolException`(`Thrown` 原样传播)。
#[z42_macros::builtins]
pub mod typed {
    use super::*;

    #[builtin(name = "__obj_get_type", this = "GetType")]
    pub fn get_type(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> {
        obj_get_type(ctx, this)
    }

    #[builtin(name = "__obj_ref_eq", pure)]
    pub fn ref_eq(a: &Value, b: &Value) -> bool { obj_ref_eq(a, b) }

    #[builtin(name = "__obj_hash_code", this = "GetHashCode", pure)]
    pub fn hash_code(this: &Value) -> Result<i64, BuiltinError> { obj_hash_code(this) }

    #[builtin(name = "__obj_equals", this = "Equals", pure)]
    pub fn equals(this: &Value, other: &Value) -> bool { obj_equals(this, other) }

    #[builtin(name = "__obj_to_str", this = "ToString")]
    pub fn to_str(this: &Value) -> Result<String, BuiltinError> { obj_to_str(this) }

    #[builtin(name = "__obj_make_weak")]
    pub fn make_weak(ctx: &VmContext, target: &Value) -> Value { obj_make_weak(ctx, target) }

    #[builtin(name = "__obj_upgrade_weak")]
    pub fn upgrade_weak(ctx: &VmContext, handle: &Value) -> Value { obj_upgrade_weak(ctx, handle) }

    #[builtin(name = "__delegate_eq", pure)]
    pub fn delegate_eq(a: &Value, b: &Value) -> bool { super::delegate_eq(a, b) }

    #[builtin(name = "__delegate_target", pure)]
    pub fn delegate_target(delegate: &Value) -> Value { super::delegate_target(delegate) }

    #[builtin(name = "__delegate_fn_name")]
    pub fn delegate_fn_name(ctx: &VmContext, delegate: &Value) -> Result<Value, BuiltinError> {
        super::delegate_fn_name(ctx, delegate)
    }

    #[builtin(name = "__make_closure")]
    pub fn make_closure(ctx: &VmContext, fn_name: &Value, env: &Value) -> Result<Value, BuiltinError> {
        super::make_closure(ctx, fn_name, env)
    }
}
