# Tasks: JIT 内联去箱快路

> 状态：🔴 DRAFT | 创建：2026-07-29 | 待 User 确认后进 Phase 4a 实施

## 进度概览
- [x] 上限 spike：数组读内联天花板 3.31×（已测，见 proposal）
- [ ] 规范确认（proposal + spec + design）→ 阶段 6.5 gate
- [ ] Phase 4a：ArrayGet/Set i64/f64 内联（方案 A：安全 data 指针 + 原生 load）
- [ ] Phase 4b：FieldGet/Set 内联（后续 spec）
- [ ] Phase 4c：跨表达式去箱（后续 spec）

## Phase 4a: ArrayGet/Set 内联（方案 A）
- [ ] 4a.1 `jit_array_data` 安全 helper（helpers/array.rs）：返回 elems 数据指针 + len
- [ ] 4a.2 `emit_array_get_i64` / `emit_array_get_f64` emitter（translate.rs）：调 data → 原生边界检查 → 原生 load → 去箱存
- [ ] 4a.3 `ArrayGet` 臂接线：reg_types 证明 i64/f64 元素 → 走 emitter,否则 helper
- [ ] 4a.4 `ArraySet` 对称（基元值内联；heap-ref 值维持 helper 走写屏障）
- [ ] 4a.5 越界 → 冷块设异常 + return 1（与 helper 语义逐字一致）
- [ ] 4a.6 golden/e2e：i64/f64 读写、越界异常、非基元回退、ArraySet 写屏障
- [ ] 4a.7 harness 实测：数组循环 834ms → ? ，记录 MODE-COMPARISON
- [ ] 4a.8 full GREEN（xtask test + test e2e --mode jit + 自举 + cargo test gc）
- [ ] 4a.9 文档：jit 机制页记录内联快路 / 去箱边界 / fallback

## 中断/停止条件（沿用纪律）
- 4a.7 实测收益 < ~1.3× → 停,记录,评估方案 B 或放弃（不硬上）
- 去箱触及 GC 安全边界不确定 → 停下问 User
- 任一阶段 JIT≠interp 等价失败 → 停,根因修

## 备注
- 方案 A 拿部分收益（每 get 一次 helper 未消）；完整 3.3× 需方案 B/C（Deferred）
- 与 PR #69（perf-vm-iteration，interp 优化）物理隔离,独立分支
