# Tasks: fix-context-joins-mid-pause

> 状态：🟢 已完成 | 创建：2026-09-15 | 完成：2026-09-15 | 类型：fix（最小化模式）

**变更说明：** 新 `VmContext` 在 collector 处于 `Marking` 时不注册：`new_with_core` 持 `gc_phase` 锁 `while Marking { wait }`
后再 push 进 `vm_contexts`。`Requested` 期照旧允许加入。
**原因：** collector 数完已注册线程都 park 了、翻到 `Marking` 开始扫根和 sweep 之后，新注册的线程既没 park 也没被扫：
它接着跑、往正在 sweep 的区里分配，造出来的对象没人标记 ⇒ `Z42NetHttpServerThreadedTests` 客户端线程刚起就
`ArraySet index: expected non-negative integer, got Null`（刚造的 `HttpHeaders._count` 读回 Null）。
这也是 investigate-concurrent-gc-stale-mark-race 的 3.2b（注册窗口封闭）。
**文档影响：** `.claude/rules/runtime-rust.md`「阻塞的 native 调用必须 park」补一段：这条是注册协议的前提。

## 为什么不踩禁区 3（2026-06-01 死锁）

- 等待不是 park：等的线程**没注册**，没有 collector 在等它。
- 等完后它才去仲裁 CAS，可能赢得 collector 角色、再等所有已注册线程 park。若有已注册线程卡在**没 park 的阻塞调用**里就死锁。
- 这个死锁**不是本修复引入的**：没有任何修复时，一个恰好在停顿释放后注册的线程一样会赢 CAS、一样死锁
  （loom `unparked_join_deadlocks_even_without_a_fix`）。2026-06-01 只是让单测每次都撞上。
- 前提已由 #598（`Thread.Join` 等）+ #648（TLS / 子进程管道 / DNS / 并行 golden join）消掉：阻塞在 VM 外的线程都 park。

## 实施
- [x] 1.1 `vm_context/construct.rs::new_with_core`：注册前等出 `Marking`（锁序 `gc_phase` → `vm_contexts`，与 collector 一致）
- [x] 1.2 `safepoint_tests`：`pause_guard_drop_notifies_waiters` / `second_collector_falls_back_to_mutator_park_returns_none`
      改成 worker 先注册、再模拟停顿 —— 旧写法在 `Marking` 期建上下文，现在是不可能的场景（测的是 park / 仲裁，不是注册）
- [x] 1.3 `cross_thread_smoke::concurrent_gc_mode_stress_no_race_no_leak` 去掉 windows/macos 的 `#[ignore]`（stale-mark 3.4）
- [x] 1.4 规则文档

## 测试
- [x] `safepoint_tests::a_new_context_waits_out_a_stop_the_world_pause`（持 `request_gc_pause` guard，另一线程建上下文，100 ms 后断言未注册）
      —— **修前红、修后绿**
- [x] `safepoint_tests::a_new_context_may_join_while_a_pause_is_only_requested`（`Requested` 期不许被挡住）
- [x] loom 模型 A：`waiting_out_marking_eliminates_race`（绿）；阴性对照：去掉等待 ⇒ 立刻报 stale mark
- [x] loom 模型 B′（新，穷举）：worker 的注册跨过停顿、主线程 `join`
  - `a_parked_joiner_never_deadlocks_with_waiting_registration`（绿）
  - `an_unparked_joiner_deadlocks_with_waiting_registration`（should_panic deadlock —— 模型有判别力）
  - `unparked_join_deadlocks_even_without_a_fix`（should_panic deadlock —— 隐患先于修复存在）
  - `a_parked_joiner_never_deadlocks_without_a_fix`（绿）
- [x] 模型 A 补上 `unregister`（`impl Drop for VmContext`）：否则 mutator 在 Idle 期过完唯一的 safepoint 就带着计数结束，
      后启动的 collector 永远等它 —— 是模型的死锁不是运行时的；无修复那次先撞上 stale mark，所以之前没暴露

## 验证
- [x] loom 两个文件全绿（< 0.1 s）
- [x] `cargo test --lib`（debug）1300 passed；`--test cross_thread_smoke` 9 passed（去 ignore 后 10）
- [x] `xtask test` 全绿（11m34s）
