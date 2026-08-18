# Proposal: interp frame-lock 瘦身 —— push/pop_frame 去掉 6 把 arena 锁

## Why

interp-bound workload（z42c 前端 lex+parse+typecheck，极度 call-heavy）的 profile 里，
**帧管理是仅次于 dispatch 的第二大桶**。根因不是记账语义（GC root / stack-trace 元数据都是正确性
必需），而是**每次函数调用要拿 8 把锁**：

- `push_frame`：`stack_arena` + `struct_arena` + `transient_arena` + `call_stack` = **4 把 `parking_lot::Mutex`**
- `pop_frame`：同样 4 把（3 个 arena truncate + call_stack pop）

三个 arena 用 `Mutex` 是**承重的**——GC 扫描线程在 safepoint 跨线程读它们（`vm_context.rs` 头注释：
「Arc<Mutex<…>> instead of Rc<RefCell<…>> because the GC scanner closure [needs cross-thread access]」），
所以不能退成 `RefCell`。但**这三个 arena 只有 mutator 线程写**（帧内逃逸对象/值 struct/瞬态 payload
分配），GC 线程**只读**（STW 下），锁几乎从不争用——纯粹是每调用 6 次原子 RMW 的白流失。

**实测天花板 spike**（把 push/pop 的 6 个 arena 锁全跳过、输出验证逐字节一致）：
前端 typecheck **4.741s → 4.556s = 3.9% faster**。

不做的代价：call-heavy 工作负载（z42c 自编译是典型）每次调用白付 6 次锁原子。

## What Changes

给三个 arena 各加一个**发布长度原子**（`AtomicUsize`，挂在 `VmContext` 上、在 `Mutex` 之外），
镜像其内部 `Vec` 长度（`stack_arena` 有两个 Vec：objs / arrs）：

- **单写者**：只有 mutator 线程写这些原子——在**每个 alloc**（经新增的 `*_alloc` 包装方法、在 arena
  锁内发布）和 **`pop_frame` 的 truncate** 后。GC 线程从不碰它们（它在 `Mutex` 下读 arena 数据）。
- **`push_frame`**：从原子 `Relaxed` load 取每个 arena 的 truncation base（**无锁**），不再锁三个 arena。
  单写者 + 同线程 → load 一定看到自己最新的 publish。
- **`pop_frame`**：先 pop `call_stack`；然后对每个 arena，**仅当本帧确实增长过**（发布长度 ≠ 戳记 base）
  才加锁 + truncate + 重新发布。call-heavy 常态帧在这三个 arena 上分配为零 → 三个比较全短路 →
  `pop_frame` 只剩 `call_stack` 一把锁。

净效果：`push_frame` 4→1 锁（去 3 arena 锁），`pop_frame` 常态 4→1 锁（去 3 arena 锁）。

**alloc 漏斗（正确性关键）**：所有 arena 分配必须经四个 `VmContext` 包装方法之一
（`stack_alloc_obj` / `stack_alloc_arr` / `struct_alloc` / `transient_alloc`），它们在 arena 锁内
alloc 后发布新长度。绕过包装的裸 `ctx.stack_arena.lock().alloc_obj(..)` 会让原子失准 → `pop_frame`
误跳一次该做的 truncate（arena **泄漏**，非崩溃——`frame_id` staleness 守卫仍保护读）。

**实测收益**：前端 typecheck **4.757s → 4.617s = 1.03× (2.9%)**，输出**逐字节一致**；
`push_frame` 197→125 samples、`pop_frame` 151→110。（低于 3.9% 天花板的差额=push 的 4 个必需
`Relaxed` load，spike 把 base 硬设 0 省掉了这些——正确实现无法省。）

## Scope（允许改动的文件）

- `src/runtime/src/vm_context.rs`：加 4 个原子字段 + 2 构造器初始化 + 4 个 alloc 包装方法 +
  重写 `push_frame` / `pop_frame`。
- **13 个 alloc 调用点改走包装**（interp 6 + jit helpers 2 + tests 2 + 其余）：
  `exec_object.rs` / `exec_array.rs` / `exec_struct.rs` / `exec_address.rs` / `exec_call.rs` /
  `exec_struct_tests.rs` / `jit/helpers/{array,closure}.rs` / `corelib/tests.rs`。
- `docs/design/runtime/vm-architecture.md`：机制文档（发布长度原子 + skip-if-unchanged）。

## 非目标 / 不做

- 不合并三个 arena 的 `Mutex`（方案 A，评估过收益低一截、改动面 56 站点，已否决）。
- 不动 `call_stack` 锁（stack-trace / GC-root 承重，多处独立加锁）。
- 不动 `Value::clone` / drop / bzero 寄存器清零（另有其事，且 bzero 是 GC-safety 承重）。
- 无格式 / wire / 语义 / API 变更；纯内部同步优化。

## 风险

- **并发正确性**（唯一实质风险）：单写者 `Relaxed` 原子 + skip-if-unchanged。已分析：mutator 单写、
  push/pop 同线程读 → 稳定；GC 只在锁下读 arena 数据、不碰原子；skip 分支下 pop 对 arena 无操作 →
  与 GC 并发扫描一致。见 design.md 的 race 分析。
- **漏斗完整性**：所有 13 个 alloc 站点 + 唯一 truncate 站点（pop_frame）已核对（grep 全仓）。
