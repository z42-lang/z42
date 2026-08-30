# Design: GC 线程本地分配缓冲（TLAB / chunk 独占）

## 背景：为什么并行现在负收益

`ArcMagrGC` 是单一共享堆，所有 mutator 线程共用一个 `Arc<VmCore>` → 一个堆。分配热路径的锁（实测 file:line）：

```
finish_alloc(obj)                        // src/gc/arc_heap/alloc.rs:157
├─ region_object.lock()  ← 全局锁 ①      // :163  bump 指针 + free_list + young_list + initialized
├─ would_oom_after_alloc → inner.lock()  // :65
├─ record_alloc:
│   ├─ inner.lock()  ← used_bytes/allocations // :27
│   ├─ check_pressure → inner.lock()     // :128
│   └─ inner.lock() (sampler clone)      // :34
└─ maybe_auto_collect → inner.lock()     // :88  (+ external_needs_collect.lock())
```

每对象 **1 region 锁 + 4–5 inner 锁**。N 线程并行 → 全在这几把锁上排队。VarRegion 路径 region 锁只 1 把
（`var_region.lock()`），但 inner 4–5 把一模一样。

**关键有利地基**：
- `RegionEntry.value` 是**每对象独立** Mutex，`marked`/`alive`/`generation` 已原子——只有**分配器元数据**
  （bump/free/young/all_blocks）+ `HeapStats` 在粗锁下。
- VmContext 已有 3 块 per-thread arena（`stack_arena`/`struct_arena`/`transient_arena`，`vm_context/types.rs:286`），
  owner 线程无争用、safepoint 已扫描——TLAB 照抄这个范式。
- `region.rs:46` 注释预留「256 是 future per-thread arenas 的合理批量」——设计时就埋了 chunk 独占的钩子。

## Architecture（chunk 独占 / 单堆单 GC）

**仍是同一个共享 `ArcMagrGC` 堆、同一套 Region（`region_object/array/var` 三个 Mutex 字段不变、GC 遍历/分代/card
table 全不动）。** 每个 mutator 线程从共享 region **借用一整块 chunk 的写权**，在里面本地 bump 填对象（零锁）；
chunk 填满就 retire（把该 chunk 已填部分的元数据批量并回共享 region）+ 再借一块。

```
                 ArcMagrGC (shared, 单堆单 GC)
   ┌───────────────────────────────────────────────────────────┐
   │ region_object: Mutex<Region<ScriptObject>>  ← 结构不变      │
   │ region_array : Mutex<Region<ArrayObj>>      │ chunks 归共享  │
   │ region_var   : Mutex<VarRegion>             │ region 所有    │
   │ free_chunk_pool（新）：整块全死 chunk 回收池 │              │
   │ stats: used_bytes/allocations = AtomicU64（retire 粒度flush）│
   │ sweep/mark/分代/card table：★零改动★（始终单 region）        │
   └───────────────────────────────────────────────────────────┘
      ▲ borrow chunk (锁一次)   ▲ retire (锁一次, 批量并元数据+stats)
      │                         │
  ┌───┴──────────┐         ┌────┴─────────┐
  │ VmContext A  │         │ VmContext B  │   借来的 chunk 内本地 bump = 零锁
  │ tlab.obj ────┼─▶chunk#3│ tlab.obj ────┼─▶chunk#7
  │ tlab.arr     │ [0..hw) │ tlab.arr     │   young_list/initialized 不在此碰
  │ tlab.var     │ 私有写   │ tlab.var     │
  └──────────────┘         └──────────────┘
```

- TLAB = per-VmContext 持有的「借来的活跃 chunk 句柄」（obj/arr/var 各一）：`{chunk 裸指针, 本地 next, cap,
  high-water 原子}`。
- **分配**（零锁）：在活跃 chunk 的 `[next]` 槽写 `RegionEntry`、`next += 1`；`GcRef` 从槽指针直接建。
- **retire**（借新 / safepoint 时，锁一次）：把该 chunk 的 `[0, hw)` 段一次性 `initialized`+push `young_list`、
  batch flush stats；chunk 归还共享 region（内存本就归它）。
- GC 侧：单 region、card table、`iterate_alive/young/dirty_cards` 全部**不变**——因为 chunk 内存和元数据始终归属
  共享 region，retire 后就是普通已分配槽。

## Decisions

### D1: 隔离单位 —— chunk 独占（User 定）

**决定：线程从共享 region 借用整块 chunk 的写权，本地 bump；不给线程私有 region。**
- **优**：共享 region 结构不变 → sweep/mark/snapshot/分代/card table **零改动**（~90 处 region 遍历不动）；chunk 内存
  归共享 region → 线程退出无生命周期问题（归还 chunk 即可）；card table 仍单 region 单索引，写屏障 `maybe_mark_
  cross_gen_card` 不变。
