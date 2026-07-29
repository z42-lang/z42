# Tasks: JIT 内联去箱快路

> 状态：🔴 DRAFT | 创建：2026-07-29 | 待 User 确认后进 Phase 4a 实施

## 进度概览
- [x] 上限 spike：数组读内联天花板 3.31×（已测，见 proposal）
- [ ] 规范确认（proposal + spec + design）→ 阶段 6.5 gate
- [ ] Phase 4a：ArrayGet/Set i64/f64 内联（方案 A：安全 data 指针 + 原生 load）
- [ ] Phase 4b：FieldGet/Set 内联（后续 spec）
- [ ] Phase 4c：跨表达式去箱（后续 spec）

## Phase 4a: ArrayGet 内联（方案 A）✅ i64 读已落地（2026-07-29）
- [x] 4a.1 `jit_array_data` 安全 helper（helpers/array.rs）：返回 elems 数据指针 + len
- [x] 4a.2 内联 emitter（translate.rs ArrayGet 臂）：调 data → 原生 bounds → 原生 load → 去箱存
- [x] 4a.3 `ArrayGet` 臂接线：reg_types[dst]==I64 && [idx]==I64 → 内联,否则 helper
- [x] 4a.5 越界 → 冷块复用 `jit_array_get`（异常消息/类型逐字一致，实测 jit==interp）
- [x] 4a.7 harness 实测：数组循环 **834→655ms = 1.27×**（低于 3.31× 上限——每 get 一次
      `jit_array_data` 调用无法被 Cranelift 提出循环，是方案 A 的固有上限）
- [x] 4a.8 综合验证：`cargo build --all-targets` + full GREEN + `test e2e --mode jit` 8/8 +
      手工 jit==interp 逐字节 + OOB 异常一致
- [ ] 4a.4 `ArraySet` i64 对称内联（后续）
- [ ] 4a.6 golden e2e 正式用例（当前靠手工 + jit-fixpoint CI 覆盖）
- [ ] 4a.9 文档：jit 机制页记录内联快路 / 去箱边界 / fallback（归档前）
- [ ] 4a.10 f64 元素内联（后续）

> **方案 A 收益 1.27× 低于设计 ~2× 目标** → 按 design Decision 1 触发「评估方案 B」。方案 B
> （loop-invariant 提指针）逼近 3.3×,但需 dominance/null-safety 分析（naive 提到 entry 会让
> null 数组异常时机漂移）,是独立设计任务。方案 A 作为正确的原生 load/去箱/OOB 基础先落地。

## 中断/停止条件（沿用纪律）
- 4a.7 实测收益 < ~1.3× → 停,记录,评估方案 B 或放弃（不硬上）
- 去箱触及 GC 安全边界不确定 → 停下问 User
- 任一阶段 JIT≠interp 等价失败 → 停,根因修

## 备注
- 方案 A 拿部分收益（每 get 一次 helper 未消）；完整 3.3× 需方案 B/C（Deferred）
- 与 PR #69（perf-vm-iteration，interp 优化）物理隔离,独立分支
