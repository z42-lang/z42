# Design: L0 协议表

> 状态：🔵 DRAFT 待审批 ｜ 前置：[proposal.md](proposal.md)

## 已定的落位事实（实测，不必重查）

| # | 事实 | 依据 |
|---|---|---|
| L1 | **`z42c.core` 是 `z42c.syntax` 与 `z42c.semantics` 唯一共同可见层** | deps 实测：core 无依赖 ← syntax 依赖 core ← semantics 依赖 core+syntax |
| L2 | 名字要被**语法层**用到（event 降糖在 `MemberParser`）⇒ 名字常量**只能**放 `z42c.core`（或 `z42.ir` 叶子库） | 由 L1 推出 |
| L3 | 判定要用 `Z42Type` / 成员表 / 布局 ⇒ **判定只能在 semantics** | `ForeachProtocol` 现状 |
| L4 | `z42c.core` 里**没有 enum、没有泛型字段** | 该包 6 个文件；`DiagnosticSeverity` 注释明说「z42 暂无 enum → static class + int 常量」 |
| L5 | 集中表的在仓先例三种形态 | `DiagnosticCodes`（static class + 串常量）/ `LanguageFeatures`（并行数组）/ `IrModule` 哨兵（常量 + 查询函数）/ `HandlerRegistry`（名字 + `Is*` 谓词） |
| L6 | `E0301` 已分配且零发射点；`LanguageFeatures` 零调用方；无 `[syntax]` 段；无 `ParseTable` | 2026-09-25 复核 |

⇒ **分层结论（无争议）**：**名字在 `z42c.core`，判定在 `z42c.semantics`。**
这一层拆分同时解决 proposal 的 #3（语法层不再写 stdlib 具体类名，改引用 core 的常量）。

> 🔴 **L7（2026-09-25 实测补，本 change 的一条硬约束）：判定问的是「哪张成员面」，这件事本身
> 就是缺陷来源，而表里存不下它。** `ForeachProtocol.MethodsOf` 给的是类型**自己声明**的那张直接表；
> `Z42InterfaceType.Methods` **不含**父接口成员、`Z42ClassType.Methods` **不含**基类成员
> （继承查找一律经 `InterfaceClosure` / 沿 `BaseName` 链走，见 `MemberResolver._hasUserExpect`）。
> `EnumeratorDisposable` 第一版查了直接表 ⇒ 经接口静态类型 foreach 时 `disposed=0`
> （具体类型那条路是 1），**静默跳过真该调的 `Dispose`**；本包声明的同形状接口链一样中招
> ⇒ 与跨包/prelude 无关。
>
> 这正是 D-new-1 里「甲的 🔴 是表达力问题」的实证：一条协议步骤真正的内容不是「查 `Dispose`」，
> 而是「**沿哪条继承面**查 `Dispose`，查不到时保守还是激进」。压成串必然丢掉后半句。
> ⇒ 本 change 的 `Resolve` 负责顺序，**每一步「问哪张面」的判断留在判据函数里、与其事故注释同处**。

## D-new-1（**已裁决：形状 乙**，2026-09-25 User 裁）：策略链做成「数据」还是「一处集中的代码」？

D3 裁定协议匹配是**有序策略链**、不是布尔开关。但「链」以什么形态存在，是本 change 最实质的岔口。

### 形状 甲 —— 链是**数据**（编码成串，判定器解释）

```
// z42c.core
ProtocolTable.Foreach = ["shape:array", "shape:get_Item(int)+Count|Length", "shape:GetEnumerator", "cond:Dispose"]
```
semantics 侧写一个解释器按序试。

- ✅ 最贴近 D3 字面：链真的是一份可读、可配、将来可由 `[syntax]` 开关裁剪的数据。
- ✅ 加同类协议 = 加一行数据 + 实现该 step 的 handler。
- 🔴 **判定条件表达不出来**：`IsIntIndexer` 要看 `get_Item` 首参是不是整数、`CountMember` 要分
  「字段 / auto 属性存储 / 计算属性访问器 / 只有 `get_X` 的导入形态」四种取法、`EnumeratorDisposable`
  要从 `GetEnumerator` 返回类型再取一次成员面。把这些压成串 ⇒ 要么串语法无限膨胀，要么退化成
  `"shape:custom1"` 这种**指回代码的占位符**（那就不是数据了）。
- 🔴 **会丢注释**：现在 `ForeachProtocol` 每条判定上方都挂着「为什么是这个判据」的事故记录
  （Dictionary 被误判走索引路径跑满 Count 轮取到 0、属性写成 `Count` 时读空值静默越界、
  string 字面量的轻量合成体查不到成员…）。搬进串里这些**没有地方可写**，而它们正是判据的价值。

### 形状 乙 —— 名字是数据、**链是一处集中的代码**（推荐）

```
// z42c.core：名字/前缀常量（语法层与语义层共用这一份）
ProtocolNames.GetEnumerator / MoveNext / Current / Dispose / GetItem / Count / Length
ProtocolNames.IDisposable / DisposableFrom / MulticastAction / AddPrefix / RemovePrefix / …

// z42c.semantics：每协议一个函数，**链的顺序在函数体里一眼可见**
ForeachProtocol.Resolve(t) -> ForeachPlan     // 数组 → 索引面 → 枚举器（含 Dispose 条件步）
```

- ✅ 名字有唯一 SoT ⇒ 消掉 proposal 里全部漂移风险（`Count`/`Length` 那次分叉、`op_*` 两份、
  语法层写 stdlib 类名）。
- ✅ 链的**顺序集中在一个函数里**（今天散在 `StmtBinder._bindForeach` 的 if 嵌套 + `ForeachProtocol`
  两处），判据与它的事故注释留在原地。
