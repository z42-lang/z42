# Proposal: 类型名与内建基元拼写冲突要报错

## Why

今天这段代码**编译期一条诊断都没有**，运行期崩：

```z42
namespace Demo;
struct Single { public int X; public int Y; }

public static void Main() {
    Single s = new Single();
    s.X = 5;                       // Error: FieldSet: expected object, got F64(0.0)
}
```

用户声明的 `Demo.Single` 被当成了基元 `float`。

### 这不是符号遮蔽，是**纯拼写折叠**

`PrimModel.Canon` 是个**纯字符串函数**（`z42c.semantics/src/PrimModel.z42:28-60`）：
它把 14 个 PascalCase 拼写映射到关键字，**不看命名空间、不看包、不看有没有 import**。

```
Byte SByte Int16 Int32 Int64 UInt16 UInt32 UInt64 Single Double Boolean Char String Object
```

⚠️ 它还会先剥掉 `Std.` 前缀 ⇒ `Std.Single` 与裸 `Single` 折到同一个结果。

### 故障链（四步，每步单独看都合理）

| # | 位置 | 做了什么 |
|---|---|---|
| 1 | `PrimModel.Canon` | `"Single"` → `"float"`；`IsScalarValue(n) = Code(Canon(n))` ⇒ 判成标量基元 |
| 2 | `StructLayout.BuildFromSymbols:163` | `ct.IsStruct && !IsScalarValue(keys[i])` ⇒ 用户这个类型**根本不进 `_defs`** |
| 3 | `ClassDescBuilder:326` | `Layouts.IsStructType(layoutKey)` 于是 miss ⇒ `StructSize` / 逐字段表**一个都不填** |
| 4 | `ZbcWriter:399` | 但仍按 `Flags & 4`（struct 位）**无条件**写出 struct 块 ⇒ 落地成 **size = 0 的空块** |

⇒ `IsBlobStruct` 因 `li.Size == 0` 返回 false ⇒ codegen 不走 blob 值语义，
整个类型被当成那个基元。

### 为什么 z42.core 自己没坏

因为对它而言**这个折叠是对的**：`z42.core` 的 `Std.Single`（`src/libraries/z42.core/src/Primitives/Single.z42:9`）
**就是** `float` 的包装类型。同理 `Std.Object` / `Std.String` / `Primitives/*.z42` 共 14 个。
⇒ 规则不是「这些名字有毒」，而是「**这些名字归 `Std` 所有**」。

## 实测边界（本提案的全部范围判据都来自这里）

工具链 = main `dddf8df76` 重建。

| 形态 | 结果 |
|---|---|
| 顶层 `struct <14 个名字之一>` + 写字段 | 🔴 **14/14 全崩**（`FieldSet: expected object` / `StructCopy src: expected a struct`）|
| 顶层 `class <同上>` + 写字段 | 🔴 崩 |
| `enum Single` | 🟢 正常 |
| `interface Single` | 🟢 正常 |
| **嵌套** `class Outer { struct Single {…} }` | 🟢 正常（符号表键是 `Outer+Single`，不被折）|
| 零字段 `struct Single { }` | 🟢 正常（没有字段可访问）|
| 声明了但从不碰字段 | 🟢 正常（崩点在字段访问）|
| 关键字拼写 `struct float` | 🟢 **已被语法挡住**（E0202 expected type name）|
| 对照 `struct Solo` | 🟢 正常 |
| 用户在 `namespace Std` 里声明**新**类型 | 🟢 合法（`Std` 不是保留命名空间）|
| 用户在 `namespace Std` 里声明 `Single` | 🟢 **现有 E0606 已经报**（同 FQN 遮蔽导入）|

⇒ **需要新诊断的范围就一条**：**顶层** `class` / `struct`，名字是那 14 个之一，且不在 `Std`。

## What

新增一条诊断：**除 `Std` 外，`class` / `struct` 的名字不得与内建基元的 PascalCase 拼写相同。**

