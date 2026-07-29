# Proposal: 可扩展 JIT lowering 管线（从「逐 opcode 特例」到「lowering 规则 + Cranelift 优化」）

状态：🔴 DRAFT（2026-07-29，待 User 裁决方向/深度）
类型：**vm**（JIT 架构/codegen 管线重构）→ 完整流程
子系统：`runtime`

## Why

当前 JIT（`src/runtime/src/jit/translate.rs`）是一个 **template JIT**：`translate_function` 用一个
巨型 `match instr` 直接把每条 z42 IR 指令 emit 成 Cranelift。fast path（arith、`jit-inline-fastpaths`
的数组内联）都是**手写在 match 里的 special case**。

**这不 scale**：每加一个优化（ArraySet / FieldGet / string / …）就是 match 里一个新特例;而且我们
反复在**手写通用编译优化**：
- `jit-inline-fastpaths` 方案 B 的「提指针到入口」= **循环不变量外提（LICM）**的手写特例。
- 内联去箱 = **lowering 到原生 + 值表示优化**的手写特例。

一个真正的优化 JIT 会**通用地**做这些。我们被迫手写的根因有二：
1. **多数操作走 opaque `extern-C` helper** → Cranelift 优化器看不进去,无法自动 hoist/fold/CSE。
2. **堆对象内存布局对 JIT 不可见/不稳定**（std `Vec`、parking_lot `Mutex` 的字段偏移不受 z42 控制）
   → 只能靠 helper 中转,不能直接发原生访存,于是每个访存都得手写一个「安全 helper + 原生 load」特例。

## What Changes（北极星架构）

把 JIT 从「巨型 match 特例」重构为**可组合的 lowering 管线**,让「加优化 = 加一条规则」,而横切
优化（hoist/fold/DCE）交给 **Cranelift 现有优化器**自动做：

```
z42 bytecode
 ├─(0) 地基：堆对象 #[repr(C)] 稳定布局 → JIT 可见偏移，原生访存不再靠 helper/猜偏移
 ├─(1) 类型定向 lowering 框架：每 op 一条 rule（类型已知→原生 Cranelift IR，否则→helper 回退）
 │      加 fast path = 加一条 rule（不改核心循环）
 ├─(2) 去箱 pass：基元类型寄存器驻留 Cranelift SSA 值，只在 helper 边界装箱（通用 pass，非 per-op）
 ├─(3) Cranelift 优化器：GVN/LICM/DCE → 手写的 hoist/fold 全自动化，新 op 自动受益
 └─→ 机器码
```

**关键洞察**：一旦访存/操作是**原生 Cranelift IR**,方案 B 我手写的「提指针」就由 Cranelift 的 **LICM
自动完成**,且对 field/string/所有访存 generalize —— 不再逐个手写。

## 迁移计划（甲：渐进，每步可验可回退）

| 阶段 | 内容 | 产出 |
|------|------|------|
| **P0 地基** | 给 `RegionEntry`/`ArrayObj`/`ScriptObject` 加 `#[repr(C)]` + 导出稳定偏移常量（JIT 与 Rust 共用单一真相）；z42 自控数组存储（避免依赖 std `Vec` 布局） | JIT 可发原生访存 |
| **P1 框架骨架** | 抽出 `LoweringRule` 接口（`fn lower(ctx, instr) -> Lowered`）；`translate_function` 改为「查 rule 表 → 原生 emit / helper 回退」；**把现有 array 内联迁进框架作模板** | 「加 rule 即扩展」验证 |
| **P2 原生访存 rules** | 用 P0 的稳定偏移,把 ArrayGet/Set、FieldGet/Set 写成**原生 Cranelift 访存 rule**（不再 helper）；Cranelift LICM 自动 hoist loop-invariant base（取代方案 B 手写提指针） | field/array 全原生,自动 hoist |
| **P3 去箱 pass** | 基元类型寄存器 → Cranelift SSA 值;materialize 到 boxed slot 只在 helper 边界 | 全函数级去箱,Cranelift 全优化生效 |
| **P4+** | string / Div-Rem / 调用内联 等作为**新增 rule** 逐步加 | 持续扩展 |

每阶段：先测该阶段上限 → 实施 → 全面验证（jit==interp 逐字节 + full GREEN + jit e2e + `cargo test gc`
+ 自举不动点）→ 提交。任一阶段收益不达预期即停/记录（沿用「先测再投」纪律）。

## Scope（P0+P1，本次先做；后续阶段各自扩）

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/runtime/src/gc/region.rs` | MODIFY | `RegionEntry` `#[repr(C)]` + 稳定偏移常量导出 |
| `src/runtime/src/metadata/types.rs` | MODIFY | `ArrayObj`/`ScriptObject` `#[repr(C)]` + 偏移常量;评估 z42 自控数组存储 |
| `src/runtime/src/jit/lowering/`（NEW 目录） | NEW | `LoweringRule` 接口 + rule 表 + 现有 fast path 迁入 |
| `src/runtime/src/jit/translate.rs` | MODIFY | 主循环改为查 rule 表 + helper 回退;保留控制流/异常派发 |
| `docs/book/src/runtime/jit-lowering.md` | NEW | 管线架构、rule 接口、去箱、布局契约（知识上浮） |

## Out of Scope
- P3 去箱 pass、P4 调用内联（各自单列 spec,P0/P1 定型后）
- AOT（LLVM）
- 改 interp（仅 JIT 管线）

## Open Questions（需 User 裁决）
- [ ] **方向甲 vs 乙**：甲=渐进（P0→P1→…,每步可验,推荐）;乙=一次性上完整 typed-SSA 中端（收益最大、风险/工期最大）。
- [ ] **数组存储自控**：P0 是否把 `ArrayObj.elems` 从 std `Vec<Value>` 换成 z42 自控的 `#[repr(C)]` 定长
      buffer（消除对 std `Vec` 布局的依赖,让 JIT 原生访存 100% 稳定）?这会触及分配/GC 扫描路径。
- [ ] **repr(C) 对 GC 的影响**：`RegionEntry` 的 `Mutex<T>`/原子字段布局固定后,需重验 GC 并发标记路径。
