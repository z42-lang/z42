# Design: ConcurrentMarkSweep 残留 mark bit race — 根因分析

> 状态：分析中（2026-06-01）。本文记录代码级追踪结论 + 候选修复，待 User 裁决根因后进阶段 3。

## 现象（CI 实测）

- `concurrent_gc_mode_stress_no_race_no_leak` 在 **windows-latest** 上间歇失败，panic：
  `stale mark bit in region_object after sweep: ... type=Leaf, slots=1`
  （[`arc_heap.rs:498`](../../../src/runtime/src/gc/arc_heap.rs#L498) 的 `debug_validate_invariants`）。
- **平台不对称**：windows (x86-64, **强**内存序) 复现；macOS-15 / ubuntu-arm (ARM, **弱**内存序) 通过；本地 Apple Silicon `--release` 也通过。
- 诊断字段确认：残留对象是 worker 循环新分配的 `Leaf`（不是 `Owner` 或老对象）。

## 关键推理

### 1. 不是内存序 bug

强序 x86 失败、弱序 ARM 通过 —— 与典型"缺 fence"bug 的表现**相反**（后者通常在弱序 ARM 上炸）。因此根因是**线程调度时序竞争**（windows CI runner 的调度让某 mutator 在错误的窗口跑了一次 write barrier），不是缺原子屏障。

### 2. 为什么 push-assertion 没先炸

sweep 期间有保护：`sweep_phase` 进入时 `debug_stw_no_push=true`（[`arc_heap.rs:1091`](../../../src/runtime/src/gc/arc_heap.rs#L1091)），退出时置 false（1199）。write barrier 在 **push 路径**上 `debug_assert!(!debug_stw_no_push)`（[1638-1642](../../../src/runtime/src/gc/arc_heap.rs#L1638)）。

但 `mark_if_unmarked(new)`（[1636](../../../src/runtime/src/gc/arc_heap.rs#L1636)）**先**把 mark bit 置 1（CAS 0→1），**之后**才检查 `debug_stw_no_push` 并 push。所以：

- 若 barrier 在 sweep 窗口 [1091,1199] 内 mark 了一个 unmarked 对象 → push-assertion 炸（我们没看到这个）。
- 若 barrier 在 `debug_stw_no_push=false` 时（sweep 已返回）mark 了一个 live 对象 → **bit 被置 1 但不触发 push-assertion**，随后 `debug_validate_invariants`（[1911](../../../src/runtime/src/gc/arc_heap.rs#L1911)）看到 stale mark → 炸。

观测到的是后者：**re-mark 落在 sweep 返回（1199）之后、pause drop（1914）之前的窗口**。

### 3. 该窗口内不该有 mutator 在跑

`collect_cycles_with_context` 的并发臂（[1847-1914](../../../src/runtime/src/gc/arc_heap.rs#L1847)）：Phase4 `request_handshake_pause` 应让所有其它 VmContext 在各自 safepoint park，Phase6 sweep + Phase debug_validate 全程 pause 持有（world stopped），直到 1914 `drop(pause)`。若 handshake 真的"所有 mutator 已 park 且保持到 pause 释放"，window 内不可能有 barrier。

→ **根因候选：handshake 的 parked 保证存在漏洞，某 worker 在 sweep 后、pause 释放前仍能执行一次 write barrier，把一个新 `Leaf` 重新 shade。**

## 候选根因（待 User 裁决细化）

### 候选 A：barrier 不在 safepoint 临界区内，park 计数与"停止触碰堆"不同步
write barrier（mark+push）发生在两次 `check_safepoint` 之间的任意点，不受 phase 锁保护。`request_handshake_pause` 等 `parked_count >= need`（[safepoint.rs:332-339](../../../src/runtime/src/gc/safepoint.rs#L332)）。理论上 collector 会等到每个 worker 到下一个 safepoint 才 park，但 `parked_count` 是跨 cycle 复用的单计数器，且 `park_until_idle` 在 ConcurrentMarking→Marking 的相变切换下，唤醒/重新 park 的时序可能让 collector 在某 worker"恰好处于 barrier 执行中、尚未抵达下一个 safepoint"时误判 `parked_count` 已满足。需对 `parked_count` 增减相对相变的 happens-before 再做一轮形式化核对。

### 候选 B：sweep 的 mark-clear 与 barrier 的 mark-set 缺乏对"该 entry 是否在本轮 STW 内"的判定
更稳健的设计：barrier 在 ConcurrentMarking 之外（即 Marking/STW 期间）根本不应 shade。可让 barrier 在 push 前检查 phase；若 phase==Marking，则该写本身充当 safepoint（先 park 再写），从根上保证 STW 窗口内无 barrier。代价：barrier 热路径加一次 phase 读。

## 约束与障碍

- **无法本地复现**（macOS/ARM 通过），任何修复只能靠 windows CI 反复验证（慢 + 概率性）—— 不满足"本地 GREEN 10/10"的常规验证门槛。
- 属 `vm` 并发语义改动，**correctness-critical**；按 philosophy 不得"放宽 invariant 绕过"，也不得未经验证就推测性 patch 上主干。

## 待 User 裁决

1. 采 A（核 handshake 计数 happens-before，定位精确漏洞）还是 B（barrier 在 Marking 相变为 safepoint，从设计上消除窗口）？
2. 既然本地无法复现：是否接受"在 windows 临时 `#[ignore]` + 本 spec 持续跟踪"作为恢复 CI 绿的过渡（注：与 philosophy"禁 #[ignore] 绕过"冲突，需 User 显式豁免），还是直接按选定根因改 + 用 CI 迭代验证？

## 追加发现（2026-06-01，User 选定 B 后）

### mark 原子序不是元凶
[`region.rs:153-167`](../../../src/runtime/src/gc/region.rs#L153) 的 `mark/clear_mark/is_marked` 全用 `Ordering::Relaxed`。但 bug 在**强序 x86 (windows)** 现、**弱序 ARM** 不现 —— 若是 Relaxed 导致的重排，应在弱序 ARM 上更易现。方向相反 ⇒ 确认是**逻辑时序竞争**（某 mutator 在 STW 窗口跑了 barrier），不是缺 fence。B（结构性消除 STW 期 barrier）方向正确，但**不能靠加 fence**解决。

### handshake 在纸面推演下"看起来是对的"
逐步推演 `park_until_idle` (parked_count add→wait→sub under lock) 与 `request_handshake_pause` (set Marking→wait parked_count>=need) 的所有交错：每个 worker 的 barrier push 都发生在它 re-park 之前，应被 Phase5 drain 捕获；未 resume 的 worker 不跑 barrier。**纸面上 worker 应全程 parked 到 sweep+validate 结束。** 这说明真实漏洞比"候选 A/B 的朴素假设"更隐蔽（可能在：`force_safepoint`/throttle 交互、alloc_object 注册新 entry 与 sweep 的 lock 边界、或某个我尚未读到的相变窗口）。

### 结论：必须先拿到 debug-mode 本地复现
CI 跑的是 **debug**（`debug_validate_invariants` 等全是 `#[cfg(debug_assertions)]`）。先前本地 `--release` 跑当然过——release 下根本没有这条断言。

### 关键新发现 1：本地（macOS/ARM）无法复现，即便极限放大
debug 模式下把 test 放大到 **8 workers × 2000 iters × 4000 collect rounds + write→barrier 间插 yield_now**，单进程跑仍 0.47s 通过。macOS/ARM 调度就是不进窗口。⇒ **任何修复无法本地验证**；只能靠 windows CI（慢 + 概率性，"过一次"也不能证明修好）。

### 关键新发现 2：朴素 B（barrier 在 Marking 期 park）不安全
`alloc_object`（[arc_heap.rs:1485](../../../src/runtime/src/gc/arc_heap.rs#L1485)）**不给新对象染色**（region entry 出生 marked=0，白色）。当前正确性依赖 barrier **同步**把新装入的 ref 染灰。如果按朴素 B 让 barrier 在 Marking 期先 park 再染色，则：worker 已 `owner.slots[0]=leaf`（白）但尚未染色 → 进行中的 cycle sweep 把可达的 leaf 当垃圾 tombstone → **可达对象被回收**（比 stale-mark 更严重）。⇒ B 必须配合 **marking 期 allocate-black**，不能是单纯"park 再染"。

### 精化根因
真正的洞：**mutator 从 `new_with_core` 注册到它第一次 `check_safepoint` 之间，能跑 alloc+barrier，而此时 collector 可能已 STW sweep。** handshake 的 `need=vm_contexts.len()-1` 在 worker 惰性注册的时序下会漏掉"刚注册、尚未抵达首个 safepoint"的 worker。windows 线程调度更易暴露这个窗口。

### 正确修复方向（需 loom/shuttle 验证）
1. **marking 期 allocate-black**：phase ∈ {ConcurrentMarking, Marking} 时 `alloc_object`/`alloc_array` 出生即 marked=1（标准并发 GC 技术；新生对象本 cycle 保守存活，下 cycle 回收）。消除"可达新对象被 sweep"。
2. **注册—首safepoint 窗口封闭**：`new_with_core` 注册时若 phase≠Idle，新 context 必须先 park（或注册 happens-before 任何 heap op，且被 handshake 计入）。消除"未 park 的 mutator 在 STW 跑 barrier"。
3. **验证手段**：因硬件无法复现，应引入 **loom/shuttle** 对 alloc/barrier/handshake 交错做确定性模型检查——这是无法硬件复现的并发 bug 的正道。属独立、聚焦工作。

### 给 User 的现实结论
- 我能写出方向正确的修复，但**本地无法验证**，且 naive B 已被证伪——需 allocate-black + 注册封闭 + loom 验证，是一块独立的聚焦工作，非快速补丁。
- CI 现状：4 平台已 3 绿，仅 windows 因本 race 间歇红。
- **建议**：先用**已有的过渡手段**（windows `#[cfg_attr(target_os="windows", ignore)]` + 本 spec 跟踪）恢复 CI 绿，把"allocate-black + 注册封闭 + loom 验证"作为本 spec 的阶段 3 正式排期。该过渡与 philosophy"禁 #[ignore]"冲突，需 User 显式豁免；鉴于无法本地验证、且盲推 correctness-critical 改动风险高，这是当前最稳的路径。

## 尝试记录：注册封闭 fix 被证实有副作用（2026-06-01）

按 User 选定的方向实现了"注册封闭"：在 `safepoint.rs` 加 `park_if_collecting`，并在 `vm_context.rs::new_with_core` 注册后调用——若注册时 phase ∈ {Requested, Marking} 则 park。

**结果：deadlock 了既有单测 `safepoint_tests::second_collector_falls_back_to_mutator_park_returns_none`。** 该测试手动设 phase=Marking + collector_active=true，worker 线程 `new_with_core` 后调 `request_gc_pause` 期望"输掉 collector 竞争返回 None"。加了注册封闭后，worker 改在**注册处** park（而非在 request_gc_pause 处）；主线程 release（collector_active=false, phase=Idle）后 worker 才醒来去 request_gc_pause，此时 collector_active 已是 false → worker **赢得** collector 角色 → 等其它 context park（主线程在 join，不参与）→ 永久 deadlock。

**结论**：注册封闭改变了"谁成为 collector"的时序——这是对 handshake 协议的非局部影响，远不止"关一个窗口"。在**无法本地复现真 bug、无法本地验证**的前提下，盲推这种 correctness-critical 改动已经实测打破既有并发不变量。已 revert 代码改动（保留本分析）。

**强烈建议**：不要再盲推。正道是引入 **loom/shuttle** 对 alloc/barrier/handshake/注册 的全交错做确定性模型检查（能同时复现 windows race 与本次 deadlock），在模型下设计并验证 fix。这是独立的聚焦工作。短期：windows `#[ignore]` 过渡解封 CI（需 User 豁免 philosophy 的禁 skip）。

## 更新 2026-07-08：平台不对称前提失效 + 阶段 3 开工

**新事实**：`redesign-xtask-test` 今日把 `xtask test runtime`（cargo test）从 Windows-only
放开到全腿后，`macos-arm64` CI runner 首次真跑此测试，**复现了同一 stale-mark race**
（`arc_heap.rs` "stale mark bit in region_object after sweep"，run 28869578498）。

这**推翻**了上文"平台不对称：windows(强序 x86) 复现 / macOS-15·ubuntu-arm(弱序 ARM) 通过"
的核心判据：**弱序 ARM 也会复现**。含义：
- "强序 x86 现、弱序 ARM 不现 ⇒ 逻辑时序竞争"的推论仍指向**逻辑时序竞争**（注册→首safepoint
  窗口），但"仅 windows 调度易进窗口"的说法不再成立——ARM CI 调度同样能进窗口。
- 本地 Apple Silicon 仍无法复现（上文 8×2000×4000 放大在本机过）——是 **CI macOS runner 的
  调度**能进窗口而本机不能。验证难度不变：fix 仍只能靠 CI 概率性 + loom 模型验证。

**过渡**：ignore 从 `target_os="windows"` 扩到 `any(windows, macos)`（User 2026-07-08 豁免）。
linux(x64/arm64) 本轮仍过，暂不 ignore、留观察。

**阶段 3 计划（loom-validated fix，开工中）**：
1. 引入 `loom` 为 dev-dependency，`#[cfg(loom)]` 下用 loom atomics/thread 重建
   register→safepoint→alloc→barrier→handshake 的最小交错模型（复现 race 与 2026-06-01 deadlock）。
2. loom 模型下设计并验证：**marking 期 allocate-black**（出生 marked=1，消除"可达新对象被 sweep"）
   + **注册—首safepoint 窗口封闭**（避开上次改"谁成为 collector"时序的 deadlock）。
3. 模型绿后落地实现，CI 全腿去 ignore 回归。

## 更新 2026-09-04：collector 仲裁建模落地（模型 B），2026-06-01 的 deadlock 变成确定性本地测试

阶段 3.1 的「未做」项已补齐：`gc_registration_race_loom.rs` 现在有**两个**模型，
共用一套按 `gc/safepoint.rs` 建模的协议原语（`request_gc_pause` 的 `collector_active`
CAS + handshake、`release_pause`、`park_until_idle`）：

| 模型 | 场景 | 搜索 | 绿了代表什么 |
|---|---|---|---|
| A — stale mark | 单 collector + 一个迟注册 mutator | preemption-bounded(3) | fix 关上了 注册→sweep 窗口 |
| B — arbitration | 活跃 collector 在 worker 已 park 时释放 | **穷举**（34 条交错） | fix **没有**重新引入 2026-06-01 deadlock |

模型 B 逐字复刻 `safepoint_tests::second_collector_falls_back_to_mutator_park_returns_none`：
test-main 冒充活跃 collector（`collector_active=true` + phase `Marking`），等 worker park，
按单测的顺序释放（先清 claim 再开世界+notify），然后 `join()`。
**关键建模点：join 之后 test-main 永不再 park，但它的 `VmContext` 仍在 `vm_contexts` 里、
仍计入后来者的 `need`。** 这个不对称就是 deadlock 的全部。

实测（本机，0.14 s 跑完全部 4 个测试）：

- `arbitration_baseline_has_no_deadlock`（对照组）**绿**：baseline 下 worker 是在 CAS
  **已经输掉之后**才 park，永远抢不到 collector 角色 → 返回 None → join 干净。
- `registration_close_reintroduces_2026_06_01_deadlock` **确定性复现**
  （loom `deadlock; threads = [(Id(0), Blocked), (Id(1), Blocked)]`）：注册封闭把 park 移到了
  **仲裁 CAS 之前**，worker 醒来时 claim 已被释放 → 赢得 collector 角色 → 等 `need = 1`
  个永远不会来的 parker，而 test-main 卡在 `join()`。

⇒ **修复的硬约束（模型已固化为门禁）**：注册窗口封闭**不得把任何 context 的 park 移到
collector 仲裁 CAS 之前**。候选 fix 必须让 `registration_close_eliminates_race` 转绿的**同时**
保持 `arbitration_*` 不死锁。对照组的存在是这个门有判别力的前提——一个两边都死锁的模型
证明不了任何东西。

### loom 0.7.2 陷阱（踩过，别重走）

检测到 deadlock 后展开时若 drop 一个 `loom::sync::Arc`，其 Drop 会调
`rt::arc::Arc::branch` 对已拆除的 execution `unwrap()` → **析构中二次 panic → 进程 abort**
（`thread caused non-unwinding panic. aborting.`，且残留 `UE` 状态的僵尸进程）。
表现是「跑几百秒不结束」，极易误判成「状态空间爆炸、模型太慢」——实际 deadlock 是
**毫秒级**就命中的。解法：模型状态改用 `Box::leak` 的 `&'static Gc`（无析构），
panic 就能正常展开、`#[should_panic]` 接得住。每条交错泄漏一个小结构体，可忽略。

### 仍未建模（下一增量）

**marking 期 allocate-black**。`alloc_object` 出生 marked=0（白），当前正确性依赖 barrier
**同步**染灰；只关注册窗口而不配 allocate-black 的 fix 仍不健全（可达新对象会被进行中的
cycle 当垃圾 sweep 掉——比 stale mark 更严重）。

## 更新 2026-09-04（二）：新对象 sweep hazard 建模（模型 C）+ 一个比原记录更强的事实

阶段 3.1c。新文件 `src/runtime/tests/gc_alloc_black_loom.rs`（自成一体，因为它需要
模型 A/B 都没建的**并发 cycle 全流程**：snapshot → yield → handshake → sweep）。

### 更正：这个 hazard 不依赖 naive B，它无条件成立

上文「关键新发现 2」把 allocate-black 说成是**朴素候选 B 的前提**（barrier 先 park
再染色才会出事）。读码后确认范围更大 —— **今天的 ConcurrentMarkSweep 就有这个洞**：

- `finish_alloc` / `alloc_array_obj`（`gc/arc_heap/alloc.rs`）发布 region entry 时
  **完全不碰 mark bit**，新对象一律出生白色；
- write barrier（`generational.rs:288`）只染**被写入堆字段的那个 ref**，覆盖不到
  「只被 frame reg 持有」的新对象；
- `snapshot_roots_into_mark_queue`（`roots.rs:21`）**确实**会走每个 VmContext 的
  frame regs（`vm_context/construct.rs:234` 装的 external root scanner），但并发路径
  **只在 Phase 1 调它一次**，Phase 6 sweep 之前**再也不重扫 roots**
  （`control.rs:183-208`）。

⇒ 在 Phase 2–4 的并发窗口里分配、且只被 frame reg 持有的对象，没有任何机制会染它，
Phase 6 直接把它 tombstone —— **可达对象被回收，mutator 手里还攥着句柄**。
比 stale mark 严重。STW 路径不受影响，因为 `mark_phase`（`collect.rs:22`）在 collect
时**重扫 roots**；而 `StwMarkSweep` 正是生产默认，这解释了为什么它没在生产里炸。

### 模型钉死了策略边界

`AllocBlack` 三档，**穷举 2105 条交错**：

| 策略 | 结果 |
|---|---|
| `Never`（今日生产） | 可达新对象被 sweep |
| `ConcurrentOnly`（phase == ConcurrentMarking） | **仍然**被 sweep |
| `ConcurrentAndMarking` | 绿 |

中间那档是建这个模型的**全部理由**：「并发 mark 期出生即黑」是最自然的读法，而它是**错的**
—— `request_handshake_pause` 先把 phase 翻成 `Marking` **然后才等** mutator park，
mutator 在抵达下一个 safepoint 之前仍能分配，此时读到的是 `Marking`。
⇒ design 上文写的 `phase ∈ {ConcurrentMarking, Marking}` 从**断言**变成**已证必要**。
`allocate_black_on_concurrent_marking_alone_is_insufficient` 就是防止它被收窄回去的门。

**归因是差分式的、不靠插桩**：`ConcurrentAndMarking` 与 `ConcurrentOnly` 的唯一差别就是
「`Marking` 期算不算黑」，前者绿后者红 ⇒ 故障必然来自 `Marking` 窗口。

### 绿不是空过（已量化）

插桩统计 `ConcurrentAndMarking` 那一档：2105 条交错里，分配落在
**Idle 724 / Requested 481 / ConcurrentMarking 828 / Marking 72** —— 四个 phase 全都走到了，
包括那 72 次危险窗口。模型确实进入了危险状态，只是被 allocate-black 挡住。

顺带：在 `Idle` / `Requested` 分配**不需要** allocate-black 也安全，因为 Phase 1 快照
还没跑、会把它当 frame-reg root 染掉。模型也覆盖了这条（否则 `ConcurrentAndMarking`
会是「碰巧绿」）。
