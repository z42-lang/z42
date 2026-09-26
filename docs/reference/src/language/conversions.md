# 类型转换分类器（Conversion classifier）

> 对齐：2026-08-12（tighten-implicit-conversions，PR2）｜ 代码：`src/compiler/z42c.semantics/src/Conversion.z42` + `TypeChecker.z42`

z42 的类型转换体系借鉴 C#（隐式 / 显式），但**比 C# 更严、更可预测**：隐式只允许**绝对无损**
的转换，任何可能丢信息或丢精度的转换都要求显式 `(T)` cast。本页描述承载这套规则的**分类器**
机制。

> **演进路线（三 PR）**：**PR1** 立分类器（分类并打标签，执行门宽松、与历史逐字节等价）；
> **PR2（已落地）** 收紧执行门（窄化 / 有损浮点在隐式上下文要求显式 `(T)`，含 C# 常量在范围内
> 例外）+ 为数值拓宽插 `ConvertInstr`（修 `double d=5` 表示 bug 与窄化不截断）；**PR3** 加用户
> 自定义 `implicit`/`explicit operator`。本页反映 **PR2 落地状态**。

## 为什么要一个分类器

历史上可赋性判定散在一堆返回 `bool` 的谓词里（`TypeFactsTc._isAssignable`、
`Z42Type.IsAssignableTo`、cast 绑定、`BoxIfNeeded`），只回答"能不能转"，**不携带**"这是哪种
转换 / 隐式还是显式 / 该调哪个转换方法"。收紧规则（PR2）和用户自定义转换（PR3）都需要这条
信息。分类器把判定集中到一处，并给每种转换打上**语义正确**的种类标签。

## 分类种类（`ConvKind`）

`Conversion.Classify(from, to, symbols)` 返回 `ConvResult{Kind, Method}`。`Kind` 取自：

| 种类 | 含义 | 隐式可赋（PR2 起）|
|------|------|:---:|
| `None` | 不存在任何转换 | ✗ |
| `Absorb` | 任一侧 error/unknown（防级联报错）| ✓ |
| `GenericErase` | 恰一侧泛型形参（类型擦除）| ✓ |
| `Identity` | 规范化同型（剥 `?` + 别名后名等价）| ✓ |
| `ImplicitNumeric` | 无损数值拓宽 | ✓ |
| `ExplicitNumeric` | 数值窄化 **或** 有损浮点 | ✗（要求 `(T)`）|
| `Boxing` | 值类型 → `object`/接口 | ✓ |
| `Unboxing` | `object`/接口 → 值类型 | ✗（要求 `(T)`）|
| `ImplicitRef` | 引用上转（派生→基、类→接口、`null`→引用、任意→`object`）| ✓ |
| `ExplicitRef` | 引用下转（基→派生）| ✗（要求 `(T)`）|
| `UserImplicit` / `UserExplicit` | 用户自定义转换运算符（PR3）| 隐式 ✓ / 显式 ✗ |

> **执行门（PR2 收紧）**：`ConvResult.ImplicitOk()` 是隐式可赋白名单——`{Absorb, GenericErase,
> Identity, ImplicitNumeric, Boxing, ImplicitRef, UserImplicit}`，**剔除 `ExplicitNumeric`**（`Unboxing`/
> `ExplicitRef` 本就不在）。`TypeFactsTc._isAssignable` 即其薄封装。窄化 / 有损浮点在隐式上下文由此
> 拒绝。`ImplicitOkPermissive()`（含 `ExplicitNumeric`）保留作 PR1 历史等价的参照，不再用于执行门。

## 隐式数值矩阵（比 C# 严）

采用 C# 的隐式数值转换矩阵，但**剔除会丢尾数精度的整数→浮点项**，令其归为 `ExplicitNumeric`：

| 转换 | z42 | C# | 理由 |
|------|-----|----|----|
| `int→long`、`byte→int`、`short→long` … | 隐式 | 隐式 | 整数拓宽无损 |
| `int→double`、`uint→double` | 隐式 | 隐式 | 32 位整数 < double 53 位尾数，无损 |
| `float→double` | 隐式 | 隐式 | 无损 |
| `char→int/uint/long/ulong/float/double` | 隐式 | 隐式 | char 是 21 位 Unicode 标量，无损 |
| **`int→float`、`uint→float`** | **显式** | 隐式 | 32 位 > float 24 位尾数，**丢精度** |
| **`long→float`、`ulong→float`** | **显式** | 隐式 | 丢精度 |
| **`long→double`、`ulong→double`** | **显式** | 隐式 | 64 位 > double 53 位尾数，**丢精度** |
| `long→int`、`int→byte`、`double→int` … | 显式 | 显式 | 窄化 |

