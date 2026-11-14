//! Materialize a declared static field default using the running VM.
use crate::{context::VmContext, object::TypeDesc};

/// 声明类型为 `tag` 的静态字段的**零值**（C# 的 `default(T)`）。静态存储的默认初始化归 VM：
/// 槽位从未写过时为 `Null`，[`crate::exec::symbol_resolution::verify_static_field`] 在第一次读到它时按声明类型补上这里的值，
/// 调用方经 `StaticTable::init_default` 装入槽位（先到者胜）。
///
/// | 声明类型 | 零值 |
/// |---|---|
/// | 基元（`int` / `double` / `bool` / `char` …） | 标量零（`default_value_for`） |
/// | 枚举 | `I64(0)`（z42 的枚举一律以 i64 承载） |
/// | 值 struct（含泛型 struct 的实例化） | **新分配的全零装箱 struct**：编译器把 struct 静态字段存成盒子（`_boxIfStaticStruct`），整读 `AsCast` 拆箱、叶子读写 `S.P.X` 就地改盒子；分配即零初始化（各叶子零值、引用叶子 null） |
/// | 引用类型 / 数组 / 委托 | `Null` |
///
/// 泛型类实例化上的静态字段，`tag` 已是代换后的实参（`Demo.Holder<int>` 上 `static T V` 的 tag 是
/// `int`），无需在这里解析型参。
pub(crate) fn static_zero_value(ctx: &VmContext, tag: &str) -> crate::object::Value {
    use crate::object::Value;
    let prim = crate::object::default_value_for(tag);
    if !matches!(prim, Value::Null) { return prim; }
    let Some(td) = lookup_value_type(ctx, tag) else { return Value::Null };
    if td.is_enum() { return Value::I64(0); }
    match ctx.heap().alloc_object(td, Vec::new(), crate::object::NativeData::None) {
        Value::Object(gc) => Value::BoxedStruct(gc),
        // alloc_object 在严格 OOM 拒绝下返回 Null：不装入，照旧返回 Null。
        _ => Value::Null,
    }
}

/// `tag` 若命名一个**按值存放**的用户类型（枚举，或有非空布局的 struct —— 与编译器
/// `StructLayout.IsBlobStruct` 同一判据），返回它的描述符；引用类型 → `None`。
///
/// 查询会按需加载声明该类型的包（它是字段的声明类型，编译期就解析过）。两道防线让「查一个
/// 解析不到的名字」不至于退化成加载全部包（加载器的 Fallback B）：
/// - 一眼可知不是值类型的拼写不查：数组、委托签名、编译器的 `unknown` 哨兵。可空值类型在 z42
///   里不合法（E0476），字段的 `?` 也不进类型拼写（走 `$Nullable` 旁路）。
/// - 泛型实例化先查**泛型定义**（按命名空间路由、一定存在），定义是 struct 才查实例化名；
///   `List<int>` 这类引用型实例化于是从不按实例化名去查。
///
/// 字段类型拼写是源码口径，实参间带空格（`Demo.Pair<int, long>`，`ClassDescBuilder._typeSourceName`），
/// 而类型身份名没有（`Demo.Pair<int,long>`，即 `AsCast` 拆箱时用的名字）——按身份名查。
fn lookup_value_type(ctx: &VmContext, tag: &str) -> Option<std::sync::Arc<TypeDesc>> {
    if tag.is_empty() || tag == "unknown" || tag.ends_with("[]") || tag.contains('(') {
        return None;
    }
    let name: String = tag.chars().filter(|c| !c.is_whitespace()).collect();
    if let Some(lt) = name.find('<') {
        if !ctx.try_lookup_type(&name[..lt])?.is_struct() { return None; }
    }
    let td = ctx.try_lookup_type(&name)?;
    let by_value = td.is_enum()
        || (td.is_struct() && td.struct_layout_ref().is_some_and(|l| l.size > 0));
    by_value.then_some(td)
}

