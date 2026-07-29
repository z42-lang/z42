# Spec: JIT ArrayGet/Set 内联去箱（Phase 4a）

## ADDED Requirements

### Requirement: i64/f64 数组读内联

#### Scenario: 单态 i64 数组读走原生内联
- **WHEN** `ArrayGet { dst, arr, idx }`，且 `reg_types` 证明 `arr` 为 i64 数组、`idx` 为 i64
- **THEN** JIT 发射原生边界检查 + 原生元素 load + 去箱存 `dst`（tag=I64, payload=元素值），
  **不调 `jit_array_get` helper**；结果与 helper 路径逐字一致

#### Scenario: 非基元元素回退 helper
- **WHEN** 元素类型为 Str/Array/Object 或 reg_types Unknown
- **THEN** 维持 `jit_array_get` helper 路径（去箱不安全 → 不内联）

#### Scenario: 越界抛异常（语义等价）
- **WHEN** 内联路径 `idx >= len`
- **THEN** 设 pending_exception（`array index N out of bounds`）+ return 1，
  与 helper 路径的异常类型/消息一致；`try/catch` 可捕获

### Requirement: i64/f64 数组写内联

#### Scenario: 基元值写内联
- **WHEN** `ArraySet { arr, idx, val }`，`val` 为 i64/f64（drop-free）
- **THEN** 原生边界检查 + 原生 store，无 helper

#### Scenario: heap-ref 值写维持 helper（写屏障）
- **WHEN** `val` 为 heap-ref（Str/Array/Object）
- **THEN** 走 `jit_array_set` helper（保留 GC 写屏障 `write_barrier_array_elem`）

## MODIFIED Requirements

**Before:** 所有 `ArrayGet`/`ArraySet` 无条件 `builder.ins().call(hr_array_get/set, ...)`。
**After:** reg_types 证明 drop-free 基元元素 → 原生内联 emitter；否则维持 helper。

## IR Mapping
无新 opcode。`ArrayGet` / `ArraySet` 的 JIT codegen 分叉（内联 vs helper），IR 不变、
zbc 格式不变、interp 不变。

## Pipeline Steps
- [ ] Lexer / Parser / TypeChecker / IR Codegen：**不涉及**（纯 JIT codegen 变更）
- [x] VM JIT（translate.rs emitter + array.rs helper）
- [ ] VM interp：不变（仅 JIT 分叉）
