# 装箱与拆箱（primitive ↔ object）

> 来源：`add-boxing-conversions`（0.3.11，基础语义）→ **`add-primitive-value-boxing`（2026-07-23，强类型装箱，现行）**。
> Method.Invoke（0.3.12）的前置。

## 设计：类型标记的堆装箱（`Value::Boxed`）

z42 运行期 `Value` 是 **tagged union**（`I64/F64/Bool/Char/Str` 与 `Object/Array` 平级）。基元与
`object`/接口 之间的装箱采**带精确类型标记的堆装箱**——因为 `Value` 的 8 字节内联 payload 放不下
「宽度 tag + i64」，且强类型 `is`/`as`/`GetType` 要求装箱值**保留精确基元类型**（`int` 装箱后 `is int`
为真、`is long` 为**假**），故走堆：

```rust
Boxed(Box<BoxedPrim>) = 13,                          // Value 变体
struct BoxedPrim { class: Arc<str>, inner: Value }   // class = FQ 基元 struct 名（Std.Int32/Std.Int64/…）
                                                     // inner = 裸基元值（I64/F64/Bool/Char/Str）
```

- **只在装箱点创建**（prim→object/接口 转换）；算术与方法体永远拿拆箱后的 `inner`，故**热路径零影响**。
- `Box<…>` 是 8 字节指针 → 不撑大 `Value`。
- z42 的基元是**真 struct**（`struct Int32 : IComparable<int> …`），故带 `class` type_desc 的装箱值
  经 Object 路径**免费获得** is-a / GetType / vcall。

> **历史订正（2026-07-23）**：本页早前描述的「未采用 `Value::Boxed`、unified Value、装箱=codegen no-op」
> 是 `add-boxing-conversions`(0.3.11) 的**旧**设计。彼时 `object o = 5L` 编译为裸 copy、裸 `Value::I64`
> 流入 object 槽 → **丢失静态类型**（`5` 装箱后 `is long` 误为真）。`add-primitive-value-boxing` 用上面的
> 类型标记堆装箱**取代**了它，实现真强类型。本页已更新为现行设计。

## IR / 机制：`__box_prim` builtin（无格式 bump）

- **装箱**：`__box_prim` **BuiltinInstr**（复用现有 builtin opcode，非新 IR 指令）——
  `const.str "Std.Int64"; builtin __box_prim %dst,%val,%cls` → `%dst = Boxed{class, inner=val}`。
- **拆箱**：复用现有 `AsCast` opcode——Boxed → prim 时 is-a 校验后返 `inner`。
- **无新 opcode → 无 zbc/zpkg 格式 bump → 无两代自举**。

## 编译器：装箱插入点（全覆盖）

编译期在**每一处**「源基元静态类型 → 目标 `object`/接口 静态类型」的隐式转换点插入 `BoundBox`
（`TypeOpEmitter._emitBox` → `__box_prim`）。**已覆盖全部 coercion 点**：

| 插入点 | 位置 |
|--------|------|
| var-decl / 赋值 `object o = 5L` | TypeChecker 赋值兼容处 |
| return（返回类型 object/接口） | `StmtBinder._bindReturn` BoxIfNeeded |
| 数组字面量 `object[]` 整数元素 | `ExprTyper._bindArrayInit` |
| call-arg（形参 object/接口） | `TypeChecker.BoxArgs` + `OverloadBinder._withDefaults`（5 调用点公共汇聚） |
| `params object[]` 尾包元素 | `_withParamsExpansion` 逐元素按元素类型装箱 |

**拆箱消歧**：`(int)x` 有两义——① `x` 是 object → 拆箱（`AsCast`/Unbox）；② `x` 是数值 → 数值窄化
（`Convert`）。按 `x.Type()` 分派，object/接口源走拆箱，数值源仍 `Convert`（绝大多数现有 cast 属②，不受影响）。

