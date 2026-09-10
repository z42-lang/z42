# Tasks: e0456-declaration-positions

> 状态：🟢 已完成 | 创建：2026-09-11 | 完成：2026-09-11 | 类型：feat（compiler，纯诊断、零字节影响）

**变更说明：** E0456（裸名跨命名空间歧义）此前 11 个调用点**全在语句/表达式位**
（catch / var / typeof / is / as / 模式），**声明位一个都没有** ⇒ 歧义的 `public Widget w;`
静默挑一个短名竞争的赢家。本 change 补上声明位：字段 / 属性 / 形参 / 返回 / 基类 / 约束 / 索引器。

出身：[unify-type-identity-fqn](../../archive/2026-09-10-unify-type-identity-fqn/)（#559）的
Deferred。那个 change 的发射端守卫只保证「**不写错答案**」（歧义时退回短名、诚实降级），
「让歧义代码**报错**」这半边一直空着。

## 落地

- `SymbolCollector._chkTypeRefT`：`_chkTypeRef` 的带源 `TypeExpr` 版本，访问校验之外顺带判裸名歧义。
  所有声明位调用点手边本就有 TypeExpr，改动是机械的。
- `MemberCollector._passMembers` 顶部换 `table.WithScopeOf(cu)`：判据要看**引用方**的 ns + usings，
  collector 原本拿的是无作用域的共享表。视图共享全部数据表（注册照常写共享表）。
- `SymbolTable.WithScopeOf`：**只设作用域、不动别名**。借 `WithCu` 会顺带把别名解析引进收集期，
  改动面超出本 change。usings 提取抽成 `_setScope`，与 `WithCu` 共用一份。
- **判据与消息都只此一份**：判据 `IsBareNameAmbiguous`（#559 已抽），消息上移为
  `AmbiguousBareNameMsg`；`TypeChecker.ChkAmbiguousBareName` 从 22 行手写改为 3 行转调。

> 顺带：`CurrentAliases` 从此写在**视图**而非共享表上，与 `TypeChecker` 的
> 「不覆写共享 CurrentAliases、并行段零共享可变」同方向。已核实无下游依赖
> （`InheritanceResolver` 两个 pass 各自重设）。

## 验证

⚠️ **新诊断在现有代码上触发 0 次**（stdlib + z42c 全量构建实测）—— 零触发 = 零证据，
不自带 fixture 就等于装了个从不响的门。故新增 6 条 + 3-CU 助手 `pmerge3Diags`
（既有 `pmergeDiags` 只吃 2 个 CU，而声明位歧义**至少需要 3 个**：两个 ns 各声明同短名，
第三个 `using` 两者并在声明位引用）：

| 用例 | 修前 | 修后 |
|---|---|---|
| 字段 / 形参 / 返回类型歧义 → 报 E0456 | **FAIL ×3** | PASS ×3 |
| 只 using 一个 ns / 外围 ns 自己声明 / 限定名 → 不报 | PASS ×3 | PASS ×3 |

**阳性三条退回对照实测修前全 FAIL**，判别力已验证；阴性三条两侧都 PASS，守住不误报。
完整 GREEN 全绿 + 自举不动点 3/3。

## 同批未做的两项（合并为独立 Deferred）

原计划「一起做」的**实参拼写统一**与**格式 bump**，实施中查明必须合并成一件事，
已登记 [`unify-resolved-vs-source-type-spelling`](../unify-resolved-vs-source-type-spelling/proposal.md)：

- 拼写统一**不能只改一个产出方**（实测只改 SIGS 会打断跨包泛型 struct 解析，harness 改则红、不改则绿）；
- 其门**必须是跨包场景**（本地路径本就是关键字拼写，我做的两版单测门都是空门，退回对照抓出）；
- 格式 bump **没有触发对象**：本 change 纯诊断零字节，#559 的 FQN 已作为整体合并且当时裁决不 bump
  ⇒ 下一次真正改持久化内容的就是那个 Deferred，届时两次内容变更合用一次 bump。
