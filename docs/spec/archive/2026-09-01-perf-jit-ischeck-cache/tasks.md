# Tasks: perf-jit-ischeck-cache

> 状态：🟢 已完成 | 创建：2026-08-31 | 完成：2026-09-01（PR #354）| 类型：perf

**变更说明：** 给 JIT 版 `is_subclass_or_eq`（`src/runtime/src/jit/helpers/object.rs`）加缓存，
与 interp 版共享同一份 per-VmContext `subclass_memo`。

**原因：** `Z42_JIT_THRESHOLD` 默认已=1（#350），z42c 顶层函数一调即 JIT → z42c 的 is-check 走 **JIT arm**。
而 #350 只给 **interp** 版加了 `subclass_memo`——JIT 版每次 `x is T` 仍走完整 base+接口链 walk（含跨包
lazy-loader 锁）。IR 序列化每条指令派发经 ~60 路 is-链，是编译热点。故 JIT 版缓存才是 z42c 提速的真入口
（interp memo 对 z42c ≈0% 墙钟正因 z42c 不跑 interp）。

**做法：** `is_subclass_or_eq` 拆成 memo wrapper（`derived==target` 短路 + 查/填 `vm.subclass_memo`）+
`is_subclass_or_eq_walk`（原 fast/slow 双路 walk）。共享 interp 的 `ctx.subclass_memo`——(derived,target)→bool
是全局单调事实（继承链不变、lazy load 只增类型），模块重载时随 interp memo 一起清（`vm_context::lookup`）。
数组/基元 is-check 在 `jit_is_instance` 已前置处理，只有 object→class 走此，与 interp 同词汇 → 共享安全。

**文档影响：** 无（纯内部性能优化，不改外部行为；产物 byte-identical）。runtime README 无需改（未增删文件/入口）。

## Scope
- `src/runtime/src/jit/helpers/object.rs` — MODIFY：is_subclass_or_eq 加共享 memo wrapper + 原体改名 _walk
- `docs/spec/changes/perf-jit-ischeck-cache/` — NEW

## 任务
- [x] 1.1 JIT is_subclass_or_eq 加共享 memo wrapper（复用 vm.subclass_memo）
- [x] 1.2 cargo build --release（无错）
- [x] 1.3 cargo test --lib（1003 passed, 0 failed）
- [x] 1.4 性能测量：编 z42.core（--release/JIT）clean ~5.86s → cached ~5.12s（**~12%**），产物 byte-identical
- [ ] 1.5 完整 GREEN（xtask test；含 CI test-vm-jit 覆盖 JIT is-check 语义）
- [ ] 1.6 归档 + PR

## 备注
- 无新增单测：is-check 语义由现有 e2e（types/is_instance 等，CI `test-vm-jit(linux-x64)` 跑 JIT 腿）+
  cargo `--lib` 覆盖；缓存透明性由「编 z42.core 产物 byte-identical」佐证。
- Deferred（母调查 [[compiler-parallel-heavy-phases-investigation]]）：IrInstr 整数 opcode 标签把 60 路
  is-链改 O(1) switch；AOT 编 z42c——更大工程，另议。
