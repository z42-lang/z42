# Design: Inline JIT safepoint fast-path（load/store 重构版）

> 2026-08-01 复活。前一版（2026-06-03，`git revert 31cee6c1`）用 `atomic_rmw.i32 Sub`
> 内联，在 Linux x86_64 上 bench `04_c2_p1_arith_loop` panic（exit 101）。本版**改变机制**——
> 快路径从原子 RMW 降级为**普通 load/store**，从根因上绕开 `atomic_rmw` 的 x86_64 lowering。

## 背景：为什么这是高价值杠杆

- 默认执行模式是 **JIT**（`main.rs`：`the build default (jit if compiled in, else interp)`），
  真实程序走 JIT，interp 是冷代码。JIT 热路径的成本才是系统性的。
- JIT 在**函数入口 + 每条后向 Br/BrCond + 每次 Call/CallIndirect 返回**共 5 处 emit
  `call jit_check_safepoint(frame, ctx)`（`translate.rs`）。helper 调用 ~10ns（caller-save
  spill + jump + return），而 fast path 本体只是"减计数器 + 比较 + 分支" ~3-5ns。bench 注释：
  热循环剩余成本约一半在这个 helper 调用上。
- 去掉 **调用开销**（把 fast path 内联进原生码）是唯一同时命中"默认路径 + 每条回边"的系统性优化。

## 与前一版（blocked）的关键差异

| 维度 | 前一版（blocked） | 本版 |
|------|------------------|------|
| 计数器递减 | `atomic_rmw.i32 Sub`（x86 `LOCK XADD`） | `load.i32` + `iadd_imm -1` + `store.i32`（裸 `mov`） |
| x86_64 lowering | ❌ panic（atomic_rmw sub 对非-repr(C) 字段） | ✅ 普通 load/store 无 atomic_rmw，不触发该 lowering |
| Rust 侧 `check_safepoint` | 不变（仍 `fetch_sub`） | 同步改为 `load`+`store`（保持两侧机制一致） |
| 每次迭代成本 | 1 条 locked 指令 | 2 条裸 `mov`（无 lock 前缀） |

## Decisions

### Decision 1: 快路径计数器改为普通 load/store（去原子 RMW）

**问题**：`safepoint_skip` 是 `AtomicU32`，fast path 用 `fetch_sub(1, Relaxed)`。原子 RMW 在
x86 是 `LOCK` 前缀指令（串行化 store buffer，~5-20 cycle），且其 Cranelift 内联在 x86_64 上
panic。

**关键事实**：`safepoint_skip` 是**每-mutator 单写**的——生产代码只有属主线程读写它。唯一的
跨线程写是 `VmContext::force_safepoint()`，其 doc 明说"For tests and embedders … production
code should not need this"。所以 fast path 的 RMW 原子性**对正确性不必要**：单写者读-改-写不会与
自己竞争。

**决定**：fast path 改为 `let prev = load(Relaxed); store(prev-1, Relaxed); if prev > 1 { return }`。
- Rust 侧（`gc/safepoint.rs`）：`fetch_sub` → `load` + `store`。
- JIT 侧：emit 普通 `load.i32` / `store.i32`（裸 mov），非 `atomic_load`/`atomic_store`（后者
  x86 seqcst 可能 lower 成 `xchg`/`mfence`，重新引入开销）。对齐 u32 的 load/store 在所有真实
  目标（x86/aarch64）硬件天然原子，torn read/write 不会发生。

**GC 存活性不变**：漏掉一次 early-poll 请求被 throttle N（默认 1024）兜住——本就是设计的延迟上界
（~50us @ 典型 50ns/iter），与 RMW 版一致。

### Decision 2: JIT 每 site 拆两个本地 block（fast / slow）

沿用 `translate.rs` 既有拓扑（如 672-677 的异常检查 ok/exc block + `seal_all_blocks()` 收尾）。
`emit_safepoint_check` emit：

```
v_vmctx   = load.i64 trusted, [ctx_val + JIT_MODULE_CTX_VM_CTX_OFFSET]
v_prev    = load.i32  trusted, [v_vmctx + VM_CONTEXT_SAFEPOINT_SKIP_OFFSET]
v_new     = iadd_imm v_prev, -1
            store.i32 trusted, v_new, [v_vmctx + VM_CONTEXT_SAFEPOINT_SKIP_OFFSET]
v_cond    = icmp_imm ugt v_prev, 1
brif v_cond, fast_blk, slow_blk

slow_blk:  call jit_check_safepoint_slow(frame, ctx); jump fast_blk
fast_blk:  ← 后续逻辑继续 emit 到这里（成为当前 block）
```

每 site 2 个 block（Cranelift block 廉价）；block 不在此处 seal，收尾 `seal_all_blocks()` 统一处理
（与文件既有约定一致，见 610-611 注释）。

**为什么不共享单个 slow_block**：每 site 的后续逻辑不同，共享 slow_block 回跳后还要分支决定下一步，
反而复杂。每 site 本地两 block + 直接 jump 是最简单拓扑。

### Decision 3: slow helper `jit_check_safepoint_slow`

slow 分支（counter 命中 0）需"reset counter + 跑 slow check"。新增 helper：

```rust
pub unsafe extern "C" fn jit_check_safepoint_slow(frame, ctx) {
    let vm_ctx = vm_ctx_ref(ctx);
    vm_ctx.safepoint_skip.store(throttle_n(), Relaxed);   // reset
    gc::safepoint::check_safepoint_slow(vm_ctx);           // Mutex + phase + auto-collect drain
}
```

需把 `gc::safepoint::check_safepoint_slow` 提为 `pub(crate)`。保留旧 `jit_check_safepoint`
（测试直接调 + 作 reference / fallback）。

### Decision 4: offset 常量用 `offset_of!`

- `vm_context.rs`：`pub const VM_CONTEXT_SAFEPOINT_SKIP_OFFSET: usize = offset_of!(VmContext, safepoint_skip);`
- `jit/frame.rs`：`pub const JIT_MODULE_CTX_VM_CTX_OFFSET: usize = offset_of!(JitModuleCtx, vm_ctx);`

`offset_of!`（stable 1.77+）编译期求值、与字段重排无关，`#[serde(skip)]`/`#[repr(Rust)]` 不影响。

## x86_64 风险与验证策略（必须认清）

前一版正是栽在**本地（macOS aarch64）过、CI（Linux x86_64）panic**。本版从机制上去掉 `atomic_rmw`
（panic 的直接对象），**但 x86_64 lowering 仍只能靠 CI 验证**——本地 aarch64 通过 ≠ x86_64 通过。

- 本地验证：`cargo test --lib`（含 JIT-compiled safepoint 单测，覆盖 aarch64 lowering）+ `xtask test`。
- CI 验证：push PR 后**必须盯 `bench-update` / `test-vm-jit(linux-x64)` 腿**，确认 x86_64 不再
  panic。红了立即回滚（本版设计成可单独 revert Phase B、保留 Phase A 的 Rust load/store）。
- Phase A（Rust `fetch_sub`→load/store）与 Phase B（JIT 内联）分两个 commit，Phase B 若 x86_64
  仍炸可只回退 Phase B。

## Testing Strategy

- 单元：`jit/helpers/control.rs` 既有 `jit_check_safepoint_*` 测试；补一个 JIT-compiled 循环在
  GC pause 下正确 park 的端到端单测（复用既有 harness）。
- Golden：`xtask test e2e`（含 jit 模式）输出与内联前逐字节一致——安全点内联不改可观察语义。
- GREEN：`cargo build --release` + `cargo test --lib` + `xtask test` 全 stage。
