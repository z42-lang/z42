# Design — perf-vm-iteration Phase 1/2（调用路径去锁 + per-object 锁）

状态：🔴 DRAFT（2026-07-29，待 User 裁决关键决策）

> 本文只覆盖需要**架构裁决**的 Phase 1/2；Phase 0（度量）已落地，Phase 3/4 的
> 非侵入项（interp 微优化 / JIT 去箱内联）待 Phase 1/2 定案后单独设计。

## 度量证据（Phase 0，见 MODE-COMPARISON.md）

扣除启动后单操作成本:

| | interp | jit（默认模式） |
|--|-------:|------:|
| Fib 每 call | ~483ns | ~103ns |
| poly 每 vcall | ~467ns | ~102ns |

每次 (v)call 两个引擎都付 **3 把共享 `call_stack` 锁**（push_frame / pop_frame /
update_caller_line）+ 分配（interp: regs Vec + args Vec + 2×Arc 帧名；jit: JitFrame + args）。
`update_caller_line` 实测在 exec_instr.rs:131/169/238（Call/VCall/CallIndirect）每调用点触发。

## 关键约束:`call_stack` 是 GC root，且有并发标记线程

- `call_stack: Arc<Mutex<Vec<VmFrame>>>`（vm_context.rs:369）——Mutex 是 **multithreading
  foundation 的前瞻设计**（VmFrame 注释 81-91：GC 扫描器是指定的跨线程读者）。
- GC 是**并发标记**（arch: 后台 mark 线程 P4 + SATB 写屏障；arc_heap.rs:1884），`call_stack`
  在收集时作为 root 被扫描（vm_context.rs:633 `for frame in ctx.call_stack.lock().iter()`）。
- VmFrame 的 `line/column` 是 `Cell<u32>`，**已经依赖单线程不变量**（注释 88-89）——即 Mutex
  实际只保护 Vec 的**结构**（push/pop 的 realloc）不被并发 root 扫描撞见，不保护 Cell 内容。

**⚠️ 这意味着能否去掉这 3 把锁,取决于 root 扫描与并发标记的确切交互**（root 是否在 STW 暂停点
一次性快照？还是标记期间后台线程会并发迭代 call_stack？）。这是 GC 并发正确性问题,**必须由掌握
GC 设计意图的 User 裁决,Claude 不擅自改**（CLAUDE.md 设计完整性 + 事实校正 + 停下问 User）。

## 需要裁决的决策

### Decision 1：`call_stack` 的锁策略

**问题**：单线程执行下每 call 付 3 把 uncontended 锁（ARM 上每把是带内存序的原子 RMW，约数 ns），
poly 场景 jit_call 的 push+pop 就吃掉约 20%。能否降到 0？

- **选项 A（推荐，若 GC root 扫描是 STW 快照）**：`call_stack` 改为 **per-VmContext 线程本地、
  无锁**（`UnsafeCell<Vec<VmFrame>>` 或 `RefCell`），GC 在 STW safepoint 暂停 mutator 后再扫
  root（此时无并发 push/pop）。push/pop/update 变纯写,零原子。
  - 前提:确认 root 扫描只发生在 mutator 被 safepoint park 之后（SATB 常见做法）。
  - 收益:两引擎每 call 省 3 把锁;poly jit 预计 −15~20%。
- **选项 B（保守）**：保留结构锁,但**消除 `update_caller_line` 那把**——VmContext 缓存栈顶
  frame 的位置 Cell 裸指针（push/pop 时维护）,update 直接写 Cell 不锁。省 1/3 的锁,不碰 root 扫描语义。
- **选项 C**：维持现状,不动锁,只做分配层优化（Decision 2/3）。

### Decision 2：per-object `Mutex<T>`（根因 A / "F1"，Phase 2）

`gc/region.rs` 每个堆对象带 `Mutex<T>`,每次 field/array 访问加解锁。同样与并发标记/写屏障耦合。
是否引入「单线程无锁快路 + 仅并发 mark 时 park」需 GC 设计裁决。**建议 Phase 1 定案后单列 DRAFT**。

### Decision 3：分配层优化（无 GC 并发耦合，Claude 可自主推进）

这些不碰锁/不碰 GC 并发,属安全可自主项,建议先并行落地:
- **regs Vec 池化**（VmContext 持单线程 free-list,frame 退出归还）——省 interp 每 call 1 次 malloc/free。
- **args Vec**：`collect_args` 改传 scratch buffer / SmallVec。
- **interp 帧名**：⚠️ 记忆 `interp-frame-string-cache-regresses` 记录 OnceLock 缓存曾 −7%
  （疑似撑大热 Function 伤 cache）。若做,放**已 boxed 的 FunctionCold**（不撑热结构）并 harness 实测,
  regress 即回退。interp-only、低 ship 权重,优先级最低。

## 推荐路径

1. **先落 Decision 3 的 regs 池化 + args scratch**（安全、可自主、可测),用 harness 验收。
2. **Decision 1 请 User 在 A/B/C 中裁决**（取决于 GC root 扫描是否 STW 快照）。
3. Decision 2 待 1 定案后单列 DRAFT。

## Testing Strategy
- 每项前后 `bench/scripts/compare-modes.sh`,记录 before/after 到 MODE-COMPARISON.md。
- 完整 GREEN：`xtask test`（含 e2e 异常栈回溯 golden + cross-zpkg catch）+ 自举不动点。
  锁/GC 相关改动额外跑 `cargo test` 的 gc safepoint / concurrent_mark 套件。
