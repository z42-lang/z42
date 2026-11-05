# 反射 Type 身份：构造类型、Type 对象自身的 GetType、数组类型名

> 反射的 `Std.Type` 是运行时类型描述符。构造泛型（`Box<int>`）在运行期只有**一个**身份，`GetType`、
> `typeof`、`==`、`IsAssignableFrom`、反射构造与 `is` 全按它比；程序对 Type 对象的
> `GetType()` 与数组类型的名字都要对齐 C#。后两处的坑一个在**编译器/运行期方法派发**（static 与
> instance 同名撞车）、一个在**运行期 Type 名合成**（数组元素名擦除）。值类型（struct/enum）的 Object
> 方法派发见 [struct 值语义 §编译器派发](struct-value-semantics.md)。

## 构造类型的名称与比较键

### 实例的两种表示

| 谁构造 | 实例的描述符 | 实参在哪 |
|---|---|---|
| 泛型定义所在的包（`Demo.Box<T>` 在本包，`new Box<int>()`） | 编译器为每个具体实例化发的**实例化描述符** `Demo.Box<int>`（基类 / 接口 / 字段类型已代换） | 描述符名里（`type_args` 从名字解析） |
| 别的包（`new List<int>()`、主包 `new Lib.GBox<int>()`） | 泛型**定义**的描述符 `Std.Collections.List` | 实例自己的 `type_args`（`ObjNew` 写入） |

同一个类型两种表示都会出现：库里构造的 `GBox<int>` 挂库的实例化描述符，主包构造的挂定义 + 实参。
两者必须是**同一个类型**。

### 规范名（`metadata/type_names.rs`）

布局注册、按名派发和实例类型测试共用以下规范名：

- 类按全名（例如 Demo.Box），实参递归；无空白；
- 顶层的内建类型写全名（`Std.Int32`、`Std.Object` —— 描述符的注册键）；
- **类型实参与数组元素**里的内建类型写关键字：`Std.Collections.List<int>`、`Demo.Box<object>`、`int[]`；
- 函数类型作实参写成委托名：`Std.Func<int,int>` / `Std.Action<string>`。

`Type.FullName` 是它的**展示形**（`type_names::display`：内建类型处处写全名，
`Std.Collections.List<Std.Int32>`）。反射 Type 的比较另用创建时确定的键。

**谁产出规范名**：编译器发射时就按它写（`LayoutKey` 是唯一的拼写出口：实例化描述符名、`ObjNew` 的实参、
`is_instance` 目标、特化函数名）；运行期**从 `Type` 对象取名**的地方——`MakeGenericType`、
`Activator.CreateInstance`、`ConstructorInfo.Invoke`、`MakeGenericMethod` 绑定的方法实参——经
`type_value_name` / `type_value_arg_name`（`canonical(FullName)`）回到同一拼写。

为什么在**发射期**规范化、而不是在运行期比较时归一：

- 类型测试在热路径上（`is` / `as` / `catch`，isa 缓存按 `(描述符 id, 目标键)`）；两边已是同一拼写，
  比较就是一次字符串相等，不分配、不解析；
- 编译器的拼写同时是**描述符与布局的注册键**（`Demo.Box<int>` 既是实例化描述符名也是 struct 布局名），
  运行期无法单方面改名而不动布局解析；
- 运行期的 `canonical` 只负责把**展示词汇**（`Std.Int32`、旧拼写里的空格）映射回去，不猜类名——
  它认的只有内建类型的几种拼写（`type_names::BUILTIN_TYPES`：关键字 ↔ `Std` 全名，与编译器
  `PrimModel` 的关键字是同一组名字）。

### 构造类型的 `Type`

名字入口 `make_type_from_name` 解析 `<…>` 中的实参。`MakeGenericType` 已持有实参 Type，校验通过后
直接使用接收者的定义句柄，将这些对象放入新实参数组。两条路径共用对象分配与比较键构造，`__typeArgs` 保存 Type 引用，`GetGenericArguments()`
据此保留实参的程序集归属。结果句柄用于泛型定义，FullName 用于名称显示及按名派发。

- **`GetType()`**：实例化描述符 → 以其名字构造（不再把实参拼两遍）；定义 + 实例实参 →
  `type_names::constructed`（定义名 + 实参）再构造。两种表示因此得到相等的 Type，也等于 `typeof`。
- **`BaseType`**：优先从开放基类表达式 `$ReflectionBase` 投影实际实参。布局和派发继续使用注册键。
  无法投影时沿实例化描述符、定义描述符的顺序查找。
- **`GetInterfaces()`**：先查按规范名注册的实例化描述符，再退回定义。
- **`GetFields()` / `GetProperties()` / `GetMethods()` / `GetConstructors()`**：类声明的成员按定义型参
  投影实际的 Type，包括嵌套数组和泛型。继承成员沿基类链查找声明类的实参。方法自己的型参保留其作用域，
  缺少基类表达式的产物仍可使用已有实例化描述符。
- **`IsAssignableFrom`**：两侧都按规范名走 `is_subclass_or_eq_td`；名字没有描述符（导入泛型的实例化）时
  按「定义 + 这些实参」回答，与 `isa_object` 对这种实例的回答一致。

### 反射 Type 的不可变比较键

普通声明用运行时 TypeId。构造泛型组合定义和有序实参键，数组组合创建时的元素键。无句柄的符号类型
用其规范名称，同名的真实声明另有 TypeId。比较和哈希只读取保存的键。泛型和数组的 native backing
保存 Rust 结构，实参及元素投影走已有 GC 字段。`GetGenericArguments()` 返回数组快照，写入该数组
与键、后续投影及成员代换互相隔离。`Type` 实现
`IEquatable`，因此可直接用作 `Dictionary` 和 `HashSet` 的键。

