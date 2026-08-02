# Design: OSR / 循环回边分层

## Architecture

```
解释器 exec_function_body 的 'exec 循环
  │  每条指令 → exec_instr
  │  Terminator::Br / BrCond 解析 target
  │  ┌── target <= block_idx （向后跳转 = loop back-edge，已有检测点）
  │  │      frame.back_edge_count += 1
  │  │      if back_edge_count == osr_threshold:        ← 恰好跨阈值触发一次
  │  │          entry = jit_ctx.resolve_osr_entry(func, target)   // 编译 osr 变体
  │  │          if Some(entry):
  │  │              JitFrame::from_interp_regs(frame.regs)
  │  │              r = native(osr_entry)  // 从 block=target 继续跑完函数
  │  │              return marshal(r)      // 'exec 循环退出，等价于函数返回
  │  └──
  │  block_idx = target; continue 'exec   // 未触发/无 JIT ctx → 照常解释
  ▼
```

JIT 侧：
```
translate_function(func, osr_entry: Option<usize>)
  cl_blocks[0..N] = create_block() ×N          // 每 IR block 一个
  if let Some(K) = osr_entry:
      osr_blk = create_block()                 // 新入口块（prepended）
      append_params(osr_blk); switch_to(osr_blk)
      (frame,ctx) = block_params(osr_blk)
      ins().jump(cl_blocks[K], &[])            // 跳过 block 0 prologue，直入循环头
  else:
      append_params(cl_blocks[0]); switch_to(cl_blocks[0])   // 普通入口（现状）
  … 照常翻译所有 block（含 block 0..K）…       // block 体不变，只是入口不同
```

## Decisions

### Decision 1：回边计数放 interp Frame（per-activation），不放 JitModuleCtx（per-function）
**问题**：OSR 要在「当前正在跑的这次调用」的热循环里触发。
**选项**：A per-activation（Frame 字段，每次调用归零）；B per-function（跨调用累加）。
**决定**：**A**。OSR 的语义是「这次执行的循环够热了就地升级」，per-activation 计数正好；
per-function 累加会让「多次调用、每次循环几下」的函数误触发（那本该走 call-count 分层）。
Frame 加一个 `back_edge_count: u32` 字段，`Frame::new` 初始化为 0，几乎零成本。

### Decision 2：复用已有 back-edge 检测点，只在向后跳转累加
`interp/mod.rs` 的 `Br` 已有 `if target <= block_idx { check_safepoint }`（GC 用）。
OSR 在同一判定里累加计数。**`BrCond` 也要补同款向后判定**（`while` 的回边多是循环体
末尾的无条件 `Br` 跳回条件块 → `Br` 分支已覆盖；但 `do-while` / 某些下沉形态回边可能是
`BrCond` → 一并覆盖，判据同为 `target <= block_idx`）。累加只在向后跳转发生，前向跳转零开销。

### Decision 3：OSR 入口跳过 block 0 prologue —— 靠「寄存器在 frame 内存」成立
普通编译 `cl_blocks[0]` 含 block 0 的 IR 指令（prologue：safepoint、ref copy-in）。OSR 从
block K 进入会跳过 block 0..K。**正确性**：
- **ref copy-in 无需**：OSR 只作用于**可翻译**函数，而带 `ref`/`out` 参数的函数含
  `LoadLocalAddr` → 不可翻译 → 不会 OSR。故无 ref copy-in 要补。
- **safepoint**：解释器刚在这个向后跳转点查过 safepoint（Decision 2 同一点），可安全跳过
  入口 safepoint（原生循环内的 BrCond safepoint 仍在）。
- **block 0..K 定义的寄存器值**：已由解释器写进 `frame.regs`（内存）。原生码读写
  `frame.regs[i]`（`translate.rs` 缓存 `frame.regs.as_mut_ptr()`，raw load/store），从 block K
  进入时这些值就在内存里，直接读 → **等价于解释器执行到此的状态**。这是 z42「寄存器在
  frame 内存、非 SSA」直接使能 OSR 的核心。

### Decision 4：状态交接 = JitFrame 从 interp frame.regs 构建（同构寄存器模型）
interp `Frame.regs: Vec<Value>` 与 JIT `JitFrame.regs: Vec<Value>` **都按 IR reg 号索引、
同一 `Value` 类型**。交接直接 `JitFrame::from_interp_regs(&frame.regs, max_reg)`（clone 或
move 语义待定；活跃寄存器全量拷贝，一次分配）。env_arena（闭包）：OSR 只作用于当前无
StackClosure 逃逸的简单循环函数；若函数用了 stack-closure，v1 判为不 OSR（保守）。

### Decision 5：OSR entry 缓存（一函数一循环头）
`JitModuleCtx` 加 `osr_entries: Mutex<HashMap<usize, OnceLock<FnEntry>>>`（key = 函数 merged
id）。`resolve_osr_entry(id, K)`：命中返回；否则锁编译 `translate_function(func, Some(K))` →
`get_finalized_function` → FnEntry，缓存。同一函数第二次 OSR（另一次调用的热循环）复用。
K 假定稳定（函数的主热循环头）；若同函数不同 K，v1 用首次的 K 的缓存（够用，主循环通常唯一）。

### Decision 6：OSR 编译顺带产出普通 entry（Open Question 落定倾向）
OSR 触发说明该函数确实热。编译时**同时**把普通 entry（`fn_entries_by_id[id]`）填上（同一
`compile_one`，标准入口），使该函数**后续被调用**也走原生。OSR trampoline（entry→br K）单独
产出供本次 handoff。一次编译，两用。（若实现上两入口需两次 translate，则接受两次——OSR 是
稀有事件。）

## Implementation Notes
- `osr_threshold`：`Z42_OSR_THRESHOLD`，默认基准定（起点建议 ~10k 回边——足够滤掉短循环，
  又能让 10M 循环在头 0.1% 迭代内升级）。clamp ≥ 1。
- handoff 后 interp `'exec` 循环必须**干净退出**当前 `exec_function_body`：native 返回值
  → 包成 `ExecOutcome::Returned/Thrown`，`return` 出去。frame 的 FrameGuard/VmFrame 正常 pop。
- 触发点在「已 `resolve` 出 target、未 `block_idx = target`」之间：native 从 target 进入，
  故不要再 `block_idx = target`。
- 线程安全：osr_entries 编译在锁内（同 lazy 的 `compile_one`）；FnEntry 裸指针 code page
  finalize 后稳定（同 lazy-jit 不变量）。

## Testing Strategy
- **单测**（`osr_tests.rs`）：构造「一次调用、内部 N>threshold 回边循环」的函数，断言
  ① 触发 OSR（计数器/编译数）② 结果与纯 interp 一致 ③ 未达阈值的短循环不触发。
- **golden**（`osr_loop_once_called`）：SumSquares 式源，`test e2e --mode jit` 输出与 interp
  逐字节一致。
- **基准**：04_c2_p1_arith_loop 在阈值 1000 + OSR 下，jit 墙钟应从 ~480ms 回落到接近
  阈值 1 的 ~114ms（SumSquares 循环 OSR 后走原生）。
- **GC-stress**：OSR handoff 与 GC safepoint 不冲突（handoff 前刚查过 safepoint）。
- 完整 `xtask test` + `cargo test --lib`（见 memory：门禁不含 cargo test）。

## Deferred / Future Work
- **osr-future-nested-loops**：嵌套/多循环头的最优 OSR 点选择（v1 用首个跨阈值回边）。
- **osr-future-deopt**：原生→interp 回退（当前单向，无需求）。
