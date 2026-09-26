# tasks: fix-flow-accessor-bypass

> 类型：**fix**（接上四条旁路；不改语义规则，只让既有规则真的生效）｜ 创建：2026-09-27
> 出身：[结构审计 2026-09](../../../internals/src/compiler/source-compile.md) 的 P3。

## Why

`FlowAnalyzer.Check`（E0407 局部未赋值就读 + 整套 `?` 空检查的种子）**只有一个调用点**：
`DeclBinder._bindMethodBody`。而**属性 getter / setter 与索引器 getter / setter 四处直接调
`_bindStmt`**，绕过它 ⇒

> **E0407 与整套 `?` 空检查在属性 / 索引器体里全是哑的。**

`_bindMethodBody` 的注释当时还写着「保证**只有一处**调用、且查的是最终交给发射层的那棵树」——
前半句对**方法**成立、对 accessor 不成立，那四处就是四条旁路。

漏报面 = 全部属性体与索引器体。属性体在 stdlib 与用户代码里都很常见（计算属性、自定义 setter、
索引器），所以这不是边角。

## What Changes

新增 `DeclBinder._bindAccessorBody(raw, env, ret, retTe, ps, pc, sp)`：绑完体之后合成一个
`MethodDecl` 壳交给 `FlowAnalyzer.Check`。四处 accessor 全部改调它。

为什么要合成壳：`FlowAnalyzer._seed` 只从 `md` 读**两样**东西 ——
① 返回类型是否带 `?`（`_retIsNullable`）；② 形参表（形参标「已赋值」，`?` 形参再标「可能为 null」）。
accessor 没有 `MethodDecl`，但它这两样都是确定的。合成壳这条路在**发射侧已有先例**
（`IrGenMemberEmitter` 的 `synthPg`/`synthPs`/`synthGet`/`synthSet` 四个壳），本处与它们同形 ——
只是发射侧要的是签名、这里要的是种子。

形参表由 `_accessorSetParams` 统一拼（索引参数 + 隐式 `value`），与两处绑定环境里
`Define("value", …)` 的口径同源 —— 两边必须一致，否则 `value` 会被 DA 当成未赋值局部而**误报**。

⚠️ **setter 的返回类型必须给 `void`**，不是属性类型：属性是 `T?` 时把 `T?` 当返回类型会让
`_retIsNullable` 为真，而 setter 根本没有返回值 —— 那是一条凭空的判据。

## Scope（允许改动的文件）

- `src/compiler/z42c.semantics/src/DeclBinder.z42`
- `src/compiler/z42c.semantics/tests/typecheck/definite_assignment_tests.z42`

## Tasks

- [x] `_bindAccessorBody` + `_accessorSetParams`；四处 accessor 改调它
- [x] 7 条单测，**每种形态阴性 + 阳性配对**：
      · 属性 getter / setter、索引器 getter / setter 各一条阴性（必报 E0407）
      · **阳性对照两条**：隐式 `value` 形参、索引参数 `i` 必须算已赋值 —— 只写阴性的话，
        「合成壳漏掉形参」会让它们被当成未赋值局部而误报，而那种门看起来照样是绿的
- [x] stdlib 25 包 + 编译器自建：**零新诊断**（既无误报，也说明 stdlib 本来没有违规）
- [x] **阴性对照（端到端，比单测更直接）**：同一份 `class C { int P { get { int x; return x; } } }`
      · 撤回 getter 那处 ⇒ **编译通过、零诊断**（证明修复前确实静默放过）
      · 带修复 ⇒ `E0407: use of unassigned local variable \`x\``
- [x] `CompilerFingerprint` 34 → 35（诊断集变了；stdlib 零命中 ⇒ CI 守门对这档是瞎的）
- [x] `xtask test compiler` 全绿：z42c 24 单元全过（含新加 7 条）+ `✅ 自举不动点 3/3 gen1==gen2`
- [ ] GREEN：CI 全矩阵绿

## ⚠️ 顺带发现：`test compiler` 的 e2e fixture 不是并发安全的（既存缺陷，另立一刀）

第一次跑 `test compiler` 时 `probing 数组` 那格红了：`pparr_a` 建成、`pparr_b` 报
「`[dependencies] z42.core` 未找到 z42.core.zpkg；已查找：/tmp/z42c-e2e-pparr/libs」，
而那个 `libs/` 目录**事后根本不存在**。

根因：deploy 系列 e2e 的 fixture 根是 **`/tmp` 里的固定路径**（`/tmp/z42c-e2e-pparr`、
`/tmp/z42c-e2e-deploy` …），而每个 fixture 开头都 `Directory.Delete(tmp, true)`。
本机此刻有多个会话在并行跑测试（`wt-learn19` / `wt-tryget` 的 scratch 都比本树更新），
另一个会话进入同一格就会把本轮的 `libs/` **中途抹掉**。

**取证**：同一份源码原样重跑 ⇒ `rc=0`、`✓ probing-paths 数组` 通过 ⇒ 与本 change 无关。

路径**故意**放在仓库外（为了覆盖「repo 外的消费方」这条路，见 `_e2eDeployRoot` 的注释），
所以不能简单搬进 `artifacts/.scratch/`；正解是**按仓库根派生一个后缀**，既留在仓库外、
又让并发的两棵树互不干扰。单独一刀。

## 不做（Out of Scope）

- **不动 ctor 里 `Check` 与 `_injectFieldInits` 的先后**。审计把它列为第二个问题（Check 跑在
  字段初始化注入**之前**），但核实后影响面为零：DA 的管辖范围明确**不含字段**
  （「字段与静态字段不在管辖（它们零初始化）」），而注入进来的语句只赋字段、既不读也不写局部。
  `?` 那一侧同理 —— 种子只来自形参与返回类型。**留着不动比改对更安全**：挪动它会改变
  `_noInitLocals` 等游标状态的可见时序，收益为零。
- **不给 accessor 补 `?` 空检查的专门用例**。`?` 机制与 DA 共用同一个 `FlowAnalyzer` 实例与
  同一个种子入口，接上一处即两者同时生效；DA 那 7 条已经钉住了「Check 有没有跑」这件事。
  单独的 `?` accessor 用例属可空线的覆盖面，单独登记。

## 验证

- 不改任何产物字节（只新增诊断路径；stdlib 零命中 ⇒ 产物不变）⇒ 无格式 bump。
- **指纹**：这条**会改变同一份源码的编译结果（诊断集）** —— 属性/索引器体里原本编得过的
  未赋值读现在报 E0407。按 version-bumping 规则表第 1 行需 `CompilerFingerprint++`，
  号按合并时的 main 现查。⚠️ stdlib 零命中 ⇒ **CI 的 fingerprint 守门对这一档是瞎的**。
