# Proposal: 编译期 IR 优化管线（z42c 新增优化 pass，interp-first）

状态：🟡 IMPL（2026-07-30，方向已定：甲=编译器端；两准则已固化到 book）
类型：**ir**（改变 z42c emit 的 IR/zbc 指令流）→ 完整流程
子系统：`compiler`（z42c pass）+ `stdlib`（若 pass 落在 `z42.ir`）

## Why

z42c 现在**零 IR 优化**（朴素 codegen；全仓无 const-fold/DCE/copy-prop pass，已探查确认）。后果:

- **interp 逐条 dispatch 朴素 IR**——没有 Cranelift 兜底,IR 里每条冗余指令,interp 每次迭代都多一次解释开销。
- z42c 的 SSA-lite lowering **系统性地** emit 冗余 `CopyInstr`:每个命名局部赋值都是 `temp = expr; Copy(local, temp)`（`ExprEmitter.z42:546/558`、`FunctionEmitter.z42:421/581`,只跳过自拷贝）。

**这是 interp 性能的主要地基缺口**。按 book [optimization-pipeline](../../../book/src/runtime/optimization-pipeline.md) **准则 1（interp-first）**:IR 层优化以「减少 interp dispatch 条数」为第一目标,而 JIT 的 const-fold/DCE 由 Cranelift 兜底 → 这层优化的**最大受益者是 interp**。

**寄存器模型给了低垂果实**:表达式临时寄存器**单赋值**（每 temp 全新 `Alloc`,不复用）→ copy-prop / temp-DCE / const-fold **几乎不需分析**即可安全做;命名局部变量重赋值（需 def-use）留后。

## What Changes

在 z42c 加一个 **IR 优化 pass 框架**（emit IrModule 后、写 zbc 前跑）+ 三个首发 pass,全部**只作用于单赋值 temp**（保守、安全）:

1. **copy-prop（拷贝传播）**:`Dst = expr`(写 temp t) 紧跟 `Copy(local, t)` 且 t 仅此一处被读 → 改成 `expr` 直接写 local,删 Copy。砍 interp 每个赋值的一次 dispatch。
2. **temp-DCE**:单赋值 temp 的 Dst 从不被任何指令读 → 删该指令（无副作用指令才删;Call/VCall/字段写等有副作用的不删）。
3. **const-fold**:两个 const temp 喂给算术/比较 → 折成一条 const（`2+3→5`）。

pass 框架可扩展（后续加 CSE、局部变量 DCE 等作为新 pass）。

## 分层与 Scope 边界（准则 2 相关）

本 change 只做**编译期**优化（无运行时内存开销,pass 产出的是更小的 zbc）。**运行时 JIT/interp 分层 + 旧指令内存回收**（准则 2 的运行时面）是**独立后续 change** `runtime-jit-tiering`,不在本 change。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.ir/src/IrOpt/` 目录 | NEW | pass 框架 + 三 pass：`IrOptPipeline.z42` / `CopyProp.z42` / `TempDce.z42` / `ConstFold.z42` |
| `src/libraries/z42.ir/src/IrModule.z42` | MODIFY | 若 pass 需要指令级 def-use 辅助（读/写寄存器查询） |
| `src/compiler/z42c.semantics/src/IrGen.z42` | MODIFY | `Generate` 末尾调用 IrOptPipeline（emit 后、返回前） |
| `src/libraries/z42.ir/src/IrOpt/*_tests.z42` | NEW | 各 pass 单测（前后 IR 指令数/语义对比） |
| `docs/book/src/runtime/optimization-pipeline.md` | MODIFY | 补「机制/实现」节：三 pass 算法 + 单赋值前提 |
| `src/libraries/z42.ir/README.md` | MODIFY | 功能索引加 IrOpt |

**只读引用**：
- `src/compiler/z42c.semantics/src/EmitContext.z42`、`FunctionEmitter.z42`、`ExprEmitter.z42` — 理解 temp 分配与 Copy emit 规律
- `src/libraries/z42.ir/src/IrInstr.z42`、`IrTerminator.z42`、`TypedReg.z42` — 指令模型

## Out of Scope
- 运行时 JIT/interp 分层 + 内存回收（准则 2 运行时面）→ 独立 change `runtime-jit-tiering`
- 命名局部变量的 DCE/const-prop（需 def-use/liveness）→ 后续 pass
- 把运行时静态分析（branch_targets/catch 索引/hoist 不变量集）搬到编译期 emit → 独立 change（类别 A）
- CSE、指令合并/专用指令 → 后续 pass
- JIT 原生访存框架（repr(C) 稳定布局）→ 独立 change

## Open Questions
- [ ] pass 顺序:const-fold 可能产出新死代码/新可传播拷贝 → 首版按 `const-fold → copy-prop → temp-DCE` 单趟,评估是否需迭代到不动点（首版单趟,量收益再决定）。
- [ ] 自举影响:改 z42c emit → golden zbc 基线变 + 自举 gen2 需仍为不动点。实施含 golden regen + 自举 5/5 fixpoint 复验（见 tasks 验证段）。
