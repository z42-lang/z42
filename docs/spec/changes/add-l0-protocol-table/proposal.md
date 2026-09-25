# Proposal: L0 协议表 —— 把「什么形状算可迭代/可释放」变成数据

> 状态：🔵 DRAFT 待审批 ｜ 类型：lang（需规范先行）｜ 创建：2026-09-25
> 前史：本 change 是 `foreach-protocol-pluggable-syntax-program` 的**批 1**。批 0（#783）已落地。
> 旧稿（三周前写在 scratchpad 里）有多处断言已过时，本稿按 2026-09-25 的 main 重核后重写。

## Why

### 1. 目标：机制不是散落的 if 链和字符串字面量

像「某个语法糖绑定到某个协议、编译期展开成已有形状」这类机制（`foreach`、将来的 `using`），
今天每一个都是**散落在 if 链和字面量里的一次性实现**。加一个要改好几处，关一个做不到，
换绑定对象更做不到。

目标：把「什么形状算可迭代 / 可释放 / 可比较」收敛成**一张表**，每条绑定自带**有序策略链**
（D3 裁决：每协议各自配置，不是全局布尔开关）。扩展点开在**内核内** —— 加一个同类机制 =
加表项 + 实现 handler，**不开放用户自定义语法**（`attribute-handler-registry` 的 D1/D7 不动）。

### 2. 现状：知名名字的硬编码散在多处，z42 侧零集中表

Rust 侧有 `src/runtime/src/metadata/well_known_names.rs`（常量 + 查表函数 + 抬头写明「C# 侧
对照物须一致」的镜像纪律）；**z42 编译器侧没有对应物**。

**本 change 只收「协议」语义那一类**（User 2026-09-25 裁决：表的范围只收 foreach/using 这类
真协议；`Std.Object` / `ValueTupleN` / `MethodInfo` / 基元包装名那些**纯类型名**另立小 PR 搬运）。
按 2026-09-25 重核的事实：

| # | 客户 | 硬编码 | 位置 |
|---|---|---|---|
| 1 | foreach 枚举器路径 | `GetEnumerator` / `MoveNext` / `Current` / `Dispose` | `StmtBinder.z42:31,102,105,106` + 条件化后的 `ForeachProtocol.EnumeratorDisposable` |
| 2 | foreach 索引路径 | `get_Item`（3 处仍在 StmtBinder：`:44,45,64`）；`Count`/`Length` **已收敛** 到 `ForeachProtocol.CountOf` | `ForeachProtocol.z42:56,98,99` |
| 3 | **event 降糖（在语法层！）** | `IDisposable` ×2 / `Disposable.From` / `InvalidOperationException` + 消息字面量 / `Subscribe`·`Unsubscribe` / `DelegateOps.ReferenceEquals` / `add_`·`remove_` 前缀 / `MulticastAction`·`Func`·`Predicate` / **handler 委托名映射** / **auto-init `new MulticastXxx<>()`** | `MemberParser.z42:14,20-22,26-28,33,49,57-59,65,66,71,76,83,239`；调用点 `DeclParser.z42:195-212` |
| 4 | prelude 内建接口 | 11 个接口与 `z42.core/src/Protocols/*.z42` 要求「逐字锁步」，只有注释在守 | `BuiltinTypeDefs.z42:29-85` |

第 3 项最严重：**语法层直接依赖了 stdlib 的具体实现类**，且它比旧稿记的多两处
（handler 委托名映射、语法层替用户合成 `new MulticastXxx<>()`）。

### 3. 特性开关仍是死旋钮，而这条接线路径**反复断在同一处**

- `LanguageFeatures`（`z42c.core`）**仍零调用方**（2026-09-25 复核：只有它自己的单测引用）。
- `E0301 FeatureDisabled` **仍零发射点**（`error-codes.md:106` 自己标着「⚠️ 零发射点」）。
- `z42.toml` **没有** `[syntax]` 段；**没有** `ParseTable` 类。