- **代价**：① 分配路径新增 borrow/retire 机制（局部、可单测）；② slot 级 free_list 复用被整块借用绕过 → 见 D7 的
  chunk 级回收 + 碎片延后。

（备选「私有 Region」——每 VmContext 一个私有 Region——per-object 更干净，但要把 GC 全线遍历改成 N region + 给热
`RegionEntry` 加 region 身份供 card 定位；GC 侧侵入大一个量级，故不选。）

### D2: chunk borrow / retire API + 元数据批量并回

**问题**：`Region::alloc` 每对象 push `young_list`、写 `initialized`；`VarRegion::alloc` 每对象 push `all_blocks`、
`live_count+=1`。这些是共享 region 结构，本地 bump 不能碰。

**决定：整块借用 + retire 时批量并元数据。**
- **`Region<T>::borrow_chunk() -> ChunkClaim`**（锁下）：从 `free_chunk_pool` 取一块全空 chunk，无则 grow 一块新
  chunk（推 `chunks`/`initialized`/`card_dirty`）。返回 `{chunk_idx, slots 裸指针, cap}`。**此刻不设 initialized/
  young**。
- 线程本地：`slots[next] = RegionEntry::new(value, (ci, next))`；`next += 1`；`GcRef::from_region_entry(&slots[i])`。
  `high_water = next`（原子发布，供并发模式 sweep 兜底）。
- **`Region<T>::retire_chunk(claim, hw)`**（锁下）：`initialized[ci][0..hw]=true`；`young_list` 批量 push
  `(ci, 0..hw)`；stats flush（D3）。chunk 归还共享 region（作为普通 chunk 参与后续 sweep）。
- **sweep 安全**：retire 后 chunk 是普通已分配 chunk，`iterate_alive/young` 正常。未 retire 的活跃 chunk 仅存在于
  「线程正在分配」瞬间——STW 下（D5）所有 TLAB 已 retire，sweep 不会遇到；并发模式靠 `high_water` 只扫 `[0, hw)`。

### D3: 统计（吸收 option B）—— retire 粒度 batch + 全局原子

`HeapStats.used_bytes` / `allocations` → `AtomicU64`。
- per-object 分配**不碰 stats**；retire 时按该 chunk 实际 `[0, hw)` 的对象数/字节 `fetch_add` 进全局原子（每 256 对象
  一次）。
- `check_pressure` / `maybe_auto_collect` / `would_oom` 读全局原子（retire 粒度、略滞后，对压力阈值保守偏早触发，
  可接受）。`near_limit_warned`/`last_auto_collect_used` 仍在 inner 锁下（慢路径）。
- **阶段 1 先落**：即使 TLAB 未上，先把 `record_alloc` 的 4–5 把 inner 锁换成原子 `fetch_add`（per-object 单原子）——
  独立可验、byte-identical。TLAB（阶段 2/3）再把这单原子 batch 到 retire 粒度。

### D4: VarRegion chunk 独占（string 重头）

VarRegion 已是纯 bump-字节分配器：
- **`VarRegion::borrow_chunk() -> VarChunkClaim`**（锁下）：取/grow 一个 64KB chunk，返回 `{chunk base, cap}`。
- 线程本地：`bump_off` 前移，写 `GcBlockHeader`+payload，本地记新块头指针。
- **`VarRegion::retire_chunk(claim)`**（锁下）：批量 append 本地块头到 `all_blocks`、`live_count += n`、stats flush。
- **oversized 块**（> chunk）/ free-list 复用：走旧 `alloc`/`alloc_dedicated` 锁路径（低频，不进 TLAB）。

### D5: safepoint 集成 —— retire-on-park

STW 模型已在 `request_gc_pause`（`safepoint.rs:335`）停所有 mutator。mutator park 前（`check_safepoint_slow`）retire
其 3 块 TLAB（批量并元数据 + flush stats + 清活跃句柄，下次分配重新 borrow）。collector 拿到完整一致的共享 region 后
mark/sweep——**与今天单 region 的 sweep 完全一样**。**ConcurrentMarkSweep 模式**：TLAB 写沿用现有 write-barrier
纪律（`interface.rs:217`）；`high_water` 让 mark 读活跃 chunk 已填部分。本 change 不改 barrier 机制。

### D6: strict_oom 退化

strict_oom 需「越界时精确撤销该对象分配 + 返 Null」，retire 粒度记账做不到 per-object 精确 refund。**决定**：
`strict_oom==true`（原子 bool）时分配走**旧逐对象锁路径**（不 borrow TLAB / 已有 TLAB 先 retire）。strict 是诊断
用途、非性能热路径，牺牲其并行度换精确性，可接受。

### D7: 空闲复用 —— chunk 级回收 + slot 级复用延后（Deferred）

整块借用绕过了共享 region 的 slot 级 `free_list`（tombstone 单槽复用）：
- **chunk 级回收（本 change 做）**：sweep 后，若某 chunk 的所有槽都 dead（或整块从未被 partial 占用后全空），整块回收进
  `free_chunk_pool`，供 `borrow_chunk` 优先复用 → 回收短命对象密集 workload（编译器正是）的大头内存。
