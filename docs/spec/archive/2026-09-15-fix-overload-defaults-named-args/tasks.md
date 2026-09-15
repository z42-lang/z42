# Tasks: fix-overload-defaults-named-args

> 状态：🟢 已完成（User 2026-09-15 确认）｜ 创建：2026-09-15 ｜ 类型：fix（compiler，最小化模式）｜ 基于 main `584774314`

**变更说明：** 重载决议把「默认值形参」「命名实参」「params 两种形态」纳入候选适用性判定，方法与构造器同一套规则（对标 C#）。
**原因：** 「推进 ctor 静默 bug」④ 实施中撞出，nightly 复现。有两个及以上重载时，省略默认实参 / 使用命名实参要么找不到方法，要么**静默选错重载**。
**文档影响：** book `language/named-arguments.md`（重载 + 命名 / 默认值的决议规则）；编译器机制页（重载决议）补一节。

## 实测（main 构建，`z42c build`）

| 写法 | 实际 | 期望 |
|---|---|---|
| `M.F("a")`，`F()` / `F(string a, int n = 2)` | E0401 no static method | 选 `F(string, int=2)` |
| `m.G("a")`（实例方法同形） | E0401 no method | 同上 |
| `M.K(s: "x")`，`K(int x)` / `K(string s)` | E0437「target-typed new 需要类型」+ `s` 未定义 | 选 `K(string)` |
| `new C("a")`，`C()` / `C(string a, int n = 2)` | **编译通过、运行期选中无参 ctor**（MissingSymbolException） | 选 `C(string, int=2)` |
| `new C(n: 7, a: "b")` | `n` / `a` 未定义 | 选 `C(string, int=2)` |
| `new P("a")`，`P(string a, int n = 2)` / `P(params int[] xs)` | 选 params 版本，E0402 | 选 `P(string, int=2)` |
| `M.H(1)`，`H(int)` / `H(int, int b = 9)`；`M.F(n: 7, a: "b")` | ✅（碰巧：精确 arity 唯一） | 不变 |

## 根因

- **方法**（`OverloadBinder._resolveOverload`）：候选先按**形参个数精确相等**过滤；为空时只有 params 展开与「全类只有一个同名方法」两条兜底。
  带默认值的重载在「少给实参」时从不进候选集。命名实参在 `_bindCall` 里是延迟占位（null），被当作 target-typed `new` 的延迟位 ⇒ 多候选时报 E0437。
- **构造器**（`ConstructTyper._bindCtorArgs`）：`_ctorKey` 按实参个数取键（找不到回落裸名 = primary），类型决议 `OverloadResolver.Resolve` 同样只认精确 arity；
  类型决议前把 raw 实参逐个 `_bindExpr`，命名实参 `a: "b"` 被当成对未定义变量 `a` 的赋值。

## 规则（对标 C#）

1. **实参 → 形参映射**：位置实参依次占前 N 个形参；命名实参按名占位（名字不存在 / 重复 / 与位置实参撞位 ⇒ 该候选不适用）。
2. **适用**：每个被占的形参，实参类型可赋值（无类型的延迟实参——target-typed `new()`、lambda——视为通配）；每个未被占的形参有默认值（本地 `Default` / 跨包 `$Default` / caller 宏）或是 params 尾参；
   params 分**正常形态**（实参是数组）与**展开形态**（多余位置实参逐个可赋值到元素类型）。
3. **择优**：先按既有逐位「更具体」比较（只比较两者都被实参占用的位置）；仍平手时，**不需要默认值**的候选胜过需要默认值的；**非展开**形态胜过展开形态。仍平手 ⇒ E0425 歧义（既有码 `AmbiguousOverload`）。
4. 精确 arity 唯一且适用的现有路径结果不变（字节零漂移）；「全类只有一个同名方法、忽略 arity」的既有兜底保留为最后一步。

## 任务

- [x] 1.1 回归：`src/tests/classes/overload_defaults_named.z42`（19 条断言，修前 15 个编译错误；另暴露两处静默选错：`Q("a")` 选中 params、`new P(1, 2)` 选中 `P(string, int)`）+ typecheck 单测 4 条（歧义 E0425 / 未知名字 / 默认值 / ctor）；两后端通过，并以故意改错的期望做对照确认用例真在执行
- [x] 2.1 `OverloadResolver.Map` / `ResolveMapped`（`ArgShape` / `MapResult`）：实参映射适用性 + 两条 C# 平手规则
- [x] 2.2 `OverloadBinder._resolveOverload` 增 `rawArgs, env`（8 个调用点透传）；命名实参 + 多候选 ⇒ 映射决议；无精确 arity 候选时「靠默认值」候选与 params 结果一起映射决议
- [x] 2.3 `ConstructTyper._bindCtorArgs`：多候选且（命名实参 / 精确 arity 类型决议未选出）⇒ 映射决议；有命名实参时不预绑定
- [x] 3.1 文档：book `language/named-arguments.md`「与重载、默认值一起用」；`compiler/source-compile.md`「重载决议：默认值形参、命名实参、params 两种形态」（伪代码 + 接入点表）
- [x] 3.2 全量 GREEN（`GREEN_EXIT=0`，并入 #667 后的 main `3b900f0b2`）；`xtask test bootstrap` ✅；本地不编译目录扫描与修前（nightly）逐条对比无新增诊断；`xtask test fingerprint --base <main 树>`：25 个包逐字节一致 ⇒ 不累加 `CompilerFingerprint`

## 实施记录

- 实施期间 main 合入 #667（普通调用实参个数编译期校验 E1005 / E1006，同改 `OverloadBinder`）：rebase 文本无冲突；语义上本变更只负责
  「多候选时选哪个」，#667 的缺实参 / 多实参判定在其后的 `_withDefaults` 汇聚点，互不重叠（映射决议仅在候选 ≥ 2 时介入，不抢单候选诊断）。

