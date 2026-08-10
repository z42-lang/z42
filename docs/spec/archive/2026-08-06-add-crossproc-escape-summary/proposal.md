# Proposal: 跨过程逃逸摘要（cross-procedural escape summary）

## Why

当前逃逸分析（`add-escape-analysis-stack-alloc`）是**单函数**的：一旦对象被传进**任何调用**，`_markEscaping`
就保守把所有实参判**逃逸**（callee 可能捕获/存字段/返回）。结果——最常见的「造个临时对象传给只读它的辅助
函数」模式（`sum += Dist(new Point(i,i))`）**享受不到栈分配**，也因此**享受不到 loop-alloc-reuse 复用**
（后者要 `StackAlloc==true`）。这是逃逸分析当前**最大的精度缺口**。

不做的话：跨函数流动的临时对象一律堆分配 + GC，且 hoist+复用覆盖面被卡在「不传出去」的对象。

## What Changes

- **给每个函数算一份「参数逃逸摘要」**：`paramEscapes[f][i]` = 函数 f 的参数 i 是否逃逸（被存字段/静态/
  数组、返回、throw、传给别的会让它逃逸的调用、闭包捕获）。摘要**互相依赖**（f 调 g）→ 跑到**模块不动点**
  （单调迭代，收敛）。
- **`_markEscaping` 参数化**：`CallInstr`（静态直接调用）的实参**按 callee 摘要**逐个判逃逸——callee 不让
  参数 i 逃逸 → 该实参**不判逃逸**（对象可栈分配）。`ObjNew` 的 ctor 实参同理（按 ctor 摘要，偏移 +1 跳过 this）。
- **统一 `_ctorLeaksThis`**：并入通用摘要（ctor 的 param 0 = this）——原「ctor 单函数 this-摘要」成为通用
  逐参摘要的特例。
- **保守兜底**：跨包 callee（体不可见）/ `VCall`·`CallIndirect`（动态派发，实际 callee 未知）/ `Builtin`·
  `CallNative`（原生，无 z42 体）→ 实参**仍全判逃逸**（无摘要即保守）。递归/相互递归由不动点单调收敛处理。
- **无格式 bump、无新 IR 指令、无运行时改动**：`StackObject` 早已能跨帧（per-context arena，ctor 子帧就是先例）
  → 传进调用的栈对象在 callee 帧经 FieldGet 照常解析；C 只是让编译器把**更多**对象标 `StackAlloc=true`。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/IrEscapeSummary.z42` | NEW | 参数逃逸摘要的模块不动点计算（名→索引表 + 迭代收敛） |
| `src/compiler/z42c.semantics/src/IrEscapeAnalysis.z42` | MODIFY | `_markEscaping` 参数化（吃摘要）；`_computeEscaped` 传摘要；`_ctorLeaksThis` 并入摘要；Run 先算摘要 |
| `src/compiler/z42c.semantics/README.md` | MODIFY | 功能索引 + 核心文件登记摘要文件 + 更新 escape pass 描述 |
| `src/compiler/z42c.semantics/tests/escape-summary/` | NEW | 摘要单测（非逃逸参数 / 逃逸参数 / 递归 / 跨包保守） |
| `src/tests/optimization/escape_crossproc_*/` | NEW | e2e golden：`foo(new Bar())` 栈分配 + 泄漏参数不栈分配（开/关输出一致） |
| `docs/book/src/runtime/escape-analysis-stack-alloc.md` | MODIFY | 跨过程摘要机制（不动点、保守兜底、与 ctor 摘要的统一） |
| `docs/roadmap.md` | MODIFY | Deferred Index：escape-stack-future ② 标记落地（本 change） |

**只读引用：**
- `src/compiler/z42c.semantics/src/IrOptInfo.z42` — `AddReads`/`AddDef` 读写枚举（摘要复用）
- `src/runtime/src/interp/exec_object.rs` — 确认 StackObject 跨帧（per-context arena）无需运行时改动

## Out of Scope

- **`VCall` / `CallIndirect` 摘要**：动态派发实际 callee 未知 → v1 保守全逃逸（单态化/去虚化后再放宽，另立）。
- **`IsInstance`/`Convert`/`ToStr` 放宽**：这些当前被有意标逃逸以**收窄运行时触达面**；放宽需运行时补 StackObject
  分支（触达面扩张）→ v1 保持标逃逸，另立 change。
- **返回值精度**：返回参数 = 逃逸（保守）；不做「返回值在调用方是否逃逸」的跨返回追踪。
- **JIT 侧 arena**（escape-stack-future ①）：本 change 不碰，JIT 仍忽略 flag 堆分配。

## Open Questions

- [ ] 摘要文件是否独立成 `IrEscapeSummary.z42`（预计 IrEscapeAnalysis 会超 300 行软限）→ design 定（倾向独立）。
- [ ] 不动点收敛用「全量迭代到不变」还是 worklist（只重算 callee 摘要变了的）→ v1 全量（简单），慢再优化。