> 这堵住了 C# 一个公认暗坑：`long l = 9007199254740993; double d = l;` 在 C# 里隐式且**静默失真**。
> z42 要求 `double d = (double)l;`，把"我知道这里会丢精度"显式化。

判定实现：`Conversion._widensLossless(fromCanon, toCanon)` 是这张无损表；不在表中且非同型的
数值对 → `ExplicitNumeric`。

## 装箱与拆箱（值类型 ↔ `object` / 接口）

`Boxing` / `Unboxing` 这两种分类对应的完整规则：

| 方向 | 规则 |
|------|------|
| 值类型 → `object` / 接口 | **隐式**可赋，转换保留**精确的**源类型 |
| `object` / 接口 → 值类型 | **显式**：`(T)o` 或 `o as T`，运行期受检 |
| `object[]` 的元素 | `a[i] = 5` 逐元素装箱；`(int)a[i]` 逐元素拆箱 |
| 引用类型 → `object` | 引用上转（归 `ImplicitRef`），不是装箱 |
| 数组协变 | **不支持** `int[] <: object[]`（避免 store-hole）|

**保留精确类型**是这套规则里唯一需要记住的东西：装箱**不会**把 `int` 悄悄变宽成 `long`。

```z42
object a = 5;      // int
object b = 9L;     // long
a is long          // false   ← 不是 C# 那种「反正都是整数」
b is long          // true
a.GetType().Name   // "Int32"
b.GetType().Name   // "Int64"
```

`enum` 同样保精度——装箱后仍是它自己的类型，不塌成底层整数（详见 [enum](enums.md)）：

```z42
object c = Color.Green;
c.GetType().Name     // "Color"
c.GetType().IsEnum   // true
c.ToString()         // "Green"
```

`struct` 值装箱后保持**引用身份**（与 C# 一致）：每装箱一次得到一个新的盒。

### cast 的目标可以是闭合泛型

`(T)x` 的 `T` 接受**带类型实参**的泛型类型，与 `as` 同口径：

```z42
object o = new GBox<int>(42);
GBox<int> g = (GBox<int>)o;              // 单实参
Pair<int, string> p = (Pair<int, string>)q;   // 多实参，逗号在 `<…>` 内
GBox<GBox<int>> n = (GBox<GBox<int>>)m;       // 嵌套（`>>` 正确拆开）
```

> ⚠️ **此前**只有 `x as GBox<int>` 合法，`(GBox<int>)x` 报 `E0202: expected ')'` ——
> 同一件事两种写法口径不一。
>
> `(f<int>)(x)` 仍解析为**泛型调用**而非 cast（与 `(f)(x)` 一致，这条歧义刻意留给调用）。
> 要在那种形状下转换，请用 `as` 或临时变量。

### 拆箱失败抛可捕获的异常

`(T)o` 在运行期核对盒里的精确类型，不符即抛，且**两种失因分开**：

| 情形 | 异常 |
|---|---|
| `o` 是 null，`T` 是**值类型** | `NullReferenceException` |
| `o` 是 null，`T` 是**引用类型** | **不抛**，结果是 null（C# 同） |
| `o` 非 null 但类型不符 | `InvalidCastException` |

```z42
object o = "hello";
try { int n = (int)o; }
catch (Exception e) { Console.WriteLine(e.GetType().Name); }   // InvalidCastException
```

「没有对象」与「对象类型不对」是两种不同的错，分成两个异常种类是为了让调试时一眼看出是哪一种。

> ⚠️ **此前**（make-hard-cast-fail-properly 之前）这两种都是**终止性内部错误，`catch` 捕获不到**，
> 消息还是 Rust 调试格式（`cannot convert Str("hello") to type tag 0x04`）。
>
> ✅ **引用类型之间**的硬转换（`(Box)someOther`）**现在也受检**（make-ref-hard-cast-checked）：
> 类型不符抛 `InvalidCastException`，`null` 照 C# 语义放行。此前它在发射层**一条指令都不发**，
> 错类型的对象原样流下去、到很远的地方才以别的面目崩。

