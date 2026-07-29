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

> **方案 A 收益 1.27×** → 触发方案 B。

## 方案 B: loop-invariant 提指针 ✅ 已落地（2026-07-29）
- [x] B.1 `written_reg(instr)` 抽出（从 max_reg），单一真相；`max_reg` 复用
- [x] B.2 非抛出 helper `jit_array_data_opt`（null/非数组 → ptr=null,len=0，不抛）
- [x] B.3 入口块提取:never-reassigned 数组寄存器（indexed by i64-eligible ArrayGet 且从不作 dst）
      → cl_blocks[0] emit `jit_array_data_opt` → SSA (ptr,len)（支配全图，全函数可用）；sort 确定序
- [x] B.4 ArrayGet 内联:hoisted 时用 (ptr,len)、per-get 时走方案 A；无符号 bounds 检查同时兜住
      OOB **与** null（hoisted null→len=0→恒 OOB→回退 jit_array_get，在真实访问点抛正确异常）
- [x] B.5 实测:数组循环 **834→388ms = 2.15×**（= 含内存 load 的原生真实上限;252ms 是无 load 下界）
- [x] B.6 正确性 jit==interp:in-bounds / OOB / **重赋值数组(不提取)** / **null 数组 0 迭代(不误抛)**
      / **null 访问(抛同异常)** / **GC-stress(64M 分配下提指针跨 GC 存活)** —— 全部逐字节一致
- [ ] B.7 综合 gate:all-targets + cargo test gc + full GREEN + jit e2e（验证中）
- [ ] B.8 文档:jit 机制页记录提指针机制 + GC 安全论证（归档前）

> GC 安全性实测确认:非移动 mark-sweep + 定长数组（无 realloc）→ 提取的 buffer ptr 在函数执行期
> （跨多次 GC、跨 ArraySet 元素写）始终有效。empirical:arrgc 用例 64M 分配强制大量 GC,jit==interp。

## 中断/停止条件（沿用纪律）
- 4a.7 实测收益 < ~1.3× → 停,记录,评估方案 B 或放弃（不硬上）
- 去箱触及 GC 安全边界不确定 → 停下问 User
- 任一阶段 JIT≠interp 等价失败 → 停,根因修

## 备注
- 方案 A 拿部分收益（每 get 一次 helper 未消）；完整 3.3× 需方案 B/C（Deferred）
- 与 PR #69（perf-vm-iteration，interp 优化）物理隔离,独立分支
