# Tasks: LICM 循环不变量外提（新 Opt.Licm 位）

> 状态：🟢 已完成（**bug 已根治，入 All**）| 类型：perf（compiler，优化管线）| 创建：2026-08-03 | 完成：2026-08-03
> "全部做" 优化程序 4/4（1=cascade / 2=CSE / 3=多块内联 已合）。最后一项，最重（需支配+循环分析）。
>
> **✅ 复杂 CFG bug 已根治**：曾在**嵌套循环 + `throw` 退出路径**（`WorkspaceBuild.TopoOrder`）miscompile
> （运行期 `undefined register`）。调试历程共 4 修：① `defBlock`（最后定义块）→ `defInLoop`（环内任一定义）；
> ② 只提**单赋值** dst（`defsGlobal==1`）；③ 多回边**并**循环体 + 自循环 guard；④ **根治**——判操作数不变性
> **用 header 支配域（`dom[b*n+h]`）而非 latch 反向可达体**：后者排除 `throw`/多出口退出块（无后继、不回 latch），
> 但它们仍受 header 支配；漏掉其内定义 → 误判「循环外」→ 误提 → undefined。所有真循环块 ⊆ header 支配域，
> 故支配域判不变性**保守安全**（绝不误提）。外提**源**仍用自然循环体（只移真循环指令）。
>
> **⑤ 异常表整体跳过（`ExcCount>0` → return）**：header-dominance 修好 `throw`-退出块后，仍有 16 个 e2e
> （event/multicast/finally/div-by-zero/reflection）红——根因是 **try/catch/finally 的异常隐式边**（受保护区→
> handler/finally）**不在** CFG（只从 Br/BrCond/Ret 建）→ 支配/循环分析错 → 误提。保守修：**有异常表的函数
> 整体不做 LICM**。加此 guard 后全 217+8 e2e/stdlib 绿、self-host 5/5、LICM **入 All=63**（默认启用）。
> 后续放开需把异常边纳入 CFG（design 级）。

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
- [x] 1.2 `OptSet`：`Licm=32` / **`All=63`（含 Licm）** / `ByName("licm")`
- [x] 1.3 `IrOptPipeline._optFunc` 接线（const-fold 后、cse 前）
- [x] 1.4 单测：循环不变 `a*b` 被提（Opt.Licm dump ≠ -O0 + mul 仍在）
- [x] 1.5 `xtask test` 全绿（LICM 入 All，作用于全 stdlib+e2e）；self-host 不动点 5/5（bug 根治后）
- [x] 1.6 文档同步（book + README，含 header 支配域判不变性的关键正确性说明）
- [x] 1.7 **根治复杂 CFG bug**：defInLoop 改用 header 支配域（修 throw-退出块）+ **跳过有异常表的函数**
      （`ExcCount>0`，修 try/catch/finally 隐式边）→ 全 e2e/stdlib 绿 + self-host 5/5 → 入 All=63

## v1 保守边界（后续）
- 链式不变量（依赖被提结果）v1 不提（留下游/再跑）；只提操作数全外部者。
- pre-header 须现成（不造）；无干净 pre-header 的循环跳过。
