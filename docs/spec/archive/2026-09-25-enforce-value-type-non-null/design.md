# Design: 值类型永不可空

## Architecture

```
编译期                                     运行期
────────────────────────────────          ────────────────────────────────
int x = null;          → 错误             字段/静态字段/数组元素/struct ref 槽
intExpr == null        → 错误               按声明类型 tag 零初始化（6 处）
int? / T?(值类型)      → 错误             object → 值类型拆箱：
NullableType 擦除点分流：                    ① null   → NullReferenceException
  值类型 → 报错                              ② 类型不符 → InvalidCastException
  引用类型 → 保持擦除（不变）
```

**不变式：值类型的存储槽永不含 `Value::Null`。** 编译期堵住"写 null 进去"，
运行期堵住"从未初始化状态读出 Null"，拆箱点堵住"从 object 侧漏进来"。

---

## Decisions

### D1：在写入侧根治，不在读取侧兜底

历史上三次补丁全在**读取侧**：`symres.rs` 读静态字段时发现是 Null 就补零；
`exec_array.rs` 读数组槽时按元素 tag 补零。它们各自有效，但根在写入侧——
`vec![Value::Null; n]` 不看声明类型。

**本变更把零值填在分配点**，6 处站点统一改为按字段 tag 取 `default_value_for_tag`
（`metadata/types/field.rs:92`，非泛型数组路径已在用同一函数）。

已有的读取侧补丁**保留不删**——它们是纵深防御，且删除会扩大本变更的爆炸半径。
但注释里标注"根因已在分配点修复，此处为兜底"。

### D2：JIT 必须同步

`jit/frame.rs:138` 与解释器的帧槽初始化是两份代码。**只改一份 = 解释器/JIT 行为分叉**，
而分叉的表现是"同一段代码在 tier-up 之后结果变了"，极难定位。

⇒ tasks 把两处绑在同一步，且验证必须覆盖 JIT 路径（`jit-fixpoint` 类用例）。

### D3：拆箱两段检查 —— 顺序与异常类型

```
object o → 值类型 T
  ① o is Null?     → NullReferenceException  「cannot unbox null to `int`」
  ② o 的类型 ≠ T?  → InvalidCastException     「object holds `string`, not `int`」
```

顺序不能反：null 没有类型，先查类型会得到一条误导的消息。

**这与 #717 的方向相反**，记录理由：#717 把拆箱遇 Null 从内部错误改成静默返回 null。
当时的判断是"让一个已存在的错误状态不再抛内部错误"。实际效果是**把警报关掉**——
原先至少会响一声。本变更恢复"响"，但抛的是**带源位置的用户级异常**，而不是内部错误，
两者兼得。

零初始化落地后，这条路径上的 null 只可能来自"真的有人把 null 装进了 object"，
那正是该响的时候。

### D4：`int?` 直接报错，不做真 `Nullable<T>`

**选项：**

| | 做法 | 代价 |
|---|---|---|
| A | 真 `Nullable<T>`：HasValue/Value、装箱语义、lifted operators、模式匹配 | 一个 minor 的主线特性；且引入"值类型能为空"的例外，与本变更的不变式冲突 |
| B | `int?` 降级为装箱 `object` | 语义怪、性能怪 |
| **C（选定）** | `T?` 中 T 为值类型 → **编译错误**，附迁移提示 | 需要迁移 4 个 stdlib API |

选 C 的关键理由：**它让不变式无例外**。有了例外，运行期就必须重新引入"值类型槽可能是 Null"
的可能性，D1/D2/D3 全部退化为尽力而为。

迁移成本实测极小：4 个生产 API + 3 个测试文件。

### D5：TryParse 规约 —— 按返回类型分流

| 返回的是 | 形式 | 为什么 |
|---|---|---|
| 引用类型 | `V? Find(...)` 单返回值，**不要 bool** | 引用类型自带"缺席"的表示 |
| 值类型 | `bool TryX(..., ref T v)`，失败写零值 | 值类型不能为空，才需要第二个通道 |

分流之后 **`bool` 与可空值的组合永远不会出现** ⇒ 不需要 C# 的 `[NotNullWhen(true)]`，
也不会出现"我检查了 bool，编译器还要我检查值"的双重检查。

`IPAddress.TryParse`（`IPAddress?`）与 `ProcessHandle.TryWait`（`ProcessResult?`）
本就踩在正确一边，是规约的正面样本。

**已知损失**：`ref` 出参没有"callee 必须赋值"的编译期保证（`out` 已砍，见
`simplify-ref-parameters`）。失败路径显式写 `v = 0` 是约定而非强制。

### D6：值类型 `== null` 报错，而非静默恒假

静默恒假是最坏的形态：代码看起来做了检查，实际那个分支永远不执行。
（#715 `==` 恒假是同一族。）

⇒ 编译期直接拒绝，消息提示"值类型永不为 null，此比较无意义"。

**边界**：泛型 `T` 未约束时既可能是值类型也可能是引用类型 ⇒ **不报**（保守）。
`object` 是引用类型 ⇒ `objExpr == null` 合法。

### D7：错误码

| 码 | 条件 |
|---|---|
| `NullToValueType` | `null` 赋给 / 传给值类型 |
| `ValueTypeNullComparison` | 值类型与 `null` 比较 |
| `NullableValueTypeNotSupported` | `T?` 中 T 是值类型；消息带迁移提示（`int? F()` → `bool F(ref int)`） |

运行期两条异常复用现有 `NullReferenceException` / `InvalidCastException` 构造路径。

### D8：摸底诊断（Q1）

零初始化唯一的静默风险：现存代码用 `f == null` 检测"值类型字段没被设过"。
grep 查不出来（按名字匹配全是同名引用字段的误命中）。

**做法**：先只加 D6 的诊断（值类型 `== null` → 报错），**不改运行期**，跑全仓构建。

- 命中数为 0 ⇒ 风险不存在，直接推进
- 有命中 ⇒ 逐个人工判读：是"检测未设置"（真风险，要改写）还是"冗余检查"（直接删）

这一步**必须在零初始化之前**，否则风险已经被静默吃掉了。

---

## Risks

| 风险 | 缓解 |
|---|---|
| **Q1 未知数** —— 现存 `f == null` 值类型哨兵 | D8 的摸底诊断，先诊断后改运行期 |
| 解释器 / JIT 分叉 | 两处绑同一步；验证覆盖 JIT 路径 |
| 自举 | stdlib TryParse 改签名，`Version.z42:83` 是唯一调用点；按 `bootstrap-seed.md` 走冷种子 |
| 依赖 `simplify-ref-parameters` | 若该变更未落地，TryParse 改用元组 `(bool, T)`（人体工学较差） |
| 泛型 struct 数组仍有 Null 槽 | 已知未堵，单列 follow-up（强推 struct backing 会打坏泛型容器） |

**不涉及**：zbc/zpkg 格式 bump（运行期行为变化，不改 wire 布局）、
golden 指令流（零初始化在运行期，不产生 IR 指令）。