**健全性**：装箱 = 加宽上转（安全）+ 受检下转（运行期核对精确类型）。因为装箱值携带精确类型，
下转可靠、`is` / `as` 精确——没有办法把一个类型当成另一个用。引用类型之间的硬转换同样受检。

> 想要「不符就给 null」而不是抛，用 `as`：两者的分工是**明确要求** vs **试一下**。
> `(T)x` 说的是「它就是 T」，说错了应当立刻响；`x as T` 说的是「是 T 的话给我」。

> 只有把值赋给 **`object` 或接口**才发生装箱。赋给泛型形参（`List<int>` 的元素）不装箱，
> 容器里外的表示不变。

## 用户自定义转换（User-defined conversions，PR3 `add-user-conversions`）

用户可用 C# 同款语法声明转换运算符，**并修掉 C# 的几处设计硬伤，令 z42 更严更可预测**：

```z42
class Celsius {
    public int Deg;
    public Celsius(int d) { this.Deg = d; }
    public static implicit operator int(Celsius c) { return c.Deg; }      // 隐式：Celsius → int
    public static explicit operator Celsius(int d) { return new Celsius(d); }  // 显式：int → Celsius
}

Celsius c = new Celsius(25);
int x = c;                 // 隐式：赋值/return/传参协变点自动调 op_Implicit → 25
Celsius c2 = (Celsius)30;  // 显式：(T)x 调 op_Explicit → Celsius(30)
int y = (int)c2;           // (T)x 亦接受 implicit → 30
```

### 比 C# 更好的三处（z42 改进）

| # | C# 的坑 | z42 的改进 | 落点 |
|---|---------|-----------|------|
| ① | 隐式转换 `(T)x` 语义不对称 | `(T)x` 同时接受 implicit 与 explicit 用户转换；隐式上下文只接受 implicit，explicit-only 报 E0439「缺 cast？」 | `_bindCastExpr` / `CheckImplicitConvert` |
| ② | 转换冲突推迟到**调用点**才报 | **声明期**冲突检测（E0440）：同 (源→目标) 重复、或 implicit+explicit 同对 → 声明处即报错 | `SymbolCollector`（`convSeen` 表） |
| ③ | 多跳转换不提示中间类型 | A→C 无直接转换但 A→B→C 存在 → 报错追加「a conversion through 'B' exists — write (C)(B)x」 | `TypeChecker._suggestVia` |

**v1 精确匹配、不组合链（比 C# 更可预测）**：用户转换要求 (源,目标) 与运算符签名逐字匹配，**不做** C# 的
「标准转换 + 一个用户转换 + 标准转换」组合链——消除 C# 里「到底走哪条链」的不确定，多跳由 ③ 诊断引导手写。

### Deferred

- `as` / `is` / 模式匹配接入用户转换（可失败语义，`user-conversions-future-as-is`）。
- 标准转换 + 用户转换的组合链（`user-conversions-future-conversion-chain`）。

## 验证

- **单测** `src/compiler/z42c.semantics/tests/conversion/`：分类器种类标签（PR1）+ 收紧门布尔投影
  `ImplicitOk()` + E0439 拒绝（非常量窄化 / `long→int` / 有损浮点）+ 常量例外接受/越界拒绝
  （`byte b=48` ✓ / `byte b=300` ✗ / `sbyte s=-1` ✓）+ 拓宽插 `(convert …)` 节点。
- **自举字节不动点**：`ConvertIfNeeded` 不触达 z42c 自身 codegen（其源无隐式 int↔float 拓宽），
  gen1==gen2 逐字节相同；全 golden / stdlib / cross-zpkg 绿。
- **迁移面为零**：常量在范围内例外覆盖了 stdlib 全部窄化点（binary-format writer 的在范围常量），
  z42c 源亦无真窄化点——PR2 未改一处 stdlib / z42c 源（仅修一个 int-vs-double 松比较的 math 测试）。

## 关联文档

- 引入/演进：change `add-conversion-classifier`（PR1）、`tighten-implicit-conversions`（PR2）、`add-user-conversions`（PR3，用户自定义转换 + ②③ 改进）——均已落地
- [enum](enums.md)——枚举值装箱后的类型身份
- [结构体](structs.md)——值类型的复制语义
- 承载代码：[`z42c.semantics/README.md`](../../../../src/compiler/z42c.semantics/README.md)
