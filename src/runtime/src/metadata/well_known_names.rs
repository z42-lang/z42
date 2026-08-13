//! Well-known string constants — qualified stdlib type names + a small set of
//! special builtin / method identifiers consumed in multiple call sites.
//!
//! Centralising these literals lets us rename a stdlib class (e.g.
//! `Std.Int32` → `Std.Primitives.Int32`) by changing one location instead of
//! grep-replacing across `interp/`, `jit/`, `corelib/`.
//!
//! The C# compiler has a counterpart at `z42.IR/WellKnownNames.cs`; both
//! sides should agree on these strings (any change here likely needs a mirror
//! change there).

// ── Qualified stdlib class names ──────────────────────────────────────────
//
// rename-primitives-to-pascal-case (2026-05-24): primitives migrated to BCL
// PascalCase struct names (`Std.Int32` / `Std.Boolean` / `Std.SByte` / ...).
// Source keyword (`int / bool / i8 / ...`) is preserved as an alias resolved
// by the C# TypeChecker via `TypeRegistry.StdlibClassName`.
//
// Narrow integer / unsigned BCL names (`Std.Int16` / `Std.SByte` / `Std.Byte` /
// `Std.UInt16` / `Std.UInt32` / `Std.UInt64`) are NOT registered here — they
// have no `Value` variant to map from (all stored as `Value::I64`) and are
// reached via compile-time-emitted class FQN strings in VCall instructions.

/// Stdlib qualified name for the `int` keyword's BCL struct
/// (`struct Int32 : ...` in z42.core/src/Primitives/Int32.z42).
pub const STD_INT32: &str = "Std.Int32";

/// Stdlib qualified name for the `long` keyword's BCL struct.
pub const STD_INT64: &str = "Std.Int64";

/// Stdlib qualified name for the `double` keyword's BCL struct.
pub const STD_DOUBLE: &str = "Std.Double";

/// Stdlib qualified name for the `float` keyword's BCL struct.
pub const STD_SINGLE: &str = "Std.Single";

/// Stdlib qualified name for the `bool` keyword's BCL struct.
pub const STD_BOOLEAN: &str = "Std.Boolean";

/// Stdlib qualified name for the `char` keyword's BCL struct.
pub const STD_CHAR: &str = "Std.Char";

/// Stdlib qualified name for the `String` primitive class. Note: capitalised
/// because stdlib retains `class String` (lowercase `string` is the source
/// keyword that lexes to this class).
pub const STD_STRING: &str = "Std.String";

/// Root class of the type hierarchy. Every user class implicitly inherits.
pub const STD_OBJECT: &str = "Std.Object";

/// Stdlib's reified-type class returned by `__obj_get_type`.
pub const STD_TYPE: &str = "Std.Type";

/// 2026-05-07 add-array-base-class: runtime base of all `T[]`. `Value::Array`
/// 不携带 TypeDesc 引用，VM 端 `is_instance` / `as_cast` 硬编码识别 `STD_ARRAY`
/// / `STD_OBJECT` 子类型。
pub const STD_ARRAY: &str = "Std.Array";

// ── Integer prim-wrapper scalar spec (unify Phase 2 R3 装箱统一) ──────────
//
// 基元装箱把整数标量 LE 字节存进 boxed `ScriptObject` 的 `struct_bytes`（D1-B：与
// struct 装箱完全同构）。box/unbox 两侧都按 wrapper FQ 名查这张表定「标量宽度 + 有无符号」——
// 宽度决定 struct_bytes 尺寸（结构统一），有无符号决定 unbox 时按 W 字节还原 i64 是 sign- 还是
// zero-extend。只有整数 wrapper 会到装箱路径（`__box_prim` 只装整数；bool/char/double/string
// 保留各自 `Value` variant，不经此路）。未列名（不该发生）→ `None`，调用方回落全 8 字节 signed。

/// (标量字节宽度, 是否有符号) —— 整数 prim-wrapper 的 FQ 名。narrow BCL 名（`Std.Byte` /
/// `Std.Int16` / `Std.UInt32` …）虽未在上面注册常量，但会作为 compile-time emit 的 class FQN
/// 抵达装箱路径，故此表须覆盖全部整数 wrapper。
pub fn int_wrapper_scalar_spec(name: &str) -> Option<(usize, bool)> {
    match name {
        "Std.SByte"  => Some((1, true)),
        "Std.Byte"   => Some((1, false)),
        "Std.Int16"  => Some((2, true)),
        "Std.UInt16" => Some((2, false)),
        "Std.Int32"  => Some((4, true)),
        "Std.UInt32" => Some((4, false)),
        "Std.Int64"  => Some((8, true)),
        "Std.UInt64" => Some((8, false)),
        _ => None,
    }
}

// ── Well-known builtin names (used outside corelib::dispatch_table) ──────

/// Builtin invoked as the fallback in `dispatch.rs::obj_to_string` when an
/// object's vtable doesn't override `ToString`. Returns the simple class
/// name (e.g. `Foo{...}`).
pub const BUILTIN_OBJ_TO_STR: &str = "__obj_to_str";

// ── Well-known method names (vtable + dispatch lookup keys) ──────────────

/// `Std.Object.ToString()` — vtable key + IR-emitted method name.
pub const METHOD_TO_STRING: &str = "ToString";

/// Per-module static initialiser suffix. Every `__static_init__` function
/// (one per file with non-trivial static fields) ends with this suffix —
/// VM scans `module.func_index` for `*.{METHOD_STATIC_INIT}` to run them
/// before the entry point.
pub const METHOD_STATIC_INIT: &str = "__static_init__";
