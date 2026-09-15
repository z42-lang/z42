# Design: 增量 major 标记（SATB）

## Architecture

```
            ┌──────────── major 周期 E（epoch ∈ 1..=127）────────────┐
 Idle ─trip→ Snapshot ─→ Mark ─→ … ─→ Mark ─→ Final ─→ Sweep ─→ … ─→ Sweep ─→ Idle
            (切片 0)    (切片 1..k)          (排空)   (切片 k+2..)
            灰化 roots  按预算排灰队列        SATB 缓冲  按 chunk 游标清扫
            开 SATB     切片间 mutator 跑、   收尾到空   epoch≠E 的 alive 条目 = 死
            开 alloc-black 允许 minor          关 SATB    结束关 alloc-black
```

- 每个切片都是一次**普通 STW 暂停**（`request_gc_pause`，所有 mutator 在 safepoint 停住），预算 `Z42_GC_SLICE_MS`
  （默认 2 ms）。**标记从不与 mutator 并发执行** —— 这是相对「并发标记」的核心降险：禁区里的三类竞态
  （注册窗口 / 仲裁 / 新对象）在切片模型里退化为「切片之间 mutator 做了什么」，由 SATB + allocate-black 两条不变量兜住。
- 切片之间 mutator 照常运行、照常触发 minor。minor 与 major 各用自己的标记位，互不擦除。
- M4 把 Mark / Sweep 切片挪到后台线程时，屏障与不变量不变，只是「切片之间」变成「任意时刻」。

## Decisions

### Decision 1: 标记位 = minor bit + major epoch（删除 reset marks）

**问题**：major 目前开头 `reset marks` 遍历全堆清标记（6~11 ms，∝ 全部条目）；而任何「漏清一个标记」都会让
下一次标记跳过它的子节点 —— 本仓栽过三次的 stale mark。

**方案**：`RegionEntry::marked` 与 `GcBlockHeader::marked`（整字节归标记所有，`gen_age` 在 `type_tag` 里）改为
`bit0 = minor 标记`、`bits1..=7 = major epoch`。major 周期开始时全局 epoch 前进（1..=127 循环，**0 保留为「从未标记」**），
「major 已标记」⇔ 字节高 7 位 == 当前 epoch。

**为什么安全**：每次 major sweep 访问全部 alive 条目 —— 标记过的保留 epoch E，没标记的被 tombstone。
⇒ 周期 E 结束后所有 alive 条目的 epoch ∈ {E, 0}。下一周期 E+1 开始时它们**自动全白**，不需要任何遍历。
回绕（127→1）时 alive 条目 epoch ∈ {127, 0}，≠ 1，同样全白。陈旧 major 标记在构造上不存在。

两个 CAS 各自保留对方的位（`compare_exchange` 循环，STW 下实际一次成功）。

### Decision 2: SATB 删除屏障下沉到写原语

**问题**：堆引用写入点散在 interp / JIT helper / corelib / 反射，约 40 处；一部分今天不调任何屏障（写新对象时不需要卡表）。
逐调用点加 pre-write 屏障，漏一处 = 漏标 = 活对象被回收。

**方案**：屏障放进**唯一的写原语**：`ScriptObject::set_field_value`、`ArrayObj::set_boxed`、`ArrayObj::write_struct_elem`。
原语不需要堆引用 —— 屏障只读一个进程级 `satb::MARKING_ACTIVE: AtomicBool`（relaxed）：

```rust
#[inline]
fn satb_on_overwrite(old: &Value) {
    if MARKING_ACTIVE.load(Relaxed) && old.is_heap_ref() && !old.is_major_marked() {
        satb::remember(old.clone());   // 线程本地缓冲；无 VmContext 的线程走全局加锁队列
    }
}
```

`refs_mut()` 改为 `pub(crate)` 并逐个审计剩余直写点（interp `exec_object.rs`、JIT `object_field.rs`、反射 `accessors.rs`，
加上 sweep 的断边 —— 断边写的是**死对象**，必须走不带屏障的内部写，否则会把死对象的旧值塞进灰队列）。

**为什么 roots 不需要屏障**：snapshot 切片把所有 roots（各线程帧寄存器、静态存储、强句柄、外部扫描器）一次性灰化。
一个 S 时刻可达的对象要么经 roots 出发的路径被标记，要么那条路径上某条边在被遍历前被覆盖 —— 覆盖时旧目标进 SATB。
S 之后新建的线程 / 帧只能持有 S 时刻可达的对象或新分配对象（后者见 D3）。这是标准 Yuasa 论证。

非标记期代价：每次堆引用写一次 relaxed load + 不taken 分支。M2 验收要求 `instructions retired` 回归 ≤ 1%。

### Decision 2b: 弱 / 软引用读取要染色（SATB 唯一的「复活」口子）

SATB 论证依赖「S 时刻不可达的对象再也不会变可达」。弱 / 软引用正好破坏它：`GcHandle` 弱句柄 `upgrade`、
`soft_ref_get` 可以把一个 S 时刻只剩弱引用的白对象读进寄存器。⇒ Snapshot 到 Sweep 结束期间，这两个读取入口在返回
非 Null 前把目标 `mark major`（并入灰队列）。G1 的 `Reference.get` keep-alive 屏障同理。finalizer 复活沿现有语义
（finalizer 在 sweep 之后执行，被复活对象下一周期才判定）。

### Decision 3: allocate-black 覆盖 Snapshot 至 Sweep 结束

