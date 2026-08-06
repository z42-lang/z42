# Proposal: 纯函数调用优化（自动推断 + CSE/LICM）

## Why
优化管线当前最大缺口：`IrOptInfo.IsPure` 把**任何 `Call` 判为不纯** → 用户函数调用永远不能进 CSE
消重、不能进 LICM 外提。循环里的循环不变纯调用每迭代重算。给优化器"这个调用同参数同结果、无副作用"
的可信信息，就能消重 / 外提。**无需用户标注**——编译器自动推断（同 crossproc-escape 的模块不动点）。

## What Changes
- **纯度推断（核心，喂优化器）**：新建 `IrPureFunctionTable.z42` —— 模块级单调不动点算同模块每个
  函数是否"纯"（对标 `IrEscapeSummary`，方向相反：乐观全纯 → 发现副作用降级 → 收敛）。
- **纯度定义**：函数 f 纯 ⟺ f 的**每条指令**都是「`IsPure` 白名单内（算术/比较/常量/copy）」或
  「对纯函数的 `CallInstr`」或「**readonly 字段读**（`FieldGetInstr.Readonly==true`，值构造后不变）」，
  **且没有块以 `throw` 终结**。其余（写字段/静态/数组、读**非**readonly 字段、读静态/数组、`ArrayLen`、
  分配 `ObjNew/ArrayNew*`、`Div/Rem`、IO `Builtin/CallNative`、`VCall`/`CallIndirect`/`MkClos`、
  `ToStr`/`StrConcat`）→ 非纯。imported / 无体函数 → 保守非纯。
- **优化**（新 `Opt.PureCall=512`，进 `All=1023`）：
  - **CSE**：同 callee + 全 args 稳定的纯调用消重（纯 = 不依赖可变外部状态 → 无需失效表）。
  - **LICM**：全 args 循环不变的纯调用外提出循环（纯含 no-throw → 提到可能零迭代 pre-header 安全）。
- **测试**：codegen IrDump 前后对比 + 运行时 golden + bench。
- **文档**：optimization-pipeline / features / roadmap。

## 关键设计决策（详见 design.md）
1. **自动推断，不引入 `pure` 标注**（User 裁决）——覆盖最大、零负担、同构 escape-summary。
2. **纯度纳入 readonly 字段读**（复用 `FieldGetInstr.Readonly`）——`int scale(int k){return k*this.factor;}`
   若 `readonly int factor` 即纯。soundness：readonly 永不变 → 同 receiver+args 恒同结果。
3. **分配排除**出纯度——CSE 消重会改对象身份（`==`/GC），语义破坏；分配交 escape/loop-alloc。
4. **no-throw 纳入纯**——LICM 提到可能零迭代 pre-header，会抛的"纯"函数提前执行 = 异常时机漂移。
5. **v1 同模块**——imported 保守非纯（跨包 Deferred）。

## ⚠️ 与 readonly 不同的风险（必须认清）
readonly 时 z42c/stdlib 源无 readonly 字段 → 新 opt 对自编译输出零影响。**pure 不同**：z42c/stdlib
有可被推断为纯的函数 → PureCall 进 `All` 后**改变自编译产物**（语义不变、字节变）→
- 既有 codegen golden 可能漂移（把 `PureCall` 从默认 dump 路径减去，同 Inline/StackAlloc/LoopAllocReuse）
- 自举 gen1==gen2：一次性 D7 破一代、重建自愈（同 escape/loop-alloc 进 All 时，可控）
→ GREEN 重点盯自举不动点 + golden。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/IrPureFunctionTable.z42` | NEW | 模块不动点纯度推断（模板 IrEscapeSummary） |
| `src/compiler/z42c.semantics/src/OptSet.z42` | MODIFY | `PureCall=512`/`All=1023`/`ByName` |
| `src/compiler/z42c.semantics/src/IrOptInfo.z42` | MODIFY | `CseKey` 加 CallInstr 分支（+PureTable 参数）；`DstReg` 加 CallInstr |
| `src/compiler/z42c.semantics/src/IrOptPipeline.z42` | MODIFY | `Run` 算 PureTable；licm/cse 门控加 PureCall + 传 PureTable |
| `src/compiler/z42c.semantics/src/IrLicm.z42` | MODIFY | `Run` 收 PureTable；`_isHoistablePureCall` 外提分支 |
| `src/compiler/z42c.semantics/src/IrDump.z42` | MODIFY | 默认 dump optSet 减去 `PureCall`（防既有 golden 漂移） |
| `src/compiler/z42c.semantics/tests/codegen/codegen_tests.z42` | MODIFY | pure-call CSE/LICM IrDump 对比用例 |
| `src/tests/optimization/pure_call_hoist/` | NEW | 运行时 golden（source.z42 + expected_output.txt） |
| `src/libraries/z42.core/bench/pure_call_bench.z42` | NEW | bench 前后对比 fixture |
| `docs/book/src/runtime/optimization-pipeline.md` | MODIFY | pure-call pass 机制 |
| `docs/features.md` | MODIFY | 登记纯度推断优化 |
| `docs/roadmap.md` | MODIFY | Deferred Backlog Index（跨包 pure / 去虚化 VCall 纯 / 分配类纯函数标量替换） |
| `src/compiler/z42c.semantics/README.md` | MODIFY | 功能索引 + 核心文件（新 pass） |

**只读引用**：`IrEscapeSummary.z42`（模板）、`IrEscapeAnalysis.z42`（_findFunc/type-switch 骨架）、
`IrInstr.z42`（CallInstr/FieldGetInstr 结构）、readonly change archive。

## Out of Scope（Deferred，登记 roadmap）
- **跨 zpkg pure**：imported 函数纯度（`IrFunction.Attrs` 已序列化理论可读，但 v1 不依赖）。
- **去虚化后 VCall 判纯**（final 类/单态化）。
- **分配类"纯"函数的标量替换**（含 new 的确定性构造函数，需身份分析）。
- **放宽 Div/Rem**（可证非零除数时）。

## Open Questions
- 无（User 已裁决：自动推断、纳入 readonly、分配排除、同模块）。
