# Spec: 循环内分配 hoist + 对象复用

## ADDED Requirements

### Requirement: 迭代内可复用的分配被 hoist 到 pre-header

#### Scenario: 对象在循环体内每迭代 new、迭代内局部、ctor this-safe
- **WHEN** 自然循环体（有干净 pre-header）内 `%r = ObjNew(Cls, ctor, args)`，且 `StackAlloc==true`、
  `%r` 迭代内局部（不跨回边携带）、ctor this-safe 且字段写无条件
- **THEN** pre-header 得到一条裸分配 `%r = ObjNew(Cls, ctor="", [])`，循环体原址变为 `Call ctor(%r, args)`；
  程序语义（可观测输出）与不做本变换时**完全一致**

#### Scenario: 数组在循环体内每迭代 new、size 循环不变、读前写全
- **WHEN** 循环体内 `%r = ArrayNew(size, elem)`，`StackAlloc==true`、size 循环不变、`[0,size)` 常量下标读前写全、`%r` 迭代内局部
- **THEN** `ArrayNew` 移到 pre-header，循环体原址无残留分配；输出与不变换时一致

#### Scenario: 复用消除累积（可观测于内存/计数）
- **WHEN** 8M 次迭代的循环，每迭代原本 new 一对象
- **THEN** 变换后该对象只分配 1 次（arena / 堆计数从 8M 降到 1），reinit 8M 次

### Requirement: 不安全的分配不被 hoist（安全兜底）

#### Scenario: 循环携带对象不复用
- **WHEN** `prev = p; ... p = new T(...)`（p 存入的 slot 跨回边 live）
- **THEN** C2 失败，不 hoist，保持逐迭代分配

#### Scenario: ctor 泄漏 this 不复用
- **WHEN** ctor 把 `this` 存到全局 / 传出 / 存入字段
- **THEN** C4 失败（`_ctorLeaksThis==true`），不 hoist

#### Scenario: 动态下标数组不复用
- **WHEN** 数组以 `a[i]=` 动态下标写、无法证明读前写全
- **THEN** C4 失败，不 hoist

#### Scenario: size 循环可变数组不复用
- **WHEN** `new int[n]` 的 n 每迭代不同（循环变）
- **THEN** C3 失败，不 hoist

### Requirement: 开关与诊断

#### Scenario: 关闭本 pass（编译期开关）
- **WHEN** `--no-opt loop-alloc-reuse` 或 debug 构建（默认关）
- **THEN** 不做本变换，逐迭代分配；输出与开启时**逐字节一致**（这是主正确性门——开/关对拍）

#### Scenario: hoisted 栈对象仍受 escape 运行时诊断保护
- **WHEN** hoisted 复用的 `Value::StackObject/StackArray` 句柄被错用（idx 越界 / frame_id 不符）
- **THEN** 沿用 add-escape-analysis 的 frame_id 悬垂校验，明确报错（非静默 UB）

## MODIFIED Requirements

### Requirement: 运行时 ObjNew 空 ctor 名 = 裸分配

**Before:** `ObjNew` 总是 alloc + 调 ctor（ctor 名恒非空）。
**After:** ctor 名为空字符串时，`obj_new` 只 alloc、不调 ctor（`outcome=None` 路径），不报「ctor not found」——
供本 pass 的 pre-header 裸分配使用。

## IR Mapping

- 无新 IR 指令、无 zbc/zpkg 格式变更。
- 复用：`ObjNewInstr`（ctor 名为空 = 裸分配哨兵）、`CallInstr`（ctor 重初始化静态调用）、`ArrayNewInstr`（整体 hoist）。

## Pipeline Steps

- [ ] Lexer —— 不涉及
- [ ] Parser / AST —— 不涉及
- [ ] TypeChecker —— 不涉及
- [x] IR Codegen —— 新 opt pass `IrLoopAllocReuse`（IrOptPipeline 挂入）
- [x] VM interp —— 空 ctor 名裸分配 + debug reinit 断言
