# GC 句柄表（Std.GCHandle）

> 对齐：2026-09-17（change `restructure-docs-three-books`）｜ 代码：
> `src/libraries/z42.core/src/GC/GCHandle.z42`、`src/libraries/z42.core/src/GC/HeapStats.z42`、
> `src/runtime/src/corelib/gc.rs`、`src/runtime/src/gc/arc_heap.rs`（`HandleSlab` / `HandleEntry`）、
> `src/runtime/src/gc/arc_heap/interface.rs`（`handle_*` 实现）、`src/runtime/src/gc/heap.rs`（trait 声明）
>
> 收集器本体（标记 / 清扫 / 分代 / 增量 major）见 [GC 子系统](gc.md)；
> 「谁还持有这个对象」的反查见[堆保留诊断](heap-diagnostics.md)。

`Std.GCHandle` 是脚本能拿到的、**显式管理生命周期**的堆引用：强句柄把目标钉活到 `Free()`，
弱句柄只观察。本页讲这条链是怎么搭的——slot 表的数据结构、强句柄靠什么保活、读出口上那道
增量 major 的闸门，以及加一种句柄类型要动哪几处。**改 `handle_*` 任何一条路径前读本页**；
只想用 API 的话，签名在 `GCHandle.z42` 的注释里。

## 1. 一条 long 贯穿两侧

z42 侧 `GCHandle` 是**单字段 struct**（`private long _slot`），字段值就是 corelib 句柄表里的
slot id；`0` 是「未分配」sentinel。五个成员全是 `[Native("__gc_handle_*")]` extern，z42 侧
不存任何状态：

| z42 成员 | builtin | 落到 |
|---|---|---|
| `Alloc(target, GCHandleType)` | `__gc_handle_alloc` | `MagrGC::handle_alloc` |
| `Target { get; }` | `__gc_handle_target` | `handle_target` |
| `IsAllocated { get; }` | `__gc_handle_is_alloc` | `handle_is_alloc` |
| `Kind { get; }` | `__gc_handle_kind` | `handle_kind` |
| `Free()` | `__gc_handle_free` | `handle_free` |

`AllocStrong` / `AllocWeak` 是 z42 侧对 `Alloc` 的两行包装，没有独立 builtin。

**为什么是 struct + corelib backing。** 要的语义是 C# `GCHandle` 那种「拷贝共享同一个
backing、任一 alias `Free()` 后全体失效」。三条路里只有这条走得通：

- 纯 z42 class：引用类型天然共享，但 corelib 侧拿不住用户 class 实例的句柄（那是
  `Value::Object(GcRef<ScriptObject>)`，句柄表要存的是自己的 slot）；
- 纯 z42 struct、状态放在字段里：值拷贝把状态一起复制，`h1.Free()` 只清 `h1`，`h2` 还持有
  引用——`Free` 失去意义；
- **struct + 一条 slot id**：拷贝复制的是 id，所有 alias 指向 corelib 里同一个 slot，`Free`
  作用在 slot 上 ⇒ 共享语义自然成立。

`enum GCHandleType { Weak = 0, Strong = 1 }` 的两个值在 corelib 侧以
`GC_HANDLE_TYPE_WEAK` / `GC_HANDLE_TYPE_STRONG` 常量硬编码（`corelib/gc.rs`），**改枚举顺序
必须同时改这两个常量**。

## 2. HandleSlab：slab + free list

```rust
struct HandleSlab {
    entries:   Vec<Option<HandleEntry>>,
    free_list: Vec<u64>,
}

enum HandleEntry {
    StrongObject(GcRef<ScriptObject>),
    StrongArray (GcRef<ArrayObj>),
    StrongAtomic(Value),              // I64 / F64 / Str / Bool / Char / FuncRef / …
    WeakObject  (WeakGcRef<ScriptObject>),
    WeakArray   (WeakGcRef<ArrayObj>),
}
```