### 反射构造得到与 `new` 相同的表示

`construction_target(规范名)`：有实例化描述符就分配它（实参取自描述符），否则分配定义 + 规范实参。
`Activator.CreateInstance`（实参 `typeof(Box<int>)`）与 `new T()`（T = `List<int>`）造出的对象因此 `is Box<int>` /
`is List<int>`、`GetType()` 等于 `typeof`。`ConstructorInfo` 记下枚举它的那个 Type 的规范名（`__owner`），
`Invoke` 按它分配；实例化有自己的（布局特化）构造器时用它，否则用定义的。

> 尚未覆盖：导入的**泛型 struct** 装箱后不带实参（`object o = new Lib.SBox<int>(4)` 的 `GetType()` 是
> Lib.SBox）。消费方按定义布局分配它，而定义方给自己的实例化另算了一份布局，两边布局对不上之前，
> 消费方不能用实例化名指代它。

## Type 对象的 `GetType()`：static 与 instance 同名在 vtable 撞车

`typeof(X).GetType()` 应返回 `typeof(Std.Type)`（Type 对象的运行期类型就是 `Std.Type`）。若 vtable 被污染则
返回 `null`，链式 `.FullName` 直接 `FieldGet on Null` 崩。

**坑不在编译器重载决议**（它正确选中 arity-0 的 `GetType`），而在**运行期 vtable 构建**
（`vm/runtime/types/registry.rs::merge_with_base`）：

- `Std.Type` 声明了一个**静态** `GetType(string fqn)`（`Type.z42`，`[Native("__type_get_type")]`，
  按 FQ 名查类型）。它的限定函数名 `Std.Type.GetType$1$string` 进 `own_methods`。
- `merge_with_base` 从 `own_methods` 建 instance vtable 时，用 `derive_simple_method_name` **剥掉
  `$1$string` mangle** 得简单名 `GetType`——与从基类（`MemberInfo`→`Object`）继承来的 instance
  `Object.GetType`（arity-0）**同名撞车**，于是**静态方法覆盖了 instance 槽**：`vtable["GetType"]` 指向
  静态 extern。
- 运行期 `t.GetType()` 走 VCall → vtable 命中静态 extern `__type_get_type` → 把 receiver（Type 对象）
  当 `fqn` 字符串 → 查不到 → `null`。

> 为什么这不是 struct 独有：任何类只要有一个 static 方法，其简单名撞上某个继承来的 instance 方法，
> 就会污染 instance vtable 槽。`Std.Type` 只是第一个真实触发者（它的 static `GetType(string)` 撞
> `Object.GetType()`）。

**根治：static 方法不进 instance vtable。** `TypeDescCold` 增 `own_static_flags`（与 `own_methods`
index 对齐，构建时从 `Function.is_static` 采）。`merge_with_base` 跳过 static 项——它们只经**直呼**
（mangled FQ 名 `Call`）派发，从不虚派发，占 instance 槽本就是错的。反射 `GetMethods()` 仍从**完整**
`own_methods` 枚举 static 方法，不受影响。

> **收敛器同步（易漏）**：`needs_fixup` 的 `expected_vtable_count` 投影也要跳过 static，否则实际
> vtable（跳了 static）永远比投影少 → `needs_fixup` 恒真 → 跨-zpkg 继承 fixup **永不收敛**（loader
> 776 轮报错）。构建与投影两处的「跳 static」判据必须一致。

## 数组类型名：全路径元素名（非元素-擦除的 `Std.Array`）

`typeof(int[]).FullName` 是 `Std.Int32[]`（不是丢掉元素类型的 `Std.Array`），与 C# 一致（元素 FQN + `[]`）。

**机制**（`corelib/reflection/type_object.rs::make_type_from_name` 的 `[]` 臂）：数组名以 `[]` 结尾时，
**递归解析元素 Type**（`make_type_from_name(elem)`），读其 `__name` / `__fullName`，拼成
`{elemName}[]` / `{elemFullName}[]`：

- `int[]` → 元素 `int` 解析到 `Std.Int32` 真句柄（含 `int→Std.Int32`）→ `Int32[]` /
  `Std.Int32[]`。
- `int[][]` → 元素 `int[]` 再递归 → `Int32[][]` / `Std.Int32[][]`（任意深度）。
- 用户类 `Box[]` → `Test.Box[]`。
- **空元素标签**（少数无元素信息的数组）保留合成名 `Std.Array`。
- 数组 Type 的内部引用槽保留生成名称所用的元素 Type。`GetElementType()` 返回这个投影，后续加载仅影响
  新创建的数组 Type。旧种子布局通过 `__elementName` 解析。

`typeof(T[])` 与运行期 `arr.GetType()` 走**同一** `make_type_from_name`（后者经
`builtin_obj_get_type` 用数组值的 `element_type`）→ 两路名字一致。

### 数组 receiver 的 `GetType()` 返回类型（编译器）

`MemberResolver._bindInstanceMemberCall` 对 `Z42ArrayType` 收者委托给 `BindArrayInstanceCall`（`MemberResolver.Prim.z42`）：数组继承 Object 四方法，查 `Object` 取**真实返回类型**（`GetType`→`Std.Type`），镜像
`Z42GenericParamType` 分支；`Std.Array` 的实例方法（`Clone` / `GetValue` / `SetValue`）照旧松绑定；两处都没有的名字报 `E0401`。若落到松绑定，返回类型是 `Z42UnknownType` → 链式 `xs.GetType().FullName` 退化成 `FieldGet "FullName"` → `null`。运行期数组值上的 VCall 仍走 `builtin_obj_get_type` 数组特化，OwnerClass
与兜底同名、不改派发——只把 receiver 结果的静态类型定为 `Std.Type`，让链式属性正确绑定。