复用 3.2a 的 `alloc_black` 窗口，但写入的是**当前 epoch** 而不是 1；覆盖三个 region、TLAB 与加锁两条路径（五个 chokepoint）。
Sweep 期间也保持：新分配落在还没被清扫游标扫到的 chunk 里时，必须被判活。窗口在最后一个 sweep 切片后关闭；
关闭后留下的 epoch E 由 D1 保证不会成为陈旧标记。

### Decision 4: 切片之间的 minor

- **灰队列与所有 SATB 缓冲是 minor 的额外根**。否则：S 时刻可达的年轻对象 Y 在切片间被断开、旧值进了 SATB，
  随后 minor 判 Y 死并 tombstone，major 再从队列里拿到一个悬空句柄。作为根，Y 活过这次 minor，major 下一个切片处理它。
- minor 只读写 bit0；晋升不改 epoch；minor 的断边写走内部无屏障写。
- minor 期间 `MARKING_ACTIVE` 保持为真（minor 本身不写堆引用，除断边外）。
- Snapshot / Final 切片本身不与 minor 叠加（同一次 `request_gc_pause` 里只做一件事）。

### Decision 5: Final 切片不重扫 roots

SATB 下 S 之后 roots 的变化不影响正确性（D2）。Final 切片 = flush 各线程 SATB 缓冲 → 排空灰队列 → 若排空过程中又无新增则结束；
预算超限就结束本切片、下一切片继续（mutator 继续跑、继续产生 SATB 条目，但只会有限增长：每个 S 时刻存在的边至多入队一次，
因为入队前检查 `is_major_marked`）。

### Decision 6: Sweep 分片

按 region、按 chunk 推进游标（对象区 → 数组区 → var 区），每片预算内处理若干 chunk。
- 清扫中的 chunk 不借给 TLAB（`borrowed` 互斥已有）；已清扫完的全死 chunk 立即进池可复用。
- `age survivors` 在同一趟里做（M1 先在 STW sweep 里合并，M2 自然随分片）。
- 死对象的 finalizer 仍在 sweep 切片内收集、切片外执行（沿现有语义）。

### Decision 7: 节奏与退化

- 启动：沿用现有 major 触发（晋升字节 ≥ allowance），但改为**启动周期**而不是立刻 STW 完成。
- 节奏：每次 minor 之后（或每分配 `nursery/4` 字节）调度一个 Mark / Sweep 切片；周期总进度按「剩余灰条目 / 已分配字节」估算，
  落后时切片连续执行（仍各自有界）。
- 退化：堆触及软上限、或 `GC.Collect()` / `ForceCollect()` / OOM 路径 ⇒ **同步完成当前周期**（正确性优先，放弃停顿目标，
  并在 `Z42_GC_TRACE` 打一行 `incremental: finish synchronously (reason)`）。

## Implementation Notes

- 周期状态、epoch、游标放 `gc/incremental.rs`；`ArcMagrGC` 持有一个 `IncrementalCycle`（`parking_lot::Mutex`，只在切片内访问）。
- SATB 缓冲：`VmContext` 持 `UnsafeCell<Vec<Value>>`（本线程写、collector 只在该线程 parked 时读），超过 4096 条时本线程主动加锁并入全局队列。
  线程退出（VmContext Drop）时 flush。
- `Z42_GC_PHASES` 为每个切片打一行：`slice <kind> <ms> (<entries>)`，并在周期结束打 `major cycle E: slices N, total X ms, max slice Y ms`。
- `StwMarkSweep` 与现 `ConcurrentMarkSweep` 模式不受影响（M2 只改分代模式的 major）；`Z42_GC_INCREMENTAL=0` 回到一次性 major，作为 A/B 与排障开关。

## Testing Strategy

**确定性单测**（`arc_heap_tests/incremental.rs`，单线程手工驱动切片）：
1. **漏标阴性对照**：Snapshot → 读字段 `A.f` 进寄存器根、`A.f = null` → 完成周期 → 断言对象存活。
   对照组：关掉 SATB（测试钩子）⇒ 必须被回收（证明测试有判别力）。
2. allocate-black：Mark 期与 Sweep 期各分配一个只被寄存器持有的对象，周期后存活。
3. minor 夹在切片之间：SATB 缓冲里的年轻对象活过 minor；major 完成后不可达即回收。
4. epoch 回绕：连续 130 个周期，每轮校验「无陈旧标记」不变量（`debug_validate_invariants` 扩展）。
5. 删 reset marks 的等价性：STW 路径下新旧实现回收同一批对象（逐个比对 handle 集合）。

**loom 模型 D**（`tests/gc_satb_loom.rs`）：两个 mutator（读边进寄存器 / 覆盖边 / 分配）× collector 切片 × 一次 minor，穷举；
同时既有三个模型（A/B/C）必须保持绿。

**端到端**：
- `xtask test` GREEN（含自举不动点，产物逐字节不变）。
- 正确性配方（pause-line memory）：冷 `package sdk`、4M nursery 冷 `package sdk`、`build stdlib` 各模式、
  **`Z42_GC_SLICE_MS=0.05`**（极小预算，最大化切片交错）跑全套 stdlib 测试。
- `z42.net http_server_threaded` × 60（多线程写堆的历史绊线）。

**性能验收**（两个二进制交错 ×5）：
| 指标 | M1 目标 | M2 目标 |
|---|---|---|
| semantics max 停顿 | ≤ 25 ms | **≤ 10 ms** |
| `13_gc_large_heap` max 停顿 | 记录基线 | **≤ 10 ms，且与堆大小无关**（两档堆大小比较） |
| 总停顿 | 不回归 | 允许 +20%（切片有固定开销），墙钟 ≤ +2% |
| instructions retired | 不回归 | ≤ +1% |
| 峰值 RSS | 不回归 | ≤ +5%（allocate-black 带来的浮动垃圾） |