**call-arg↔基元 native 交互**：call-arg 装箱把整数实参装箱成 object（如 `Assert.Equal(object,object)`），
基元 struct 的 native 方法（Equals/CompareTo）按裸 long 读参 → `arg_i64`（`corelib/convert.rs` 取参助手）
**透明拆箱 `Boxed(I64)`**，一处修全部 int native。

## 运行时语义（interp + jit 全路径）

| 操作 | Boxed 行为 |
|------|-----------|
| `is_instance`（`o is long`）| `is_subclass_or_eq_td(registry, &b.class, target)`——走真 type_desc（base+接口链）|
| `as_cast`（`o as T`）| target 是 object/接口且匹配 → 保持 Boxed；target 是 prim → 拆箱返 inner；否则 Null |
| `GetType` | 从 `b.class` 造 `Std.Type`（精确基元类型）|
| `vcall` / `Equals` / `ToString` / `GetHashCode` | 解析 `b.class` 的方法，`this = b.inner`（拆箱后交基元 struct 方法体）|
| GC trace | inner 为基元 / `Str(Arc)` → 无 GC ref |

裸 `Value::I64`（未过 object 边界的场景）仍走 `prim_isa` 松匹配（`fix-boxed-primitive-is-as`）；
**装箱后**走 Boxed 臂精确匹配。→ 强类型：`object l=9L; l is long`（装箱 `Std.Int64`）→ true；
`object x=5; x is long`（装箱 `Std.Int32`）→ **false**。

## 语义规则

| 方向 | 规则 |
|------|------|
| prim → object | 隐式可赋值；编译期插 `__box_prim`（带精确基元类型）|
| object → prim | 显式 cast `(T)o` / `o as T`，运行期 is-a 受检拆箱；不符/null → 异常 / Null |
| object[] 元素 | `a[i] = prim` 逐元素装箱；`(T)a[i]` 逐元素拆箱 |
| 引用类型 → object | 引用上转（class/record/array 已是带 TypeDesc 的 GcRef，装箱=恒等，不走 Boxed）|
| 数组协变 | **不引入** `int[] <: object[]`（避免 store-hole）|

## 健全性

装箱 = 加宽上转（prim→object，安全）+ 受检下转（object→prim，运行期查 `Boxed.class` 的 is-a）。
`Boxed` 携带精确基元 type_desc，下转可靠校验、强类型 `is`/`as` 精确——无法把一个类型当另一个用。

## enum 精度边界

`enum` 值当前底层是**裸 `long`**（无独立类型标记，`Color c = Color.Green` 尚不能持有带类型 enum 值——
枚举成员访问求值为底层 `long`）。故装箱 enum → 底层 `Int64`，`enumValue.GetType().IsEnum` 得 false。
这是 enum **值表示**的边界，不是装箱机制的边界——精确 enum 装箱需先让 enum 值携带其 enum 类型
（语言语义改动，见 Deferred）。

## Deferred / Future Work

### add-boxing-future-enum-precise
- **触发原因**：enum 值底层裸 `long`，装箱丢类型精度（GetType→Int64，`(MyEnum)o` 与 `(long)o` 不可区分）；
  且 `Color c = Color.Green` 当前不能持有带类型 enum 值。
- **前置依赖**：**enum 值携带 enum 类型**（值表示层改动，非 coercion 插入）——`Value` 需能表达带类型
  enum 值（如 `Boxed{class: "MyNs.Color", inner: I64}` 或专用带-tag enum 值），且成员访问/赋值/比较
  链路按 enum 类型而非底层 long 传播。
- **当前 workaround**：装箱 enum 视作其底层 int；需精确类型时不经 object 中转。

### add-boxing-future-catchable-invalidcast
- 拆箱失败经运行期 `Convert`/`AsCast` 内部错误产生，当前是**终止性 VM 异常、不可 `try/catch` 捕获**——
  与所有 `Convert` 失败一致。让其成为可捕获 z42 异常是独立的既有问题，不属装箱机制。
