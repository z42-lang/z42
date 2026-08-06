# Spec: 跨过程逃逸摘要

## ADDED Requirements

### Requirement: 参数逃逸摘要（模块不动点）

#### Scenario: 只读参数的函数 → 参数不逃逸
- **WHEN** 函数 `Dist(Point p)` 只读 `p.X`/`p.Y`、不存不传不返
- **THEN** `table[Dist][0] == false`（p 不逃逸）

#### Scenario: 泄漏参数的函数 → 参数逃逸
- **WHEN** 函数 `Store(X x)` 把 `x` 存到静态字段 / 返回 `x` / throw `x`
- **THEN** `table[Store][0] == true`

#### Scenario: 传递给逃逸 callee → 逃逸传导
- **WHEN** `f(Y y) { Store(y); }` 且 `Store` 的 param0 逃逸
- **THEN** `table[f][0] == true`（经 CallInstr 实参传导）

#### Scenario: 相互递归收敛
- **WHEN** `f(p){ g(p); }` 与 `g(q){ f(q); }` 都不含直接汇点
- **THEN** 不动点收敛，`table[f][0]==false && table[g][0]==false`

#### Scenario: 跨包 / 动态派发 callee 保守
- **WHEN** 参数传给跨包函数（体不可见）/ `VCall` / `CallIndirect` / builtin
- **THEN** 该实参保守判逃逸（无摘要即 true）

### Requirement: 跨过程精度解锁栈分配

#### Scenario: 传给只读函数的临时对象可栈分配
- **WHEN** `Point p = new Point(i,i); use = Dist(p);`，`Dist` 的 param0 不逃逸、p 无其它逃逸用法
- **THEN** 该 `ObjNew` 置 `StackAlloc=true`（跨过程前会因「传进调用」被判逃逸）

#### Scenario: 传给泄漏函数的对象不栈分配（安全）
- **WHEN** `X x = new X(); Store(x);`，`Store` 的 param0 逃逸
- **THEN** 该 `ObjNew` **不**置 StackAlloc（堆分配，无悬垂）

### Requirement: 语义不变（正确性主门）

#### Scenario: 开/关逃逸栈分配输出一致
- **WHEN** 同一程序 `--no-opt stack-alloc` 关 vs 开
- **THEN** 输出**逐字节一致**（跨过程摘要不改变可观测语义，只改分配位置）

#### Scenario: 误判由运行时诊断兜底
- **WHEN**（若摘要有 bug）逃逸对象被误栈分配、帧退出后被访问
- **THEN** frame_id 悬垂校验**明确报错**（非静默 UB）

## MODIFIED Requirements

### Requirement: ctor this-泄漏判定并入通用摘要

**Before:** 独立 `_ctorLeaksThis(ctorName)` 单函数摘要判 ctor 是否泄漏 reg0=this。
**After:** 并入通用逐参摘要——对象合格判据查 `table[ctor][0]`（this=param0 的特例）；删除单独实现。

### Requirement: call 实参逃逸判定由 blanket 改为按摘要

**Before:** `_markEscaping` 把 `CallInstr`/`ObjNew` 的**所有**实参标逃逸。
**After:** 按 callee 摘要逐实参标（`CallInstr` args[i]→param i；`ObjNew` args[i]→ctor.param[i+1]）；无摘要
（跨包/VCall/CallIndirect/builtin）仍全标。

## IR Mapping

- 无新 IR 指令、无 zbc/zpkg 格式变更、无运行时改动。仅编译期 `IrEscapeAnalysis` 精度提升 → 更多 `StackAlloc=true`。

## Pipeline Steps

- [x] IR Codegen —— `IrEscapeSummary`（不动点）+ `IrEscapeAnalysis`（消费摘要）
- [ ] VM interp —— 无改动（StackObject 跨帧已支持）
