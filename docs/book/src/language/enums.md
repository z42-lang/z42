# enum（枚举）

> SoT。`make-enum-distinct-type`（2026-09-09）之前 z42 的 enum 是 **enum-as-int**：
> 成员就是 `long`、类型名是个孤立的类。本页描述的是取代它的模型。旧模型的残留描述
> 曾散落在 [`runtime/struct-value-semantics.md`](../runtime/struct-value-semantics.md)，
> 已改为指回本页。

## 一句话

**enum 是独立类型，不是整数的别名。** 底层表示是 `i64`，但那是**表示**、不是**身份**——
`Color` 与 `long` 之间要转换必须显式写出来，两个不同的 enum 之间也一样。

## 为什么改：旧模型自相矛盾

旧模型里 `E.Member` 绑成 `BoundLitInt(long)`，而**类型位置**的 `E` 解析成一个在转换格里
没有任何边相连的孤立 `Z42ClassType`。两者根本不是同一个东西，于是：

```z42
enum Color { Red, Blue }

Color c = Color.Blue;   // ❌ E0402：把 long 赋给一个谁也转不进去的类
```

**z42 因此产不出任何 enum 类型的值**——enum 只能当整数用，声明一个 `Color` 变量就编不过。
这不是「设计成整数别名」，是一个 bug：类型名存在、值却永远到不了它。

## 语义（对齐 C#）

### 转换：双向都要显式 cast

```z42
Color c = Color.Blue;        // ✅ 成员的静态类型就是 Color
long  n = (long)Color.Blue;  // ✅ 读底层值：显式写出来
Color d = (Color)1;          // ✅ 反向同理

long  m = Color.Blue;        // ❌ E0439
Color e = 1;                 // ❌ E0439
Color f = 0;                 // ❌ E0439 —— `0` **不开** C# 那个后门
```

> **为什么 `0` 不开例外**：C# 允许 `Color c = 0` 是历史包袱（为了 `[Flags]` 的零值）。
> 开了它，enum 就又变成「0 是它、别的不是」的怪东西。z42 pre-1.0，不背这个包袱。

不同 enum 之间**不隐式互转**（底层都是 i64，正是旧模型下最容易串味的一对）：

```z42
enum Mood { Sad, Glad }
Color c = Mood.Glad;         // ❌
```

### 比较：两侧必须是同一个 enum

`==` / `!=` / `<` / `<=` / `>` / `>=` 都要求两侧同一个 enum（关系比较按底层 i64 排序）：

```z42
bool a = Color.Red == Color.Blue;   // ✅
bool b = Color.Red <  Color.Blue;   // ✅
bool c = Color.Red == 0;            // ❌ 要比底层值就写 (long)Color.Red == 0L
bool d = Color.Red == Mood.Glad;    // ❌ 异种 enum
```

> ⚠️ 这道「配对」检查在旧模型下**完全不存在**：`==` 的 `BinaryRule` 不带操作数约束，
> 而 `_checkOperand` 是逐侧看的——「左右配不配」零覆盖。现由
> `TypeChecker._checkEnumOperands` 补上，且**只作用于比较运算符**：算术位上
> `_checkOperand` 已经先报了「requires numeric operand」，两道一起上会同一个错报两遍。

### 算术：不参与

enum 不是数字，`+`/`-`/`*`/… 一律拒绝（要算就先 `(long)` 出来）。这条不只是洁癖——
它是发射端的**前提**，见下「字符串拼接」。

## 运行期表示与身份

**表示不变**：enum 值在寄存器 / 字段 / 数组槽里就是一个 `i64`。
`Type.GetEnumUnderlyingType()` 恒返 `typeof(long)`（声明的 `: byte` 当前被忽略）。

**身份在装箱时才需要被携带**。enum 值擦除到 `object` / 接口时装箱成一个
`BoxedStruct`，盒上挂的是 **enum 自己的 `TypeDesc`**（不是 `Std.Int64`）：

```z42
Color  c = Color.Blue;
object o = c;                    // 装箱：盒的 TypeDesc = Color
o.GetType().Name                 // "Color"
o is Color                       // true
(long)((Color)o)                 // 1 —— 拆箱透明
```

挂 `Std.Int64` 也能保住 i64 承诺，但 `GetType()` 会答 `Int64`，与 `typeof(Color)` /
`Color.Blue.GetType()` 仍然不自洽——那正是这次要根除的东西。

**擦除面与整数基元逐字一致**：`object` / 接口装箱，**泛型形参不装**。
`List<Color>` 和 `List<int>` 一样存裸 i64，容器内外表示不变、不需要拆箱对偶。
代价是一个**未代换的类型形参**位上的 enum 值是裸 i64（见下「已知边角」）。

## 字符串化：成员名（C# `Enum.ToString`）

四条路答案必须一致：