选 slab 而不是 `HashMap<u64, HandleEntry>` + 单调递增计数器：后者平均也是 O(1)，但 slot 不
复用、表只增不减；slab 是连续内存、cache 友好，`free_list` 让 `Free` 过的 slot 立刻回到待用
池（LIFO 复用，单测 `handle_free_then_realloc_reuses_slot` 锁住这条）。

`entries[0]` 永不读写——它在第一次 `alloc` 时被懒占位成 `None`，好让「slot id 0 = 未分配」
这个 sentinel 与真实索引不冲突。

分配时的取值分支（`arc_heap/interface.rs::handle_alloc`）只有四种结果：

| target | Strong | Weak |
|---|---|---|
| `Value::Object` / `Value::Array` | 存 `GcRef` clone | 存 `GcRef::downgrade` |
| 原子值（int / string / bool / …） | 存 `Value` clone | **返回 slot 0**（原子值无法弱化） |
| `Value::Null` | **返回 slot 0** | **返回 slot 0** |

拿到 slot 0 的句柄是合法对象，只是 `IsAllocated == false`、`Target == null`。这里不抛异常是
刻意的：`Alloc` 总成功、状态事后查，跟 C# 一致，也跟 `WeakHandle.MakeWeak(atomic) → null`
同一手感。

`Free` 对 slot 0、越界 slot、以及重复释放全是 no-op（幂等），所以 z42 侧不需要保护性判断。

## 3. 强句柄靠什么保活：它是 mark root

**这是本页最容易写错的一条。** slab 里存一个 `GcRef` 本身**不**保活：`GcRef<T>` 是
「地址 + generation」的 region 句柄，不是引用计数指针，clone 它不改变任何存活判定。强句柄
之所以是强的，是因为 `HandleSlab::strong_targets()`（只吐 Strong 三种 variant，Weak 两种
故意排除）被挂进了**每一条标记的根集**：

| 路径 | 文件 | 什么时候跑 |
|---|---|---|
| 全堆 STW 标记 | `arc_heap/collect.rs::mark_phase` | `force_collect` / 非分代整堆回收 |
| 分代 minor 标记 | `arc_heap/generational.rs::mark_phase_minor` | 默认收集器的绝大多数停顿 |
| 增量 major 开镜 | `arc_heap/roots.rs::snapshot_roots_into_mark_queue` | 增量 major 的根快照 |
| 保留诊断反查 | `arc_heap/roots.rs`（retention 图） | `Heap.RetainingRoots`，报成 `RootKind::Pinned` |

**三条标记路径必须一起改**：只补整堆那条，默认（分代）配置下强句柄照样不保活，而分代 minor
才是绝大多数回收。诊断那条也得跟着——否则反查会显示一个「活着但没有任何 retainer」的对象，
正好是这个诊断存在的意义所在。

弱句柄不进任何根集，这是它与强句柄**唯一**的行为差别（而不是「能不能 downgrade」）。
`src/runtime/src/gc/arc_heap_tests/roots.rs` 里三个测试分别锁住：强句柄跨整堆回收保活、弱句柄
不保活、强句柄跨 minor 保活。

## 4. 读出口上的增量 major 闸门

`handle_target` 读到值之后还要过 `admit_resurrected`（`arc_heap/incremental.rs`）：

- 增量 major 正在**标记**：把读出的值 shade 一次——它可能在快照时只被弱可达，而现在要进
  寄存器了；
- 增量 major 正在**清扫**：未被本轮标记的值一律拒绝（返回 `None` ⇒ 脚本看到 `null`），否则
  会把一个马上要被回收、子节点可能已经没了的句柄递给 mutator。

所有「不经强引用把已有堆值交给 mutator」的出口共用这道闸门：弱读、软读、堆遍历。对强句柄
它实质是恒真（强目标必然已被标记），但代码路径是同一条，别为强句柄绕过它。

## 5. HeapStats：7 字段投影，`MaxBytes = -1`

