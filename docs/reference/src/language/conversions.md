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
- 装箱/拆箱运行期机制：[语言部分 · 装箱](../../../design/language/boxing.md)
- 承载代码：[`z42c.semantics/README.md`](../../../../src/compiler/z42c.semantics/README.md)
