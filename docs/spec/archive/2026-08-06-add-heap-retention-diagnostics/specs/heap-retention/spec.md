# Spec: 通用堆保留诊断（Heap Retention Diagnostics）

## ADDED Requirements

### Requirement: L1 — 查询直接引用者

#### Scenario: 对象字段引用被报为直接引用者
- **WHEN** 对象 `a`（类型 `Demo.Holder`）的字段指向对象 `b`，调用 `Std.Diagnostics.Heap.DirectReferrers(b)`
- **THEN** 返回的 `Retainer[]` 含一项，其 `Type` 为 `Demo.Holder`（`Label` 可读标识该引用者）

#### Scenario: 数组元素引用被报为直接引用者
- **WHEN** 数组 `arr` 的某元素指向对象 `b`，调用 `DirectReferrers(b)`
- **THEN** 返回项含该数组（`Type` 反映数组类型 / Label 标识"数组"）

#### Scenario: 无人引用的对象直接引用者为空
- **WHEN** 对象 `b` 除查询点外无任何堆对象/根引用（触发 GC 后仍存活仅因查询局部持有），调用 `DirectReferrers(b)`
- **THEN** 返回空数组（或仅含查询点自身的栈帧根，见 L2）——不含虚假引用者

### Requirement: L2 — 查询保留根（类别级）

#### Scenario: static 字段保留的对象报告 StaticField 根
- **WHEN** 对象 `b` 被某 `static` 字段（直接或经引用链）可达，调用 `Std.Diagnostics.Heap.RetainingRoots(b)`
- **THEN** 返回的 `RootRef[]` 含一项 `Kind == RootKind.StaticField`

#### Scenario: 仅栈局部保留的对象报告 StackFrame 根
- **WHEN** 对象 `b` 仅被当前调用栈的局部变量可达，调用 `RetainingRoots(b)`
- **THEN** 返回项含 `Kind == RootKind.StackFrame`

#### Scenario: 保留根经引用链可达（传递）
- **WHEN** `static 字段 → a → b`（`a` 引用 `b`），调用 `RetainingRoots(b)`
- **THEN** 报告 `StaticField` 根（反向 BFS 传递可达，不要求根直接指向 `b`）

### Requirement: 触发 GC 保证准确（无浮动垃圾误报）

#### Scenario: 已死的引用者不被报告
- **WHEN** 曾引用 `b` 的对象 `a` 已不可达（浮动垃圾），调用 `DirectReferrers(b)`
- **THEN** `a` **不**出现在结果中（查询先触发 full GC → `a` 被 sweep → 反向扫描只见存活可达对象）

### Requirement: 无回归

#### Scenario: 不调用诊断时 GC/堆行为不变
- **WHEN** 运行任意不调用 `Std.Diagnostics.Heap` 的现有用例
- **THEN** GC / 堆行为与本 change 前一致（诊断是按需 API + 独立堆扫描，不改 mark/sweep 热路径；分类根 scanner 仅诊断时调用）

## IR Mapping

无新 IR 指令、无 zbc/zpkg 格式 bump。能力经 native builtin（`__heap_direct_referrers` /
`__heap_retaining_roots`）+ `Std.Diagnostics` stdlib 类落地。

## Pipeline Steps

- [ ] Lexer / Parser / TypeChecker / IR Codegen —— 不涉及
- [x] VM interp / GC —— 反向图堆扫描 + 分类根枚举 + whyRetained 查询（**核心**）
- [x] stdlib —— `Std.Diagnostics.Heap` / `Retainer` / `RootRef` 新类
