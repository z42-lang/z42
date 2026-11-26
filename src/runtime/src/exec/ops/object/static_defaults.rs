//! Materialize a declared static field default using the running VM.
use crate::object::{TypeArgs, TypeDesc, Value, type_args_from_name};
use crate::vm::VmContext;

/// 声明类型为 `tag` 的静态字段的**零值**（C# 的 `default(T)`）。静态存储的默认初始化归 VM：
/// 槽位从未写过时为 `Null`，[`crate::exec::symbol_resolution::verify_static_field`] 在第一次读到它时按声明类型补上这里的值，
/// 调用方经 `StaticTable::init_default` 装入槽位（先到者胜）。
///
/// | 声明类型 | 零值 |
/// |---|---|
/// | 基元（`int` / `double` / `bool` / `char` …） | 标量零（`default_value_for`） |
/// | 枚举 | `I64(0)`（z42 的枚举一律以 i64 承载） |
/// | 值 struct（含泛型 struct 的实例化） | **新分配的全零装箱 struct**：编译器把 struct 静态字段存成盒子（`_boxIfStaticStruct`），整读 `AsCast` 拆箱、叶子读写 `S.P.X` 就地改盒子；分配即零初始化（各叶子零值、引用叶子 null） |
/// | 空 struct | 跳过构造器分配的类型化零值 |
/// | 引用类型 / 数组 / 委托 | `Null` |
///
/// 泛型类实例化上的静态字段，`tag` 已是代换后的实参（`Demo.Holder<int>` 上 `static T V` 的 tag 是
/// `int`），无需在这里解析型参。
pub(crate) fn static_zero_value(ctx: &VmContext, tag: &str) -> crate::object::Value {
    zero_value_for_type(ctx, tag)
}

/// Materialize a zero value in a dynamic register or reference-backed static slot.
pub(crate) fn zero_value_for_type(ctx: &VmContext, tag: &str) -> crate::object::Value {
    let prim = crate::object::default_value_for(tag);
    if !matches!(prim, Value::Null) { return prim; }
    if matches!(tag, "" | "unknown" | "string" | "str" | "Std.String" | "object" | "Std.Object")
        || tag.ends_with("[]") || tag.contains('(')
    {
        return Value::Null;
    }
    // Static-field tags may have spaces between type arguments. The lookup key is canonical.
    let name: String = tag.chars().filter(|c| !c.is_whitespace()).collect();
    let Some(td) = lookup_type(ctx, &name) else { return Value::Null };
    if td.is_enum() { return Value::I64(0); }
    if is_empty_struct(&td) {
        let args = if td.name == name {
            td.type_args_handle()
        } else {
            TypeArgs::intern(&type_args_from_name(&name))
        };
        return ctx.heap().alloc_instance(td, args);
    }
    if !td.is_struct() || !td.struct_layout_ref().is_some_and(|layout| layout.size > 0) {
        return Value::Null;
    }
    match ctx.heap().alloc_object(td, Vec::new(), crate::object::NativeData::None) {
        Value::Object(gc) => Value::BoxedStruct(gc),
        // alloc_object 在严格 OOM 拒绝下返回 Null：不装入，照旧返回 Null。
        _ => Value::Null,
    }
}

/// Resolve a candidate zero-value type. Generic reference types only need the lookup of
/// their definition. Empty struct instantiations can reuse the definition's zero-size layout.
fn lookup_type(ctx: &VmContext, name: &str) -> Option<std::sync::Arc<TypeDesc>> {
    let Some(lt) = name.find('<') else { return ctx.loader().try_lookup_type(name) };
    let def = ctx.loader().try_lookup_type(&name[..lt])?;
    if !def.is_struct() { return None; }
    ctx.loader().try_lookup_type(name)
        .or_else(|| is_empty_struct(&def).then_some(def))
}

fn is_empty_struct(td: &TypeDesc) -> bool {
    td.is_struct() && td.fields.is_empty() && td.struct_layout_ref().is_some_and(|layout| layout.size == 0)
}