- ✅ `using` 作第二个客户时照同款加一个 `UsingProtocol.Resolve` + 复用 core 的名字常量。
- 🔴 链不是数据 ⇒ **`[syntax]` 开关暂时裁不到「协议内的某一步」**，只能裁「整个语法构造」
  （`control_flow` 关掉则 `foreach` 整体不可用）。按 D3 的**语义**要求（每协议各自配、有序回落）
  是满足的；按「链是一份可配数据」的**字面**要求不满足。

### 形状 丙 —— 只做名字集中，链完全不动

- ✅ 最小、零行为风险。
- 🔴 拿不到「链在一处可见」这个收益，而那正是 `Count`/`Length` 当年分叉的根因（两处各写一份）。
- 🔴 `using` 落地时仍然是「新增一条 if 分支」，D2 裁决（经机制实现、非新增 if）落不了地。

> **我的推荐：乙。** 理由：甲的两个 🔴 不是工程量问题而是**表达力**问题 —— 判据里那些「首参是不是
> 整数」「属性的四种取法」压成串之后只会变成指回代码的占位符，等于把一份真数据换成一份假数据，
> 而代价是丢掉判据上方的事故注释（本仓这些注释是资产，不是噪声）。乙拿到了「名字唯一 SoT」与
> 「链在一处可见」这两个真实收益，也够 `using` 按 D2 落地；甲相对乙**唯一**多出来的能力是
> 「`[syntax]` 裁到协议内的某一步」——而那个需求今天并不存在，将来真要，也可以在乙之上把
> `Resolve` 里的步骤挂上 feature 名（那是批 2 表项挂 `feature` 的同款做法）。

**裁决：乙**（2026-09-25）。随之确定的两条边界：

- **名字是唯一 SoT，形态照仓内先例**：`z42c.core` 加一个 `ProtocolNames`（static class + 串常量，
  同 `DiagnosticCodes` 的形态 —— L4 说明该包没有 enum、没有泛型字段，并行数组只在需要「遍历全表」
  时才用，而名字常量不需要遍历）。
- **链在一处可见**：`ForeachProtocol` 增 `Resolve(Z42Type) -> ForeachPlan`，把今天散在
  `StmtBinder._bindForeach` 的 if 嵌套里的**路径选择**收进去；`StmtBinder` 只按 `ForeachPlan`
  合成 AST。判据函数（`IsIntIndexer` / `CountMember` / `EnumeratorDisposable`）**连同其事故注释
  原地不动**，`Resolve` 只负责「按什么顺序问它们」。
- **`[syntax]` 的粒度写进文档**：本批只能裁「整个语法构造」（D-new-1 的 🔴），这一点要落到
  `syntax-customization.md`，否则下一个人会以为能裁到协议内的某一步。

## D-new-2：特性门接线（照 #819 踩通的路径）

`[syntax]` 段 → `LanguageFeatures` → 解析器，让 E0301 首次发射。**四条必须照抄的经验**：

1. manifest 侧只搬运**中性 name/value 对**（`SyntaxNames`/`SyntaxValues`/`SyntaxCount`），
   解释权在消费方 —— 同 `[optimize]`/`[analyzers]`/`[lints]` 三段的既有形态
   （公字段 + `ManifestLoader` 构造后填，不撑大已 20 参的构造函数）。
2. **不在 CLI 层预先 resolve**，唯一一次组合落在 manifest 在手的那一层（workspace 每个成员有
   自己的 manifest）。
3. 🔴 **旋钮效果必须进 cache key**，否则「全量生效、增量被忽略」（`[optimize]` 实测踩过）。
   预计同款折进 `depsId`。
4. 🔴 门要取**行为/字节**判据，且**判别力来自夹具** —— 夹具里必须真的有会被该特性门挡住的语法。
   未知特性名**不静默忽略**（同 `Opt.ByName` 的口径）。

⚠️ **可能要跨一个 nightly**：给 `ProjectManifest` 加公字段并被 `z42c.driver` 读 = 新跨成员符号。
`[optimize]` 那轮没踩到（它没加新字段，只是把既有字段接上），本轮要加。判据一跑即知 ——
`xtask test bootstrap`（staged-bootstrap 边界门）就是为这件事建的。

## D-new-3：event 降糖搬出 parser 的边界

语法层改为**只引用 core 的名字常量**，合成 AST 的动作留在原地（不搬到 semantics）。
理由：搬 AST 合成 = 改变「谁在什么阶段造节点」，会动 `DeclParser` 的调用时机与 `isIface` 分支，
风险与本 change 的收益不成比例。proposal #3 要消掉的是「语法层**知道** stdlib 具体类叫什么」，
引用常量已经消掉它。

⚠️ 两处旧稿漏掉、必须一起搬的：
- `MemberParser.z42:20-22` handler 委托名映射（`MulticastFunc` → `Func`）；
- `MemberParser.z42:239` 语法层替用户合成 `new MulticastXxx<>()`。
⚠️ `"single-cast event already bound"` 存的是**带引号的原始 token 文本**（`StringLitExpr` 的约定），
表里存内容串会让生成的 AST 变形 —— 要么连引号一起存，要么在使用点加回。

## 验收（判别力要求）

- **不改行为**：自举不动点 3/3 + golden 全绿 + `List<T>`/`Dictionary` 的 foreach 三路径不变。
  收敛名字这件事**必须字节不变**（否则就不是收敛而是改语义）。
- **特性门**：开/关两个测试（只写「开着能解析」等于没测门）；关掉 ⇒ E0301 **必须**报。
- **event 降糖**：语法层不再出现 stdlib 具体类名（可加一条 grep 门守住，判别力靠注入假名验证）。
- **收敛有效**：改表里一个名字 ⇒ 所有客户一起变（否则说明还有第二份）。