- 级别 **Error**（不是 warning）——今天它 100% 导致运行期崩或错值，没有「有意为之」的合法写法。
- 落点 `z42c.semantics/src/DeclEnforcer.z42`（「声明良构约束簇，纯语法/AST 级检查」），
  与既有的 `_passAttributeSuffixEnforce`（E0444/E0445/E0447「改名」族）同一形状。
- 消息给出**可操作的出路**：改名。

## 豁免判据：**声明所在的命名空间是 `Std`**

不用包身份（`SelfPkg` / `DepScan.IsPrelude`），理由三条：

1. **它就是事实上的规则**。`Canon` 自己剥 `Std.` 前缀 —— 语言早已认定 `Std.X` 是这些名字的家。
2. **逃生口已经堵着**。实测：用户把 `Single` 声明进 `namespace Std` ⇒ **现有 E0606** 当场报
   （那 14 个 FQN 在 z42.core 里都存在）。不需要本提案再管。
3. **包身份在两处不可用**：单文件 `--emit-zbc` 编译没有工程 ⇒ `SelfPkg` 为空，
   而那**正是本缺陷最容易发生的场景**；用 `SelfPkg == ""` 豁免等于把门关在最需要它的地方。

## 已知会被判红的既有代码（IMPL 必须一并处理）

`src/tests/` 里那一处已在 #947 改名（`constraint_value_vs_ref.z42` 的 `Single` → `Solo`）。
剩下 **4 个编译器单测文件**用内联源码串自声明这些类型，**目的就是冒充 z42.core**：

- `z42c.semantics/tests/typecheck/prim_member/prim_member_tests.z42:29-32, 72, 82, 84`
  （`struct Int32` / `struct Double` / `struct Boolean` / `class String`）
- `z42c.semantics/tests/typecheck/bare_type_param_member_tests.z42:21`（`class Object`）
- `z42c.semantics/tests/typecheck/generic_inference/generic_inference_tests.z42:304, 334`
  （`struct Int32` / `struct String`）

它们的片段**没有 namespace 声明**。拟改法：给片段加 `namespace Std;` ——
这让它们**更忠实于所模拟的对象**（z42.core 正是在 `Std` 里声明它们的），
而不是给测试开一个后门。

⭐ 这些片段走的是 `SemanticDump.FirstErrorCode` 那条**不链 stdlib** 的独立 Infer 路径
（`StmtBinder.z42:343` 明写），所以加了 `namespace Std;` 也不会撞上 E0606（没有 import 可遮蔽）。

## 明确不做

- **不改 `PrimModel.Canon`**。让折叠变成命名空间感知的才是「根治」，但 `Canon` 的结果同时是
  **派发键 / 查找键**（[[z42-nullable-types-line]] 与 #791 / #866 两次独立撞到过「不能走类型拼写」），
  在热路径上、全仓消费。收益是「让 `Demo.Single` 能用」这个几乎无人需要的能力，
  风险是全局的。**不值得。**
- **不管 `enum` / `interface` / 嵌套类型**——实测不受影响，为它们加规则是在凭空扩大语言约束。
- **不碰 `ZbcWriter` 那个「无条件写空 struct 块」**。它现在是**信号**（#947 的覆盖门正是靠
  `size = 0` 把这个缺陷抓出来的）。诊断落地后它不再可达，但删它是另一件事。

## 影响面

- **语言规则**：新增一条命名约束 ⇒ 这是 `lang` 类变更，走完整流程。
- **向后兼容**：会**拒绝今天编得过的代码**。但那些代码 100% 在运行期崩或产出错值
  ⇒ 把静默错答案换成编译错误，方向与本仓一贯立场一致。
- **格式**：无 zbc / zpkg 改动。
- **自举**：新诊断只拒绝 stdlib 与编译器自身都不写的形态（z42.core 在 `Std` 里，已豁免）
  ⇒ 不涉及分阶段引入纪律。

## 待裁决

1. **错误码取号**。类型检查带空号只剩 `E0491` 与 `E0499`（两者都**不在** `error-codes.md` 表里
   ⇒ 都是真空号；本仓既定做法就是扫登记表找空位）。建议 **`E0499`**。
2. **4 个 fixture 的改法**：加 `namespace Std;`（推荐）vs 给测试留豁免口。