- **slot 级复用（Deferred）**：partial-live chunk 里的零散死槽暂不被 TLAB 复用（仍可被旧锁路径 / ambient 分配经
  `free_list` 复用）。非移动 GC 本就有碎片；pre-1.0 可接受。登记到 `docs/book` GC 页 Deferred 段 + roadmap Deferred
  Backlog Index。触发条件：出现「live set 稳定但堆随 GC 轮次单调涨」的碎片回归 → 回来做 per-thread free-slot cache。

### D8: 单 PR 分阶段（User 定）

整个 change 分 5 阶段实施、**最后一个 PR 落地**（同 PR #333）。每阶段本地独立 GREEN + byte-identical 自举回归：
1. **stats 原子化**（D3 阶段 1 部分）：`HeapStats` 原子 + `record_alloc` 去 inner 锁。独立、byte-identical。
2. **Region\<T\> TLAB**（D1/D2/D5/D6/D7）：borrow/retire/chunk 回收 + VmContext TLAB + 分配路由 + safepoint retire +
   strict_oom 退化。**对象/数组并行零锁在此生效。**
3. **VarRegion TLAB**（D4）：string/closure 半边。
4. **翻并行默认 + jobs-scaling 实测**（转正验收）。
5. **文档**。

## Implementation Notes

- **地址稳定性**：`chunks: Vec<Box<[MaybeUninit<RegionEntry<T>>;256]>>`，Vec 增长时 Box 内容不移动 → 已分配槽指针恒稳。
  borrow 时拿 Box 内数组裸指针，本地 bump 写 + `GcRef::from_region_entry` 直接从槽指针建，无需 resolve 再锁。
- **TLAB 数据结构**（`gc/tlab.rs`）：`struct Tlab { obj: Option<ChunkClaim>, arr: Option<ChunkClaim>, var:
  Option<VarChunkClaim> }`；`ChunkClaim { chunk_idx: u32, slots: *mut MaybeUninit<RegionEntry<T>>, next: u16, cap:
  u16, high_water: AtomicU16 }`。挂 `VmContext`（owner 线程唯一写；参照 stack_arena 的持有范式）。TLAB 的 `retire`
  需要回调进 `heap` 的对应 region → TLAB 持 `Weak<VmCore>` 或经 `ctx.heap()` 调 `retire_chunk`。
- **borrow 的 `Send`**：`ChunkClaim` 持裸指针，需 `unsafe impl Send`（owner 线程独占访问；chunk 内存 `Send` 同
  `Region` 现有约定）。
- **retire 时机**：① 活跃 chunk 满（`next == cap`）→ retire + borrow；② safepoint park 前；③ VmContext drop 前。
- **不变式**：① 一个 chunk 至多一个线程独占写（borrow 记归属 / grow 出的新 chunk 只给借它的线程）；② STW 下所有 TLAB
  已 retire，GC 见完整共享 region；③ retire 后 chunk 元数据（initialized/young）与逐对象路径**等价**。三者由 borrow/
  retire 契约 + safepoint 协议保证，配 `// SAFETY` 注释 + 单测夯实。

## Testing Strategy

- **单元测试**（`gc/tlab_tests.rs` + region/var 测试补充）：
  - 单线程连续分配跨 chunk 边界，验证 borrow/fill/retire 切换、句柄有效、值正确。
  - retire 后 region 元数据（initialized/young_list/all_blocks/used_bytes）与逐对象 `alloc` 路径**逐字段等价**。
  - 多线程并发分配（spawn N 线程各分配 M 对象），全部句柄有效、无 UAF；STW sweep 后存活集正确。
  - 跨线程引用 + 线程退出：线程 A 分配对象传给 B 后退出（retire+归还 chunk），mark 沿 B 保活 A 的对象。
  - chunk 级回收：全死 chunk 回收进 pool，后续 borrow 复用同一 chunk；分配-GC-再分配循环下堆不单调涨（chunk 粒度）。
  - strict_oom 退化：越界走逐对象锁路径、返 Null、used_bytes 精确。
- **byte-identical 自举**：TLAB 是纯运行期分配器内部改动，不改 zbc/zpkg 产物——gen1(nightly 编源) == gen2(new z42c
  编源) 逐字节一致（忽略 16B BLID）。GC 正确性的强回归。
- **jobs-scaling 实测**（翻默认前的验收门）：workspace build 墙钟扫 jobs∈{1,2,4,8,24}，确认 CpuCount 明显快于 1
  （从当前「越多越慢」转正）。达标才翻 `ParallelConfig` 默认。
- **完整 GREEN**：`cargo test --lib`（GC 单测）+ `xtask test`（e2e/cross-zpkg/stdlib/compiler/vscode-syntax）。cold
  自举路径交 CI。
