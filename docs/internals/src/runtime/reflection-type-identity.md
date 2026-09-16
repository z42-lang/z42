# 反射 Type 身份：Type 对象自身的 GetType、数组类型名

> fix-value-type-object-methods（2026-09-01）。反射的 `Std.Type` 是运行时类型描述符，程序对它的
> `GetType()` 与数组类型的名字都要对齐 C#。这两处的坑一个在**编译器/运行期方法派发**（static 与
> instance 同名撞车）、一个在**运行期 Type 名合成**（数组元素名擦除）。值类型（struct/enum）的 Object
> 方法派发见 [struct 值语义 §编译器派发](struct-value-semantics.md)。

## Type 对象的 `GetType()`：static 与 instance 同名在 vtable 撞车

`typeof(X).GetType()` 应返回 `typeof(Std.Type)`（Type 对象的运行期类型就是 `Std.Type`）。此前返回
`null`，链式 `.FullName` 直接 `FieldGet on Null` 崩。

**根因不在编译器重载决议**——它其实已正确选中 arity-0 的 `GetType`。根因在**运行期 vtable 构建**
（`metadata/loader/type_registry.rs::merge_with_base`）：

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

`typeof(int[]).FullName` 此前是合成的 `Std.Array`（丢了元素类型）。C# 是 `Std.Int32[]`（元素 FQN + `[]`）。

**机制**（`corelib/reflection/type_object.rs::make_type_from_name` 的 `[]` 臂）：数组名以 `[]` 结尾时，
**递归解析元素 Type**（`make_type_from_name(elem)`），读其 `__name` / `__fullName`，拼成
`{elemName}[]` / `{elemFullName}[]`：

- `int[]` → 元素 `int` 解析到 `Std.Int32` 真句柄（含 change A 的 `int→Std.Int32`）→ `Int32[]` /
  `Std.Int32[]`。
- `int[][]` → 元素 `int[]` 再递归 → `Int32[][]` / `Std.Int32[][]`（任意深度）。
- 用户类 `Box[]` → `Test.Box[]`。
- **空元素标签**（少数无元素信息的数组）保留旧合成名 `Std.Array`。
- `__elementName` 不变（`GetElementType()` 仍读它）。

`typeof(T[])` 与运行期 `arr.GetType()` 走**同一** `make_type_from_name`（后者经
`builtin_obj_get_type` 用数组值的 `element_type`）→ 两路名字一致。

### 数组 receiver 的 `GetType()` 返回类型（编译器）

`xs.GetType()` 此前在 `MemberResolver._bindInstanceMemberCall` 落到**兜底**分支，返回类型松绑
`Z42UnknownType` → 链式 `xs.GetType().FullName` 退化成 `FieldGet "FullName"` → `null`（`var t =
xs.GetType(); t.FullName` 或显式 `Type t = …` 反而正常，因绕过了这条兜底）。补一条 `Z42ArrayType`
分支：数组继承 Object 四方法，查 `Object` 取**真实返回类型**（`GetType`→`Std.Type`），镜像已有的
`Z42GenericParamType` 分支。运行期数组值上的 VCall 仍走 `builtin_obj_get_type` 数组特化，OwnerClass
与兜底同名、不改派发——只把 receiver 结果的静态类型从 Unknown 修正为 `Std.Type`，让链式属性正确绑定。