⭐ **这不是孤例**：同一条「manifest 段 → 编译器」的接线路径已经断过三次 ——
`[build] incremental`（别人已修，`wire-build-incremental`）、`[optimize]`（本轮修，见下）、
`[[example]]`/`[examples]`（待裁）。见 [[dead-manifest-knobs-family]]。
**`[optimize]` 接线（PR #819）刚把这条路踩通一遍，`[syntax]` 可以照抄**：
中性 name/value 对进 manifest 模型 → 消费方解释 → 唯一一次 resolve 落在 manifest 在手的那层 →
**旋钮效果必须进 cache key**（否则全量生效、增量被忽略）→ 加一道**取产物字节/行为**的判据门。

### 4. 为什么现在做：条件步骤终于有了真实样本

D3 要求「有序策略链」，而直到 2026-09-25 之前 foreach 三条路径**全是无条件的** —— 表里没有
任何东西体现「条件」。`fix-foreach-dispose-optional`（PR #823）把 `Dispose` 改成条件步骤后，
foreach 协议的真实形状才是：

```
数组 → 索引面（get_Item(int) + Count→Length 两档）→ 枚举器（GetEnumerator → MoveNext/Current → Dispose 若可 dispose）
```

这正是表要表达的东西。在此之前做表，只会把三个无条件分支平铺成三行、看不出「链」的必要性。

## What Changes（User 裁决 = C 档）

1. **L0 协议表**：把上表 #1–#4 的协议类名字收敛成一张集中表，每条绑定自带有序策略链。
2. **特性门接线**：`z42.toml [syntax]` → `LanguageFeatures` → 解析器，让 **E0301 首次发射**
   （关掉某特性 ⇒ 用该语法必须报错）。照 #819 踩通的路径。
3. **event 降糖搬出 parser**：语法层不再直接引用 stdlib 具体类。

## Non-goals

- **不做第 3 层用户定义语法**（`operator` / `keyword` 声明）——roadmap 的 Deferred 不动，D1/D7 不推翻。
- **不做 L2 ParseTable**（`_infixBp` / `ParseStatement` if 链表化）——那是批 2。
- **不做 `using` 语句**——批 3/4。
- **不碰后端**：不新增 AST 节点语义 / IR 指令，不 bump zbc/zpkg 格式。
- **不改 foreach 现有行为**（本 change 只换绑定来源；行为与字节均不变，靠自举不动点与 golden 守）。
- **不收纯类型名**（`Std.Object` / `ValueTupleN` / `MethodInfo` / 基元包装名三份）——另立小 PR。
- **不收 `op_*` 映射表**（`MemberParser._operatorMethodName` 与 `TypeFactsTc._operatorMethodNameTc`
  两份，16 条各写一遍）。它确实是「协议名的两份拷贝」，但**两份的形状不同**（一份是符号→名字、
  另一份是名字→派发键，回落分支也不一样，#795 刚各修过一次），收敛它要先定「谁是 SoT」，
  是独立一条。混进本批会让「字节必须不变」这条对不上账。

## 裁决（已定）

| # | 议题 | 裁决 |
|---|---|---|
| D1 | 粒度 | **L0 协议表 + L2 ParseTable**；扩展点开在内核内，不开放用户自定义语法 |
| D2 | `using` 落法 | 按内建支持，但**经语法扩展机制实现**（注册表项，非新增 if 分支） |
| D3 | 协议匹配语义 | **每协议各自配置**的**有序策略链**（不是布尔开关）—— C# 自己就是这样 |
| D4 | `using` 形态 | 两种都要：`using (r) { }` + `using var r = ...;` |
| D5 | 批 2 范围 | keyword-led 与 ambiguous-lookahead 一起表化，顺序约束从注释变数据 + 守门 |
| D6 | 批 1 边界 | **C 档**：表 + 特性门接线 + event 降糖搬出 parser（2026-09-24 裁） |
| D7 | 表的范围 | **只收真协议**（foreach/using 这类）；纯类型名另立 PR（2026-09-24 裁） |

## 相关

- [[dead-manifest-knobs-family]]：特性门接线要走的那条路径已断过三次；#819 踩通的做法照抄。
- `fix-foreach-dispose-optional`（#823）：条件步骤的第一个真实样本。
- `docs/internals/src/compiler/syntax-customization.md`：三层设计页。
  ⚠️ 它的「实施路径」**自相矛盾**（正文写「分三批」、表里列 4 批、后文又写「批 1–3 落地后」）——
  本 change 落地时顺手订正，见 tasks。