`Std.GC.GetStats()` 走 `__gc_stats`，corelib 直接 emit 一个 `Std.HeapStats` 实例（不暴露
ctor，保证字段就是当时的 GC 状态）。z42 侧是 7 个只读 auto-property；**编译器把
`public long X { get; }` 脱糖成私有字段 `__prop_X` + `get_X()`**，所以 corelib 里手工造的
TypeDesc 字段名带 `__prop_` 前缀，且**顺序即 slot 顺序**，与 `builtin_gc_stats` 压进去的
`Vec<Value>` 一一对位——改任一侧的顺序或增删字段，两处必须同时改。

`MaxBytes` 用 `-1` 表示无上限：Rust 侧是 `Option<u64>`，z42 没有 `Optional<T>`，三个候选
sentinel 里 `0` 会与「上限就是 0 字节」混淆、`u64::MAX` 转 i64 溢出，`-1` 是唯一无歧义的。

⚠️ Rust `HeapStats` 现在是 11 个字段（多出 `minor_collections` / `major_collections` /
`reclaimed_bytes` / `pause_histogram`），z42 侧仍只投影 7 个。分代计数与回收字节数目前
**脚本读不到**——要补就在 `heap_stats_type_desc()` 的名字表、`builtin_gc_stats` 的值表、
`HeapStats.z42` 三处同时加。

## 6. 与 WeakHandle / SoftHandle 的分工

三者走**三条互不依赖的 corelib 路径**，不要试图让一个包另一个（`GCHandle.AllocWeak` 内部
包 `WeakHandle` 会造出「Free 释放 wrapper 但不释放 backing weak」这种说不清的语义）：

| 需求 | 选 | 背后 |
|---|---|---|
| 简单弱引用，靠 GC 自动清 | `Std.WeakHandle` | z42 class，`NativeData::WeakRef` 直接持 `WeakRef` |
| 强引用 + 自己决定释放点（含跨 native 边界 anchor） | `GCHandle`（Strong） | slab slot，进 mark root |
| 弱引用 + 显式 `Free`（要 slot 复用 / 要查 `Kind`） | `GCHandle`（Weak） | slab slot，不进 root |
| 「内存紧张时可以被清掉」的缓存 | `Std.SoftHandle` | `gc/soft_registry.rs` + 压力阈值 |

`Delegates/SubscriptionRefs.z42` 这类内部只需要弱引用的地方用 `WeakHandle`。

## 7. 句柄类型只有两种

`GcHandleKind` 就 `Weak` / `Strong`。C# 的 `Pinned`、`WeakTrackResurrection`、
`AddrOfPinnedObject()` 以及 `Target` 的 setter 都没有对应物。pinning 在当前收集器上没有意义：
对象条目的地址在其生命周期内稳定（region 的 chunk 是 `Box` 持有、不搬迁），没有「防止被移动」
这回事；要改 anchor 模式就 `Free()` 后重 `Alloc`。

真要加一种 kind，需要动的点是固定的：`GcHandleKind` 加 variant → `HandleEntry` 加 variant →
`HandleEntry::kind()` / `target()` / `strong_targets()` 三个 match 补分支（**`strong_targets`
最容易漏，漏了就是「新 kind 声称保活但不保活」**）→ `handle_alloc` 的取值分支 → `corelib/gc.rs`
的两个常量与两个转换函数 → `GCHandleType` 枚举。

## 8. 测试在哪

| 层 | 位置 |
|---|---|
| slab 行为单测（alloc / free / 复用 / 幂等 / kind） | `src/runtime/src/gc/arc_heap_tests/weak_refs.rs` |
| 强/弱句柄作为 mark root（整堆 + minor） | `src/runtime/src/gc/arc_heap_tests/roots.rs` |
| 端到端：强 anchor / 弱失效 / alias 共享 slot | `src/tests/gc/gc_handle.z42` |
| 端到端：`GetStats()` 字段与 `-1` sentinel | `src/tests/gc/gc_stats.z42` |
