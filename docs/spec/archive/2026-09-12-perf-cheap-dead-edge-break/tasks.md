# Tasks: perf-cheap-dead-edge-break

> 状态：🟢 已完成 | 创建：2026-09-12 | 完成：2026-09-12

**变更说明：** minor 清扫给**死对象断边**这件事，从「每个死对象一次 `malloc/free` + 三次取锁」
变成「零分配 + 一次取锁」。两处：

1. `ScriptObject::clear_inline_refs` 不再把内联 ref 的 offset `collect()` 进一个 `Vec<u32>`。
   那行注释说这是为了「先释放 `type_desc` 的借用」——**没有需要释放的借用**：`type_desc` 与
   `storage` 是 `ScriptObject` 的两个不相交字段，直接按字段读就够了；而且
   `composed_object_layout()` 返回的本来就是 owned `Arc`，根本没借着 `self`。
   代价是**每个死对象一次 malloc + free**。新增 `TypeDesc::composed_object_layout_ref()`
   （借用版，不克隆 `Arc`）供它使用。
2. 断边挪到**紧邻的 scan 循环**里做 —— 那里为了算 `script_object_size_estimate` 已经握着
   region 锁和该 entry 的 value 锁。原先它在 tombstone 循环里，要重新取 `region_object`、
   重新 `resolve(h)`、重新锁 value，然后**第三次**取 `region_object` 才 tombstone。

**为什么是它：** `Z42_GC_PHASES=1` 上 `minor/tomb objects` = **67.7 ns 一条**，而干同一件事的
`minor/tomb arrays` 只要 **10.8 ns**——数组那条路没有这段断边。比值不对就是缺陷（模块文档原话）。

**顺序安全性：** 断边现在跑在 finalizer **之前**。`FinalizerFn` 不接受任何参数，没有任何途径
读到这个对象，所以没有可观察的差别。从 scan 到 tombstone 之间也没有东西碰死 entry ——
中间的晋升 / 字节记账 / 卡表脏化走的全是 `survivors_object`。

**文档影响：** 无（book 里没有任何一页描述这段顺序）。

- [x] 1.1 `composed_object_layout_ref()`（借用版访问器）
- [x] 1.2 `clear_inline_refs` 去掉 `Vec` 分配，改按字段不相交借用
- [x] 1.3 断边挪进 scan 循环，tombstone 循环退化成数组那条路的形状
- [x] 1.4 四个单测（断边生效 / 越界 offset 跳过不 panic / 两个访问器不许漂移）
- [x] 1.5 GREEN（`./xtask test` 全绿含自举不动点 3/3 + gc modes；`cargo test --lib` debug 1232 passed）

## 实测（base = `e130cdb9`，两个二进制各 3 跑，`z42c.semantics --release --no-incremental`）

| | 墙钟 | 峰值 RSS | 总停顿 | 中位 | p90 | 最大 |
|---|---|---|---|---|---|---|
| base | 8.94 s | 552 MB | 411.0 ms | 10.20 ms | 17.20 ms | 46.0 ms |
| **本 change** | 8.94 s | 552 MB | **383.1 ms（−6.8%）** | 10.10 ms | 17.30 ms | 44.4 ms |

（停顿分位数是三跑里**逐名次取中位**，不是挑一跑。）

### ⚠️ 说清楚这是哪一种收益

**总停顿 −6.8%，而分布几乎没动**（中位 10.20→10.10、p90 17.20→17.30、最大 46.0→44.4）。
省下的是**每次回收一笔随死对象数走的开销**（每次 minor 约 1.3 ms，33 次），整条分布平移，
平移量小于本机的跑间噪声。跟 #590 是同一种形状，**别当尾延迟改进宣传**。
`z42c.semantics` 的墙钟也没动（GC 只占它 4.5% 墙钟，28 ms 落在噪声里）——
墙钟收益要在 GC 占比高的负载上看，就是下面那张表。

阶段分解：`minor/tomb objects` **51.0 → 8.0 ms**（其中去 `Vec` 分配拿走 32.5 ms、挪锁再拿
10.5 ms），`minor/scan objects` 17.3 → 24.3 ms（断边搬进来了），`minor sweep` 247 → 215 ms。

第二个形状不同的负载（各 5 跑墙钟中位）：

| scenario | interp | jit |
|---|---|---|
| `12_gc_churn` | 0.74 → **0.70 s（−5.4%）** | 0.58 → **0.535 s（−7.8%）** |
| `09_alloc_ctorless` | 0.47 → 0.47 s | 0.455 → 0.45 s |

## ☠️ 同一轮量过并否决的三条（都在 `minor mark` 上，别再试）

`minor mark` ≈ 90 ms（22% 总停顿），其中 BFS 80 ms。拆出来的形状：一次构建 7 341 616 次
子槽 visit、4 654 066 条堆边、2 931 577 次 trace、2 893 723 个块被标记，**13.7 ns/visit**。
三刀全部失败：

1. **先标记再入队（去重）+ 叶子不入队**。字符串是纯叶子却占 27.5% 的 pop，重复 pop 另占
   26%——两条加起来砍掉 54% 的队列吞吐量。**实测 90.1 → 91.4 ms，反而略慢。**
   原因：`GcRef` 是 8 字节 POD 标签指针（`Clone` 就是 `*self`），push/pop 近乎免费，而
   `mark_if_unmarked` 的次数**一次没少**（去重只是把 CAS 从 pop 侧挪到 push 侧）。
2. **软件预取流水线**（环形缓冲把子块头部的首次触碰延后 8 步，中间发 `prfm`）。
   **90 → 110 ms，慢 22%。** Apple M 系的乱序窗口本来就把这些 load 叠起来了，加一层环形
   缓冲是净增指令。
3. **STW 下把 mark 位的 CAS 换成 load+store**（仓里 `set_in_young` 已有「STW 所以不需要 CAS」
   的先例）。**88.5 vs 90.1 ms，噪声内。** Relaxed CAS 在这台机器上不是瓶颈。

⇒ **串行 minor mark 已经到底了**，剩下的杠杆只有并行、或者少标记一些东西。

## 📌 顺带发现、本 PR 没动

`composed_object_layout()` 的另外两个只读调用方也在白克隆 `Arc`：
`ScriptObject::trace_inline_refs`（mark 阶段每个被 trace 的对象一次，一次构建 1 402 855 次）
和 `field_access_of`（解释器每次字段读写）。两处都换成 `_ref` 量过：`minor mark` 90.1 → 92.0 ms、
墙钟 8.94 → 8.88 s，**都在噪声内**。按「实测零收益不留」的规矩没进本 PR。
