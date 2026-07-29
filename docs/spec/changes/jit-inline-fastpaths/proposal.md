# Proposal: JIT 内联去箱快路（系统级 JIT 提速）

状态：🔴 DRAFT（2026-07-29，待 User 确认）
类型：**vm**（JIT 执行语义/代码生成变更）→ 走完整流程
子系统：`runtime`

## Why

perf-vm-iteration（PR #69）证明:局部微优化（frame 池化、块标签哈希）多为 interp、
收益几个百分点。**默认是 JIT,真正的系统级空间在 JIT 的架构**:

当前 JIT 本质是「原生码穿线解释器」—— 只有 i64/f64/bool **算术**被真正编译成原生指令,
其余 **array / field / call / string** 操作全部跨 `extern "C"` 调 helper,值始终是 24 字节
boxed `Value`。Cranelift 优化不进 opaque helper,所以这些操作既慢又挡住了寄存器分配/LICM/
常量折叠。

**实测天花板（本 change 的 spike，2026-07-29）**:同一 50M 迭代循环,JIT 模式:
- `total += a[j]`（数组读,走 helper）：834ms
- `total += j`（原生 i64，内联天花板）：252ms → **3.31×**

3.31× 是理论上限（零内存访问）；真实内联仍有一次原生 load/iter,估 **~2–2.8×**。对象/数组/
调用密集的真实程序,JIT 有望从「4.6× vs interp」拉到 **10×+**。这是数量级、且直接作用于默认模式。

## What Changes

给 JIT 增加**单态快路的原生内联 + 基元去箱**,把 helper-bound 操作带向原生效率:

- **Phase 4a**：`ArrayGet`/`ArraySet`（i64/f64 元素）内联 —— 原生边界检查 + 元素 load/store,
  结果去箱驻留寄存器。本 spike 已验证 ~3.3× 上限。
- **Phase 4b**：`FieldGet`/`FieldSet`（单态,IC 命中）内联 —— 偏移 load/store + 去箱。帮 poly/对象。
- **Phase 4c**：跨表达式**值去箱** —— i64/f64 在 Cranelift SSA 寄存器驻留,只在边界装箱;解锁
  Cranelift 全部优化。这是最大但最难的一步。
- **Phase 4d**（更远,单列评估）：单态调用直接/内联跳转,去 `jit_call` 蹦床。

每阶段：**先测该单项上限 → 设计 → 实施 → full GREEN（含 `xtask test e2e --mode jit` + 自举）→ 提交**。
任一阶段实测收益不达预期（如去箱被证伪）即停、记录、不硬上（沿用 opt_level / 去锁的纪律）。

## Scope（Phase 4a，本次先做；后续阶段各自扩 Scope）

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/runtime/src/jit/translate.rs` | MODEDIFY | `ArrayGet`/`ArraySet` 臂:reg_types 证明 i64/f64 元素时走内联 emitter,否则维持 helper |
| `src/runtime/src/jit/helpers/array.rs` | MODIFY | 新增 `jit_array_data`（安全返回 elems 数据指针 + len,供内联做原生 load）或等价安全接口 |
| `src/runtime/src/jit/translate.rs`（emitter） | MODIFY | 新增 `emit_array_get_i64` 等原生 emitter（镜像 `emit_i64_binop` 的 slot 读写手法） |
| `docs/book/src/runtime/jit.md`（或对应机制页） | MODIFY | 记录内联快路机制、去箱边界、fallback 条件 |
| `src/tests/.../jit_array_inline/`（golden/e2e） | NEW | i64/f64 数组读写 + 边界异常 + 非 i64 元素回退 helper 的正确性 |

**只读引用**：`emit_i64_binop`（1313）、`BrCond` 内联（1187）、`jit_array_get` 现状（helpers/array.rs:43）、
Value 布局常量（VALUE_STRIDE=24 / PAYLOAD_OFFSET=8）。

## Out of Scope

- Phase 4b/4c/4d（各自单列 spec + 上限测 + 确认后再做）
- AOT（LLVM，未实现）
- 改 `Value` 表示本身（去箱是 JIT 局部的,不动全局 Value enum）

## Open Questions（需 User / 实施时定）

- [ ] **数组数据指针的安全暴露**:内联需要 elems 的原生指针。方案 A=安全 helper 每次返回指针
  （仍一次调用/get,但省 Value 往返 + 让元素 load 原生化,估 ~1.5–2×）；方案 B=把指针提升到
  循环前（需 JIT 识别 loop-invariant 数组,复杂,但逼近 3.3× 上限）。**建议先做 A 测出真实收益,
  再决定是否投 B**。
- [ ] 去箱的 GC 安全边界（元素是 heap-ref 时不能裸存,须经写屏障）——本阶段限 i64/f64（drop-free）,
  heap-ref 元素维持 helper。
