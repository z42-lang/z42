# Proposal: OSR / 循环回边分层（on-stack replacement）

## Why

分层执行（runtime-jit-tiering）按**调用次数**决定编译：一个函数被调 ≥ `jit_threshold`
次才升级为原生码。这有个死角——**「被调一次、但内部循环极热」的函数永远不升级**。

实测（bench 04_c2_p1_arith_loop）：`SumSquares(10M 循环)` 被 `Main` 只调**一次**，
call-count = 1 < 阈值 → 全程解释执行 → 阈值 1000 下比阈值 1（首调即编）**慢 4.2×**
（480ms vs 114ms）。任何阈值 ≥ 2 都有这个惩罚，因为 call-count 分不清「一次性 init」
与「一次调用但循环极热」——两者调用数都是 1。

**根治**：按**循环回边数**（loop back-edges = 循环迭代数）分层。解释器执行一个热
循环到一定迭代数时，**就地**把该函数编译成原生码、把当前活跃寄存器状态交给原生码、
**从循环头继续跑**（不等下次调用——这是一次性函数唯一能提速的路径）。这就是 OSR
（On-Stack Replacement），JVM / V8 等成熟 JIT 的标准机制。

## What Changes

- 解释器在**向后跳转**（loop back-edge，已有 `target <= block_idx` 检测点）处累加
  per-activation 回边计数；跨 `osr_threshold` → 触发 OSR。
- JIT 支持**从循环头 block 进入**编译（`translate_function` 加 `osr_entry`）：cranelift
  入口块直接 `br` 到循环头对应 block，跳过 block 0 的 prologue。
- 触发时：编译 OSR 变体 → 用当前 interp `frame.regs` 建 `JitFrame` → 调原生从循环头
  继续 → 返回值 marshal 回解释器的调用者。
- 可行性关键（z42 特有）：**寄存器都在 `frame.regs`（内存）** 而非 SSA，且 **interp 与
  JIT 寄存器模型同构**（都按 IR reg 号索引）→ 状态交接就是拷贝一个 Vec，block 0..K
  定义的值早已由解释器写进 `frame.regs`，原生从内存读即可。
- `Z42_OSR_THRESHOLD` 配置（默认待基准定，clamp）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/interp/mod.rs` | MODIFY | `Frame` 加回边计数字段；`Br`/`BrCond` 向后跳转处累加 + 跨阈值触发 OSR 交接 |
| `src/runtime/src/jit/translate.rs` | MODIFY | `translate_function` 加 `osr_entry: Option<usize>`；Some(K) 时 prepend 入口块 `br cl_blocks[K]`，跳过 prologue |
| `src/runtime/src/jit/mod.rs` | MODIFY | OSR 编译入口（`compile_osr_entry`）+ 从 interp 触发的 handoff 封装 |
| `src/runtime/src/jit/lazy.rs` | MODIFY | `LazyCompiler::compile_one` 支持 osr 变体（复用 translate 的 osr_entry） |
| `src/runtime/src/jit/frame.rs` | MODIFY | `JitModuleCtx` 加 OSR-entry 缓存（`func → OSR FnEntry`，一函数一循环头）+ resolve |
| `src/runtime/src/config.rs` | MODIFY | `Z42_OSR_THRESHOLD` env 定义 |
| `docs/book/src/runtime/jit-lazy-compile.md` | MODIFY | 新增「OSR / 循环回边分层」机制节 |
| `src/tests/e2e/osr_loop_once_called/source.z42` | NEW | golden：一次性调用、内部大循环的函数，输出与 interp 逐字节一致 |
| `src/runtime/src/jit/osr_tests.rs` | NEW | 单测：回边计数触发 OSR、状态交接正确、结果一致 |
| `docs/spec/changes/add-osr-loop-tiering/*` | NEW | 本 change 文档 |

**只读引用**：
- `src/runtime/src/gc/safepoint.rs` — 复用其 back-edge 检测点心智
- `src/runtime/src/interp/exec_call.rs` — 参照 `try_native_static_call` 的 JitFrame handoff 模式

## Out of Scope

- **多循环头 / 嵌套循环的最优 OSR 点选择**：v1 只在「第一个跨阈值的向后跳转目标」做 OSR
  （最内层热循环头），不做循环嵌套分析。
- **deopt（原生→解释器回退）**：单向，与现有分层一致（philosophy 不做兼容路径）。
- **OSR 变体的跨线程共享缓存优化**：v1 每函数缓存一个 OSR entry，够用。

## Open Questions

- [ ] `osr_threshold` 默认值（基准定：既要 SumSquares 及时升级，又不能让短循环白编译）。
- [ ] OSR 编译是否**同时**产出普通 entry（供后续调用走原生）——倾向是（一次编译两用）。
