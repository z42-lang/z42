# 类型转换分类器（Conversion classifier）

> 对齐：2026-08-11（add-conversion-classifier，PR1）｜ 代码：`src/compiler/z42c.semantics/src/Conversion.z42`

z42 的类型转换体系借鉴 C#（隐式 / 显式），但**比 C# 更严、更可预测**：隐式只允许**绝对无损**
的转换，任何可能丢信息或丢精度的转换都要求显式 `(T)` cast。本页描述承载这套规则的**分类器**
机制。

> **演进路线（三 PR）**：本页描述的分类器是 **PR1** 的产物——它把转换**分类并打标签**，但
> 执行门暂时保持宽松（与历史行为逐字节等价）。**PR2** 收紧执行门（窄化 / 有损浮点要求显式 +
> 为数值转换插 `ConvertInstr`），**PR3** 加用户自定义 `implicit`/`explicit operator`。本页随各 PR
> 增补；当前反映 PR1 落地状态。

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

> **PR1 的宽松门**：`ConvResult.ImplicitOkPermissive()` 临时把 `ExplicitNumeric` 也算隐式可赋，
> 以等价于历史"数值窄化信任程序员放行"的行为，保证 PR1 产物字节不变。PR2 从白名单剔除
> `ExplicitNumeric`（及要求 `Unboxing`/`ExplicitRef` 显式），窄化才真正要求 cast。

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

## 机制 / 实现

`Conversion.Classify` 的判定顺序（短路），镜像历史 `_isAssignable` 的分支序以保证 PR1 布尔等价：

```
1. 任一侧 error/unknown          → Absorb
2. 恰一侧泛型形参                 → GenericErase
3. to == object                  → 值 prim 源 Boxing；否则 ImplicitRef
4. 两侧数值 prim                 → 数值矩阵（Identity / ImplicitNumeric / ExplicitNumeric）
5. from.IsAssignableTo(to)       → Identity（同名类 / 接口 / 数组 / func / 别名 prim）
6. class/instantiated → 基/接口  → 命中 symbols 上转查询则 ImplicitRef；下转则 ExplicitRef；否则 None
7. object/接口 → 值 prim         → Unboxing
8. 否则                          → None
```

> **关键设计**：数值 prim 对（步 4）**提前到结构判定（步 5）之前**——否则有损拓宽（`int→float`）
> 会被 `IsAssignableTo`（其 `_canWiden` 判其为拓宽）笼统当成 `Identity`，丢掉"有损"信息。提前后
> 数值对一律走细粒度矩阵。这不改 PR1 的布尔投影（数值对无论哪种都落在宽松门白名单内），只让
> **种类标签正确**，为 PR2 的收紧提供准确依据。

`_isAssignable(from, to, symbols)` 现在就是 `Classify(from, to, symbols).ImplicitOkPermissive()`
的薄封装。cast 绑定、`BoxIfNeeded`、codegen 在 PR1 **不经**分类器（它们要到 PR2/PR3 有行为变化
时才接入），以把 PR1 严格限定为"零行为变化的基础设施"。

## 验证

PR1 的正确性由**自举字节不动点**兜底：用新 z42c 自编译 z42c 源码两代，gen1 与 gen2 产物逐字节
相同，且全部 golden / stdlib / cross-zpkg 测试输出零变化——证明分类器的布尔投影与历史
`_isAssignable` 逐位等价。分类器种类标签由 `src/compiler/z42c.semantics/tests/conversion/` 单测覆盖。

## 关联文档

- 引入/演进：change `add-conversion-classifier`（PR1）；后续 PR2（收紧+迁移）、PR3（用户自定义转换）
- 装箱/拆箱运行期机制：[语言部分 · 装箱](../../design/language/boxing.md)
- 承载代码：[`z42c.semantics/README.md`](../../../src/compiler/z42c.semantics/README.md)
