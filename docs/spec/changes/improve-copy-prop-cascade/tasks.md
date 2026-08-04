# Tasks: 级联 copy-prop（use-site 传播 + ReplaceReads 基建）

> 状态：🟢 已完成 | 完成：2026-08-03 | 类型：perf（compiler，优化管线）| 创建：2026-08-03
> "全部做" 优化程序 1/4（后续：CSE / 多块内联 / LICM）。依赖 add-compiler-inlining 线（PR #102）。

**变更说明：** 补 `IrOptInfo.ReplaceReads`（通用「按 remap 重写一条指令/终结子的读操作数」，~40 case
type-switch，镜像 AddReads 的读枚举，可复用）；用它给 `Opt.CopyProp` 加 **use-site 级联传播**：
对单赋值 `dst = copy src`（src 稳定）建 dst→src 映射（含链式解析），全函数改写 dst 的使用点为 src，
删除变死的 copy。消除现有 copy-prop「单趟不级联」遗留的链式/非相邻 copy。
**原因：** 现有 copy-prop 只做相邻 producer→copy 的 retarget，`dst=copy src; …later… use dst` 这类留存；
use-site 传播把它们清掉 → 全代码更少 interp dispatch。ReplaceReads 同时为 CSE 铺路。
**文档影响：** book 优化页（copy-prop 节补 use-site 级联 + 安全边界）；z42c.semantics README。

## 独立性（D2）
- use-site 级联仍属 `Opt.CopyProp`（无新位）；CopyProp 单独开仍正确（producer-retarget + use-site 都自洽）。

- [x] 1.1 `IrOptInfo.ReplaceReads(ins, remap)` + `ReplaceTermReads(term, remap)`（remap: TypedReg[] by id，null=不改）
- [x] 1.2 `IrOptPipeline._passCopyPropUse(f)`：建单赋值 copy 映射（src 稳定：temp defs==1 / param defs==0）+
      链式解析 + ReplaceReads 改写全部使用点 + 删死 copy
- [x] 1.3 接线：`_optFunc` 的 CopyProp 分支在 producer-retarget 后追加 use-site 级联
- [x] 1.4 单测：链式/非相邻 copy 被清；独立性（仅 CopyProp）；self-host 不动点
- [x] 1.5 `xtask test` 全绿 + 文档同步
