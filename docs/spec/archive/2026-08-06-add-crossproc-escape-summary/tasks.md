# Tasks: 跨过程逃逸摘要

> 状态：🟢 已完成 | 创建：2026-08-06 | 完成：2026-08-06

## 进度概览
- [x] 阶段 1: 摘要不动点（IrEscapeSummary）
- [x] 阶段 2: 消费摘要（IrEscapeAnalysis 参数化）
- [x] 阶段 3: 测试与验证

## 阶段 1: 摘要不动点（IrEscapeSummary.z42，NEW）
- [ ] 1.1 `ParamEscapeTable` 持有者（`bool[][]` 按函数索引 + `nameToIdx` StrMap + `ParamEscapes(idx)`/`Lookup(name)`）
- [ ] 1.2 `Compute(m)`：建 nameToIdx + init 全 false + 单调不动点循环（每轮各函数 `_computeEscaped` 用当前表 → 更新参数位 → changed 收敛）

## 阶段 2: 消费摘要（IrEscapeAnalysis.z42，MODIFY）
- [ ] 2.1 `_markEscaping(m, table, esc, ins)` 参数化：`CallInstr` 实参按 `table[callee][i]`；`ObjNew` 实参按 `table[ctor][i+1]`；VCall/CallIndirect/Builtin/CallNative/MkClos 保守全标；其余汇点不变
- [ ] 2.2 `_computeEscaped(m, f, table)` 传摘要；Pass B copy 闭包不变
- [ ] 2.3 删 `_ctorLeaksThis`，对象合格改查 `table[ctor][0]`（找不到 ctor → 保守 true）
- [ ] 2.4 `Run(m)`：先 `IrEscapeSummary.Compute(m)` → 用收敛表跑 `_markFunc`
- [ ] 2.5 行数检查：IrEscapeAnalysis 若超 300 软限，确认摘要已拆到 IrEscapeSummary

## 阶段 3: 测试与验证
- [ ] 3.1 cargo build (z42vm) 无错 + z42c 自建通过
- [ ] 3.2 单测（tests/escape-summary/）：非逃逸参数 / 逃逸参数（存字段·返回·throw）/ 传导 / 相互递归 / 跨包保守
- [ ] 3.3 e2e golden（src/tests/optimization/escape_crossproc_*/）：传只读函数栈分配 + 传泄漏函数不栈分配；开/关（`--no-opt stack-alloc`）输出一致，interp+jit
- [ ] 3.4 `xtask test` 完整 GREEN gate，含 **self-host 5/5 gen1==gen2**（摘要作用于编译器自身、字节不动点）
- [ ] 3.5 量测：`foo(new Point())×N` 循环 C 前后（对象堆→栈；如叠加 #118 看复用）
- [ ] 3.6 文档同步：README（escape pass 描述 + IrEscapeSummary）+ book escape-analysis-stack-alloc（跨过程摘要机制）+ roadmap Deferred ② 落地
- [ ] 3.7 spec scenarios 逐条覆盖确认

## 备注
- VCall/CallIndirect 摘要（去虚化后放宽）、IsInstance/Convert/ToStr 放宽（需运行时补 StackObject 分支）→ Out of Scope，design/roadmap Deferred。
- 与 #118 loop-alloc-reuse 协同：C 让「传进调用的对象」也 StackAlloc → 循环里这类对象也能被 #118 hoist 复用（两者不同文件、不冲突）。
