# Design: interp frame-lock 瘦身

## 数据结构

`VmContext` 新增 4 个发布长度原子（`Mutex` 之外，与既有 `next_frame_id: AtomicU32` /
`jit_ctx: AtomicUsize` 同一放置模式）：

```rust
stack_obj_len: AtomicUsize,   // 镜像 stack_arena.objs.len()
stack_arr_len: AtomicUsize,   // 镜像 stack_arena.arrs.len()
struct_len:    AtomicUsize,   // 镜像 struct_arena.slots.len()
transient_len: AtomicUsize,   // 镜像 transient_arena.slots.len()
```

## 不变式：单写者

这 4 个原子的**唯一写者是 mutator 线程**，写点只有两处，都在对应 arena 的 `Mutex` 锁内：

1. **alloc**（长度 +1）：经四个 `VmContext` 包装方法发布
   `store(idx as usize + 1, Relaxed)`（`idx` = alloc 返回的旧长度）。
2. **pop_frame 的 truncate**（长度回落）：truncate 后 `store(arena.len(), Relaxed)`。

GC 扫描线程**从不写、也不读**这些原子——它在 arena 的 `Mutex` 下读 arena **数据**（`scan_roots`）。

⇒ 单写者 + 读者同为 mutator 线程 → `Relaxed` 足够：一个线程总能按程序序观察到自己最新的写。

## alloc 漏斗（完整性核对）

长度改变**只**发生在 alloc（增）与 truncate（减）。全仓核对（grep）：

- **增**：`StackArena::alloc_obj` / `alloc_arr`、`StructArena::alloc`、`TransientArena::alloc` —— 共
  13 个调用点，全部改走 `stack_alloc_obj` / `stack_alloc_arr` / `struct_alloc` / `transient_alloc`
  包装（含 jit helpers 2 处、tests 2 处；JIT 也走同一 `push_frame`/`pop_frame`，故必须一并入漏斗）。
- **减**：三个 arena 的 `truncate` —— 生产代码里**只有 `pop_frame`** 调用（`exec_struct_tests.rs`
  的 `arena.truncate` 是对局部 arena 的单测，不经 ctx，不影响原子）。
- `copy_into` 不改槽数（读 src 写 dst，两者预分配）→ 不入漏斗。
- 无任何 `.clear()` / `mem::take` 重置 ctx 的这三个 arena。

## push_frame / pop_frame

```
push_frame:
  frame.stack_obj_base = stack_obj_len.load(Relaxed)   // 无锁
  frame.stack_arr_base = stack_arr_len.load(Relaxed)
  frame.struct_base    = struct_len.load(Relaxed)
  frame.transient_base = transient_len.load(Relaxed)
  call_stack.lock().push(frame)                          // 唯一的锁

pop_frame:
  f = call_stack.lock().pop()                            // 唯一必然的锁
  if stack_obj_len != f.stack_obj_base || stack_arr_len != f.stack_arr_base:
      lock stack_arena; truncate; re-publish 两个长度    // 仅本帧增长过才进
  if struct_len != f.struct_base:
      lock struct_arena; truncate; re-publish
  if transient_len != f.transient_base:
      lock transient_arena; truncate; re-publish
```

## Race 分析（与 GC 扫描并发）

GC 在 safepoint 扫描 arena（`scan_roots`，持 arena `Mutex`）。本改动下：

- **push 无锁读原子**：与 GC 无交集（GC 不碰原子）。base 值用于本帧后续 pop 的 truncate 目标，
  纯 mutator-局部语义。
- **pop skip 分支**（发布长度 == 戳记 base，本帧未增长）：pop 对 arena **零操作** → 无论 GC 是否
  正在扫描，arena 数据不变 → 一致。
- **pop truncate 分支**：truncate 在 arena `Mutex` 内 → 与 GC 的 `scan_roots` 互斥。GC 要么在 truncate
  **前**扫（看到待释放槽仍是合法存活 `Value`，安全）、要么**后**扫（已释放，安全）。
- **skip 判定的正确性**：`发布长度 == base` ⟺ 本帧在该 arena 上净增长为 0。因为只有 mutator 分配，
  且被调用者（callees）按 LIFO 已 pop 回其 base（≥ 本帧 base），故相等即「本帧没有任何该 arena 分配
  存活」→ 无需 truncate。发布长度 < base 不可能（本帧只会在 base 之上增长；callees 回落到 ≥ base）。

## 稳健性：漏斗即使被绕过也不崩

若某个未来 alloc 站点漏接漏斗（发布长度偏小）：push 戳记一个偏小的 base，pop 的 skip 判定
`发布长度(偏小) == base(偏小)` 可能相等 → 跳过 truncate → 该帧分配**泄漏**（不回收），但**不崩溃**——
后续帧的 `frame_id` staleness 守卫仍拒绝跨帧读误命中。即失败模式是「性能退化 + 内存泄漏」，非「读到错
值 / UB」。（这只是稳健性兜底，不是允许漏接的借口——13 站点已全接。）

## 收益 / 验证

- 前端 typecheck **4.757s → 4.617s = 2.9%**，`--dump-bound` 输出**逐字节一致**（sha 相同）。
- profile：`push_frame` 197→125、`pop_frame` 151→110。
- 天花板 spike（跳过全部 6 arena 锁、输出一致）= 3.9%；本实现 2.9%（差额 = push 的 4 个必需
  `Relaxed` load，spike 硬设 0 省掉、正确实现不能省）。
- 无格式 / wire / 语义变更；自举字节不动点（gen1==gen2）。
