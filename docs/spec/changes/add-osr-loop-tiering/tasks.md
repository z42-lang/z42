# Tasks: OSR / 循环回边分层

> 状态：🟢 核心完成（阶段1+2 验证通过）| 创建：2026-08-01 | 类型：vm

## 进度概览
- [x] 阶段 1：JIT OSR 入口编译（translate + compile + 缓存）
- [x] 阶段 2：interp 回边计数 + OSR 触发 / handoff
- [ ] 阶段 3：配置 + 测试 + 基准 + 文档

## 阶段 1：JIT OSR 入口编译
- [x] 1.1 `translate_function` 加 `osr_entry: Option<usize>`；Some(K) 时 prepend 入口块
      `br cl_blocks[K]`（跳过 block 0 prologue），None 时现状不变
- [x] 1.2 `JitFrame::from_interp_regs(&[Value], max_reg)`（同构寄存器直接构建）
- [x] 1.3 `LazyCompiler::compile_osr(id, K)` → `get_finalized_function` → FnEntry
- [x] 1.4 `JitModuleCtx.osr_entries: Mutex<HashMap<usize, OnceLock<FnEntry>>>` +
      `resolve_osr_entry(id, K)`（命中返回 / 否则编译缓存；不可翻译 → None）
- [x] 1.5 顺带填普通 entry（Decision 6：后续调用也走原生）

## 阶段 2：interp 回边计数 + OSR 触发
- [x] 2.1 `Frame` 加 `back_edge_count: u32`，`Frame::new*` 初始化 0
- [x] 2.2 `Br` 向后跳转（已有 `target <= block_idx`）累加计数；`BrCond` 补同款向后判定 + 累加
- [x] 2.3 计数 == `osr_threshold` 且 `jit_ctx` 已发布 → `resolve_osr_entry(func_id, target)`
- [x] 2.4 Some(entry)：`JitFrame::from_interp_regs` → push_frame → native → pop_frame →
      marshal `ExecOutcome`（Returned/Thrown）→ `return`（干净退出 exec_function_body）
- [x] 2.5 None（不可翻译/无 ctx）：照常 `block_idx = target` 继续解释（零行为变化）
- [x] 2.6 func → merged id 解析（osr 触发需 id；参照 central divert 的 name→id 或直接带 id）

## 阶段 3：配置 + 测试 + 文档
- [x] 3.1 `Z42_OSR_THRESHOLD` env（config.rs），默认基准定，clamp ≥ 1
- [ ] 3.2 `osr_tests.rs`：触发 / 不触发 / 结果一致 / 不可翻译不 OSR
- [x] 3.3 golden `osr_loop_once_called`：e2e interp==jit byte-identical
- [x] 3.4 基准：04_c2_p1_arith_loop 阈值 1000 + OSR → jit 墙钟 ~480ms→~114ms（回边升级）
- [ ] 3.5 GREEN：`xtask test` 全绿 + `cargo test --lib`（门禁不含 cargo test，必单独跑）
- [x] 3.6 文档：`jit-lazy-compile.md` 新增「OSR / 循环回边分层」机制节（数据结构 + mermaid）
- [ ] 3.7 `docs/roadmap.md` 分层进度更新（如涉及）

## 备注
- 单向，无 deopt（Decision 7 一脉）。
- 可行性核心：寄存器在 frame 内存（非 SSA）+ interp/JIT 同构寄存器 → 状态交接零障碍。
- 风险守卫：回边计数只在向后跳转累加（前向零开销）；OSR 只作用可翻译函数（无 ref 参数）；
  handoff 前刚查过 safepoint。
