# Tasks: release 专属优化 pass 的三处不健全（误编译）

> 状态：🟢 已完成 | 创建：2026-09-30 | 完成：2026-09-30 | 归档：2026-09-30
> 分支/worktree：`fix-release-opt-soundness` | 基于：origin/main
> 类型：`fix`（只收紧优化资格，**无格式 bump、不改语义**；只影响开了对应 Opt 位的构建，release 默认 `Opt.All`）

**变更说明（三处，同一病根：pass 的安全前提写得比实际弱）：**

- **A LoopAllocReuse（C4）**：资格从「ctor 单基本块」加强为「ctor 单块 **且** 对 this 无条件写全本类实例字段、
  基类只能是隐式 `Std.Object`」。原理由「未被写字段保持裸分配零初始化」不成立——裸分配只在 pre-header 做一次。
- **B 逃逸摘要（Pass A″）**：摘要模式下，被 `copy` 进参数槽的源值标逃逸（经 ref 出口写回离开本帧）。
- **C LICM 纯调用外提**：改用「可投机执行」子集（纯 ∧ 无 `FieldGet` ∧ 块图无环 ∧ 只调可投机函数），
  CSE 仍用「纯」。

**原因（全部实测复现，关对应 pass / 默认优化正确，`--opt-all` 错）：**

| | 复现 | 关 | 开 |
|---|---|---|---|
| A | `Counter(int x){X=x;}` + 循环里 `c.Hits = c.Hits + 1` 累加 5 轮 | 5 | 15 |
| B | `Stash(Box p, ref Box q){q=p;}`，caller `new Box(7)` 传入、经 ref 写回存进堆字段 | 7 | `FieldGet: expected object, got Null` |
| C | 零迭代循环里 `GetV(n)`（读 `n.V`，n == null） | 0 | NPE |

来源：全仓编译器审查（2026-09-30）。

**未决（待 User 裁决，未改）：** C 的「可投机」**不排除递归**——`fib` 外提是 add-pure-call-opt 的设计目标
（`pure_call_hoist` 用例、bench ~200×）。代价：会无限递归的纯函数在零迭代循环里被投机执行 → 栈溢出。

**文档影响：** `internals/runtime/optimization-pipeline.md`（pass 2e C4、pass 2g「纯 ≠ 可投机」）、
`internals/runtime/escape-analysis.md`（Pass A″）。

## 任务

- [x] 1 复现 A / B / C（修前 `--opt-all` 错、默认对）
- [x] 2 A：`IrLoopAllocReuse._ctorReinitsAllFields`
- [x] 3 B：`IrEscapeAnalysis.ComputeEscapedRegs` 摘要模式 Pass A″
- [x] 4 C：`PureTable.IsSpeculatable` + `IrPureFunctionTable._computeSpeculatable`；`IrLicm._isHoistablePureCall` 改用之
- [x] 5 回归（均带 `opt_all`，修前红）：`loop_alloc_reuse_stale_fields.z42`（含继承字段 + 写全字段仍复用的阳性对照）、
      `escape_summary_ref_writeback.z42`（含经局部 copy 链的一跳）、`pure_call_no_speculation.z42`（NPE + 死循环两种）
- [x] 6 既有优化用例不回归：`loop_alloc_reuse` / `pure_call_hoist` / `readonly_field_hoist` / `escape_ref_param_writeback`
- [x] 7 文档
- [x] 8 GREEN：`xtask test` 全绿
