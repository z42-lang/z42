# Tasks: Inline JIT safepoint fast-path（load/store 重构版）

> 状态：🟢 已完成（PR #94 已合并 2026-08-02；x86_64 test-vm-jit 全绿、04_arith jit -53%/2.1×、无 panic——
> 前一版 atomic_rmw 的 x86_64 lowering 阻塞由 load/store 从根因解除） | 类型：refactor + perf | 复活：2026-08-01
> 设计见 [`design.md`](design.md)。
>
> **历史**：前一版（2026-06-03）用 `atomic_rmw.i32 Sub` 内联，Linux x86_64 上 bench
> `04_c2_p1_arith_loop` panic（exit 101），`git revert 31cee6c1`（2026-06-05）恢复 helper 调用。
> 本版**换机制**：快路径 RMW → 普通 load/store，从根因绕开 `atomic_rmw` 的 x86_64 lowering。

## 变更说明

JIT 在函数入口 + 每条后向 Br/BrCond + 每次 Call/CallIndirect 返回共 5 处 emit
`call jit_check_safepoint(frame, ctx)`（~10ns/次）。把 fast path（减计数器 + 比较 + 分支）内联进
原生码，仅 slow path（counter==0，~0.1%）走 helper。计数器递减用普通 load/store（单写者，原子性
不必要），避免前一版 `atomic_rmw` 的 x86_64 panic。

## 原因

`jit_check_safepoint` 在每条热循环后向跳转 fire。默认执行模式是 JIT，故这是默认路径上每条回边的
系统性开销。helper 调用开销 > 实际工作。

## 文档影响

- `docs/design/runtime/vm-architecture.md`（或 book 对应页）JIT codegen 章节加 safepoint inline 节

## Scope（允许改动的文件）

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `src/runtime/src/gc/safepoint.rs` | MODIFY | fast path `fetch_sub`→`load`+`store`；`check_safepoint_slow` 提 `pub(crate)` |
| `src/runtime/src/vm_context.rs` | MODIFY | 加 `pub const VM_CONTEXT_SAFEPOINT_SKIP_OFFSET`（`offset_of!`） |
| `src/runtime/src/jit/frame.rs` | MODIFY | 加 `pub const JIT_MODULE_CTX_VM_CTX_OFFSET`（`offset_of!`） |
| `src/runtime/src/jit/helpers/control.rs` | MODIFY | 加 `jit_check_safepoint_slow`；保留 `jit_check_safepoint` |
| `src/runtime/src/jit/helpers/registry.rs` | MODIFY | 注册 `check_safepoint_slow` FuncId |
| `src/runtime/src/jit/translate.rs` | MODIFY | 加 `emit_safepoint_check`；替换 5 处 `hr_check_safepoint` call |
| `docs/design/runtime/vm-architecture.md` | MODIFY | JIT codegen safepoint inline 节 |

**只读引用**：`src/runtime/src/gc/safepoint_tests.rs`（理解 counter 语义）。

## 进度概览
- [ ] 阶段 A: Rust 快路径 RMW→load/store（安全、可独立 commit）
- [ ] 阶段 B: JIT 内联（offset 常量 + slow helper + emit + 5 site 替换）
- [ ] 阶段 C: 验证（cargo test --lib + xtask test）+ 文档 + 归档

## 阶段 A: Rust 快路径
- [ ] A.1 `gc/safepoint.rs`：`check_safepoint` fast path `fetch_sub`→`load`+`store`；注释说明单写者
- [ ] A.2 `gc/safepoint.rs`：`check_safepoint_slow` 改 `pub(crate)`
- [ ] A.3 `cargo test --lib gc::safepoint` 通过

## 阶段 B: JIT 内联
- [ ] B.1 `vm_context.rs`：`pub const VM_CONTEXT_SAFEPOINT_SKIP_OFFSET`
- [ ] B.2 `jit/frame.rs`：`pub const JIT_MODULE_CTX_VM_CTX_OFFSET`
- [ ] B.3 `jit/helpers/control.rs`：`jit_check_safepoint_slow`（reset + slow）
- [ ] B.4 `jit/helpers/registry.rs`：注册 helper id
- [ ] B.5 `jit/translate.rs`：`emit_safepoint_check(builder, ctx_val, frame_val, hr_slow)`
- [ ] B.6 `jit/translate.rs`：替换 5 处 `hr_check_safepoint` call（entry + 4）
- [ ] B.7 补 JIT-compiled 循环在 GC pause 下 park 的单测

## 阶段 C: 验证 + 文档 + 归档
- [ ] C.1 `cargo build --release` clean + `cargo test --lib` 全过
- [ ] C.2 `xtask test` 全绿（e2e/jit 输出逐字节一致）
- [ ] C.3 `docs/design/runtime/vm-architecture.md` safepoint inline 节
- [ ] C.4 归档 + commit + PR + **盯 CI x86_64（bench-update / test-vm-jit）**

## 备注
- **x86_64 风险**：本地 aarch64 过 ≠ x86_64 过（前一版就栽在这）。Phase A/B 分 commit，Phase B
  x86_64 若仍炸可只回退 B、保留 A。