```z42
Color c = Color.Blue;
c.ToString()               // "Blue"
((object)c).ToString()     // "Blue"
"" + c                     // "Blue"
$"{c}"                     // "Blue"
Console.WriteLine(c);      // Blue
```

未定义值（越界 cast / 位组合）→ 数字，同 C#：

```z42
((Color)9).ToString()      // "9"
```

### 实现原理

运行期的 i64 不带类型信息，所以**字符串化前先装箱**——盒带着 `TypeDesc`，
`TypeDesc::enum_member_name` 就能从既有的 `enum_members` 元数据查回名字。两侧各一个入口、
共用同一个查表：

| 入口 | 谁走 |
|---|---|
| `interp::vcall_resolve` 的 enum 臂 | `x.ToString()`（receiver 是盒） |
| `corelib::convert::value_to_str` | `Console.WriteLine(object)` / 拼接 / `ToStr` |

编译器侧一个原语 `TypeOpEmitter._emitEnumBox`（非 enum 静态类型 no-op），挂三处发射点：
插值洞、字符串 `+`、`e.ToString()`。

> **字符串 `+` 为什么不判「结果是不是 string」**：因为不需要。enum 的算术 `+` 已经被转换格
> 拒掉了 ⇒ 能走到发射、且带 enum 操作数的 `+` 只可能是拼接。这是类型系统给的**不变式**，
> 不是发射端的假设——`enum_type_tests.z42` 里那两条算术负例就是它的门闩。

## `Equals` / `GetHashCode`

enum 不声明任何方法，所以 `Object` 协议的候选走查在 enum 盒上会走空、落到
`Std.Object.Equals` → `__obj_equals`（引用语义）⇒ **`Color.Green.Equals(Color.Green)` 会答
false**。解法是让 enum 盒的候选类名换成**底层类型** `Std.Int64`：`Std.Int64.Equals(long)`
正是想要的值比较，与装箱 `long` 走同一个方法。（`ToString` 不受影响——前面的 enum 臂先拦下。）

## 跨包 enum

导入的 enum 在消费方与本地 enum **完全同形**：同样带 `IsEnum` 标志、同样的转换/比较规则、
同样按成员名字符串化。两条独立的还原路径都要接通：

| 路径 | 谁走 |
|---|---|
| 源码里**写出的**类型引用（`Color c = …`） | `SymbolTable.EnumTypes` → `Z42ClassType.Enum` |
| **导入成员签名里**的类型（如 `Type.Visibility` 的返回类型） | `ImportedSymbolLoader._resolve` 的 `EnumTypeNames` 分支 |

只接第一条会漏掉第二条——签名里的 enum 落到末尾的 prim fallback、丢掉标志，于是
`t.Visibility == TypeVisibility.Public` 报「enum 比非 enum」。

另外，导入的 enum 必须把**源命名空间**登记进 `ClassNamespaces`：类名限定
（`EmitContext.QualifyClass`）查的是那张表。缺了它，消费方发出的是
`<消费方ns>.Color` 这种不存在的 FQ 名——在没人拿它查运行期类型的年代这**是个静默错误**
（`typeof` 只拿到查不到 handle 的合成 `Type`），装箱会真的去查，于是当场炸。

## 反射面

反射一律以 **i64 本位**（与底层表示对齐，不随本变更改动）：

| API | 形状 |
|---|---|
| `Type.IsEnum` | `bool`（zbc TYPE 段 `CLASS_FLAG_ENUM`） |
| `Type.GetEnumUnderlyingType()` | 恒 `typeof(long)` |
| `Enum.GetNames(Type)` | `string[]` |
| `Enum.GetValues(Type)` | `long[]` |
| `Enum.GetName(Type, long)` | `string`（无匹配 → `""`） |
| `Enum.Parse(Type, string)` | `long`（大小写敏感；非成员 → 抛） |
| `Enum.IsDefined(Type, long)` | `bool` |

## 已知边角

- **未代换的类型形参位上的 enum 值是裸 i64**。`T Identity<T>(T x) where T: enum` 的返回值
  在擦除下没有盒，因此 `GetType()` 得 `Int32`、`"" + x` 得数字。这是泛型擦除的既有边界
  （`List<int>` 同款），不是 enum 特有；enum 盒的 `Equals` 因此**接受裸 i64 作右操作数**，
  否则 `Assert.Equal(Color.Green, Identity(Color.Green))` 会以用户看不见的理由为 false。
- **声明的底层类型被忽略**：`enum E : byte` 的 `: byte` 当前不生效，一律 i64。
  要真正尊重它需要在 TYPE 段 enum 块里持久化 —— 一次格式 bump，暂缓。

## 相关

- [模式匹配](pattern-matching.md) —— enum 的常量模式 / 关系模式（`>= Status.NotFound and < Status.ServerError`）
- [`runtime/struct-value-semantics.md`](../runtime/struct-value-semantics.md) —— 值类型的
  `GetType()` 折叠与装箱-struct Object 协议（enum 的 `GetType()` 折叠与之同源）
