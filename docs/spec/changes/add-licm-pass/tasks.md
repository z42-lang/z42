# Tasks: LICM 循环不变量外提（新 Opt.Licm 位）

> 状态：🟡 opt-in 落地（不入 All，有已知 bug）| 类型：perf（compiler，优化管线）| 创建：2026-08-03
> "全部做" 优化程序 4/4（1=cascade / 2=CSE / 3=多块内联 已合）。最后一项，最重（需支配+循环分析）。
>
> **⚠️ 已知 bug（未根治，故 opt-in 不入 All）**：复杂 CFG——**嵌套循环 + `throw` 退出路径**（如
> `WorkspaceBuild.TopoOrder`）——LICM 误提，运行期 `undefined register`。调试历程：先修 ① `defBlock`（最后定义块）
> → `defInLoop`（**环内任一定义**，否则环内+环外双定义的命名局部漏判）；② 只提**单赋值** dst（`defsGlobal==1`，
> 否则多定义局部被提丢赋值）；③ 多回边**并**循环体 + 自循环 guard。三修后 array-OOB 症状消，但嵌套+throw
> 仍 `undefined %28`。**根因推测**：自然循环体 = 从 latch 反向可达（不过 header），**异常/多出口退出块**（`throw`
> 块无后继、不回 latch）被排除出 body → 其内定义漏入 defInLoop → 误判某操作数「循环外」→ 提到 pre-header 时
> 该操作数未定义。**修复方向**：body 应含「header 支配 + 能到 header 的所有块」或补异常边进 CFG；或对含
> ThrowTerm/多出口的循环保守跳过。根治前 LICM 仅 `--opt licm` opt-in，简单单循环正确（有单测）。

**变更说明：** 新增 `Opt.Licm` 位（32，All→63）+ `IrLicm.Run(f)`：把自然循环体内**纯 + 循环不变**的
计算提到循环 pre-header（每进循环只算一次）。机制：① CFG（终结子→后继/前驱位阵）；② 支配（迭代数据流
`dom[b]={b}∪∩dom[preds]`）；③ 回边（b→h 且 h 支配 b）→ 自然循环体（h + 从 latch 反向可达不过 h）；
④ pre-header 保守（h 唯一循环外前驱 + 该前驱 `br h` → 干净；否则跳过，不做 CFG 手术）；⑤ 不变量（IsPure +
**所有操作数循环外定义**，v1 单层——被提指令互不依赖→任意序安全）；⑥ 外提到 pre-header 尾。接线
inline→const-fold→**licm**→cse→copy-prop→dce。
**原因：** 循环主导 interp 运行时；把不变计算移出循环体大幅减 dispatch。
**文档影响：** book 优化页（LICM 节）；README（IrLicm 行）。

## 独立性（D2）/ 安全
- 新 `Opt.Licm` 位；单独开正确。仅提 IsPure 指令（白名单排除 Div/Rem 陷阱、FieldGet NPE、Call/*Set 副作用）
  → 提到「可能零迭代」pre-header 也安全（纯值、不触发陷阱/副作用）。单赋值 + pre-header 支配循环 → 提前定义仍支配使用点。
- 唯一处理序（回边按块序）→ 确定 → 自举不动点收敛。

- [x] 1.1 `IrLicm.z42`（NEW）：CFG + 支配 `_computeDom` + 回边/循环体 `_tryLoop` + pre-header 判定 + 不变量 + 外提
- [x] 1.2 `OptSet`：`Licm=32`（**opt-in，不入 `All`；All=31**）/ `ByName("licm")`
- [x] 1.3 `IrOptPipeline._optFunc` 接线（const-fold 后、cse 前）
- [x] 1.4 单测：循环不变 `a*b` 被提（Opt.Licm dump ≠ -O0 + mul 仍在）
- [x] 1.5 `xtask test` 全绿（LICM **opt-in 不入 All** → 默认/release/self-host 不启用 → 无 miscompile；LICM 单测用 Opt.Licm 显式验简单循环）
- [x] 1.6 文档同步（book + README，标注 opt-in + 已知 bug）

## v1 保守边界（后续）
- 链式不变量（依赖被提结果）v1 不提（留下游/再跑）；只提操作数全外部者。
- pre-header 须现成（不造）；无干净 pre-header 的循环跳过。
