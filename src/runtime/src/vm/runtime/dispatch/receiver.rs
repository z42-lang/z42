//! Receiver identities and primitive type predicates shared by all engines.
use crate::object::Value;

/// L3-G4b primitive-as-struct: maps a primitive `Value` variant to its stdlib
/// struct's qualified class name (e.g. `Value::I64` → `"Std.Int32"`). The VM
/// dispatches primitive method calls by constructing `{class}.{method}` and
/// looking up the function in `module.func_index` — replacing the old
/// hardcoded `(Value, method)` → builtin-name switch.
///
/// Returns None for non-primitive values (objects, null, etc.).
pub(crate) fn primitive_class_name(obj: &Value) -> Option<&'static str> {
    use crate::metadata::index::well_known_names::*;
    match obj {
        // ⚠️ These are derived from the *runtime* `Value`, which cannot tell a `ulong` from a
        // `long` from an `int` (all `Value::I64`), nor a `float` from a `double` (both `F64`).
        // So an instance method declared on Std.{SByte,Int16,Byte,UInt16,UInt32,UInt64,Int64}
        // or Std.Single is **never reached through this path** — it lands on Int32/Double's
        // same-named method instead.
        //
        // fix-narrow-prim-instance-dispatch : the compiler now emits a **static
        // Call** to the wrapper's method for exactly those 8 receiver types (primitives are
        // sealed value types — there is no virtual dispatch to preserve), so they reach their
        // own implementations. This table stays as the fallback for everything else.
        //
        // The previous note here claimed "narrow int / long values are tagged with class FQN
        // at compile-time in VCall instructions" — that was never true: `vcall()` takes no
        // receiver-type argument. It read as if the gap were covered, which is why the two
        // live symptoms (UInt64.ToString printing negatives above i64::MAX, Single.GetHashCode
        // silently using Double's fold) went unnoticed for months.
        Value::I64(_)  => Some(STD_INT32),
        Value::F64(_)  => Some(STD_DOUBLE),
        Value::Bool(_) => Some(STD_BOOLEAN),
        Value::Char(_) => Some(STD_CHAR),
        Value::Str(_)  => Some(STD_STRING),
        // add-array-base-class: T[] dispatches to Std.Array methods
        // (Clone / GetType / ToString / Equals / GetHashCode). The lookup path
        // tries `Std.Array.<method>` first, then falls through to base
        // `Std.Object.<method>` via the primitive overload retry logic.
        Value::Array(_) => Some(STD_ARRAY),
        _ => None,
    }
}

/// refactor-vcall-ic-primitives : synthetic TypeId for IC keying.
/// Primitives don't have a real `TypeDesc.id` (they're built-in runtime values,
/// not user-defined classes). Returning a stable `PRIM_TYPE_*` lets `VCallIC`
/// cache them with the same `cached_type_id` mechanism used for object
/// receivers — no extra slot, no separate cache path.
///
/// Returns None for objects (which use real `type_desc.id.0`) and `Value::Null`.
#[inline]
pub(crate) fn value_synthetic_type_id(obj: &Value) -> Option<u32> {
    use crate::vm::runtime::ids::tokens::*;
    match obj {
        Value::I64(_)   => Some(PRIM_TYPE_I64),
        Value::F64(_)   => Some(PRIM_TYPE_F64),
        Value::Bool(_)  => Some(PRIM_TYPE_BOOL),
        Value::Char(_)  => Some(PRIM_TYPE_CHAR),
        Value::Str(_)   => Some(PRIM_TYPE_STR),
        Value::Array(_) => Some(PRIM_TYPE_ARRAY),
        _ => None,
    }
}

/// fix-boxed-primitive-is-as: 基元值是否 is-a `class_name`。z42 不装箱基元，故 `object o = "hi"`
/// 里 o 仍是裸 `Value::Str` —— `is`/`as` 须按其 stdlib 类名（`primitive_class_name`，如
/// `Std.String`）匹配，外加 `Std.Object` 基类（所有基元 is-a object）。编译器 `QualifyTypeName`
/// 发 FQ 形（`Std.String`/`Std.Int32`/`Std.Object`），此处直接比 FQ。
pub(crate) fn prim_isa(val: &Value, class_name: &str) -> bool {
    // 所有基元 is-a object。
    if class_name == "Std.Object" || class_name == "Object" {
        return primitive_class_name(val).is_some();
    }
    match val {
        // 整数宽度不可辨：z42 运行时用单一 Value::I64 表示 int/long/short/byte/…（boxed 后
        // 无宽度信息），故一个 boxed 整数 is-a **任意整数类型**——值的声明类型永不假阴，
        // 跨宽度松匹配是该表示的必然（`9L is long` / `(byte)7 is byte` 才不会误判 false）。
        Value::I64(_) => is_integer_class(class_name),
        // 非整数基元：精确匹配其 stdlib 类名（string/double/bool/char）。
        other => primitive_class_name(other) == Some(class_name),
    }
}

/// 整数族类名（编译器 QualifyTypeName 发 FQ 形；带非限定形兜底）。
pub(crate) fn is_integer_class(cn: &str) -> bool {
    matches!(cn,
        "Std.Int32" | "Std.Int64" | "Std.Int16" | "Std.SByte"
        | "Std.Byte" | "Std.UInt16" | "Std.UInt32" | "Std.UInt64"
        | "Int32" | "Int64" | "Int16" | "SByte"
        | "Byte" | "UInt16" | "UInt32" | "UInt64")
}

