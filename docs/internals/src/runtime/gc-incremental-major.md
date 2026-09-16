# 增量 major：SATB 屏障与有界停顿

> **相关**: [GC 调参与 safepoint](gc-tuning.md) · [GC TLAB](gc-tlab.md) ｜ **对齐**: 2026-09-16
> change `add-incremental-major-gc`（M2a：SATB 屏障；M2b 切片调度、M2c 验收完成后本页继续补全）

## 为什么

分代 GC 的**最大停顿全部来自 major**，而 major 的每个阶段都随活堆线性增长：`13_gc_large_heap` 上一次 major
约 60~70 ms（full mark 40 + sweep 20），活堆 ×3 时最大停顿 392 ms。目标是**最大停顿 ≤ 10 ms 且与堆大小无关**，
路线是把 major 拆成有界的 STW 切片，切片之间 mutator 照常运行。这就要求标记在 mutator 改图的同时仍然正确 ——
本页先讲保证正确性的屏障（M2a），切片调度（M2b）落地后补在后面。

## 标记位：minor 位 + major epoch

见 [gc-tuning-and-safepoint.md「minor 位与 major epoch 同字节分治」](gc-tuning.md)。要点：major 标记 =
「槽里的 epoch 等于本周期 epoch」，开周期即全堆变白；minor 只动 bit 0，切片之间跑 minor 不会擦掉 major 的标记。

## SATB 删除屏障

### 要防的那个洞

```
快照时：root → H（灰，未扫描）→ X（白）
mutator：r = H.f        // X 进了寄存器 —— 寄存器已在快照时扫过
         H.f = null     // 唯一的堆边被剪
marker ：扫描 H，字段已是 null，永远见不到 X
sweep  ：X 被回收，而 r 还攥着它
```

插入式屏障（Dijkstra，染被写入的新值）堵不住它，除非收尾时重扫全部根 —— 现有 `Z42_GC_MODE=concurrent`
正是这样漏的。SATB 走另一头：**覆盖一个堆引用槽之前，把旧值记下来**，标记收尾前把记下的值全部染灰。
配合 allocate-black（周期内出生的对象直接是黑的），就是 Yuasa 论证：快照时可达的每个对象，要么沿原图被走到，
要么它路径上第一条被剪的边的旧值被记录。**根不需要屏障** —— 快照时已整体染灰。

### 屏障放在哪

放在**写原语里**，不在 ~40 个调用点：

| 原语 | 覆盖的写入 |
|---|---|
| `ScriptObject::set_field_value` | 侧表引用叶子；**byte 内联的对象/数组字段**（先 `read_inline_ref` 取旧值） |
| `ScriptObject::set_ref_slot` | 直接写侧表引用叶子（`StructFieldSetPrim`、反射 `SetValue`） |
| `ArrayObj::set_boxed` | 引用数组元素；struct[] 元素的引用叶子 |
| `ArrayObj::write_struct_elem` / `set_struct_ref` | struct[] 元素整体 / 单个引用叶子 |
| `ArrayObj::copy_elems_from` | `Array.Copy` 的批量快路径（Boxed→Boxed 一次 `clone_from_slice`，先整段 `record_overwrite_all`）—— 第一轮审计漏掉，增量 major 的 `Z42_GC_SLICE_MS=0.05` 压测以编译器 SIGSEGV 抓到 |

`refs_mut_raw()` 不带屏障，只给**刚分配的对象**（旧值全是 `Null`）和 **GC 自己断边**（记录死对象会把悬空句柄
塞进标记队列）。规则写在 `../../../agent/rules/runtime-rust.md`。

### 原语手里没有堆 —— 线程本地记录

一个进程里可以有多个 VM 堆。进程级队列会让 A 堆的标记器用自己的 epoch 去染 B 堆的对象，所以：

```
VmContext::new*  ── satb::bind_thread(heap_id)        // 本线程的记录属于这个堆（栈式，嵌套 context LIFO 还原）
写原语           ── record_overwrite(old)
                     if MARKING_HEAPS == 0 { return }  // 标记期外：一次 relaxed load
                     slow: 本线程缓存「我的堆在标记吗、epoch 是几」（MARKING_GEN 变了才刷新）
                           旧值已被本周期标记 → 不记；否则 push 进线程本地 buf
retire_thread_tlab ── buf 交给本堆的 satb_queue          // 每条 park 路径都先 retire，collector 等到全员 park 才继续
close_major_marking ── loop { retire 自己；取 satb_queue；标记入 mark_queue；drain } 直到一轮取空
                       satb::end_marking(heap_id)
```

`open_major_cycle`（开 epoch + `begin_marking`）与 `close_major_marking` 接在 STW 周期和并发周期（Phase 1 / Phase 5）上。
在一次性 STW 周期里屏障实际不起作用（标记期没有 mutator 在写），但并发模式的 Phase 3 有 —— **M2a 起并发模式的
上述漏洞已被堵上**。

### 弱 / 软引用读取

一个快照时只剩弱引用的对象，可以经 `upgrade_weak` / 弱 `GcHandle` / `soft_ref_get` 被读回寄存器。SATB 的前提
「快照时不可达的对象不会再变可达」被它打破，所以这三条读路径在标记期把结果交给 `shade_if_marking`（进 `satb_queue`）。

### 标记期中的 minor

`satb_queue` 与 `mark_queue` 是 minor 的**额外根**。否则：屏障记录了一个年轻对象，随后的 minor 判它死、回收掉，
major 再从队列里拿到一个悬空句柄。

## 代价

标记期外每次堆引用写多一次 relaxed load + 不跳转的分支。实测（与只有 epoch 改动的二进制交错）：
`09_alloc_ctorless` 指令 +0.26%，`z42c.semantics` −0.03%（噪声内），编译产物逐字节一致。

## 测试

`src/runtime/src/gc/arc_heap_tests/incremental.rs`，手工驱动周期（开周期 → 快照根 → mutator 动作 → drain → 收尾 → sweep），
存活性一律经弱引用观察，不解引用可能已被回收的句柄：

- 字段读进寄存器再清字段：开屏障存活 / **关屏障被回收**（阴性对照，证明测试能判别）
- `set_field_value`（byte 内联引用）/ 数组元素覆盖同样被记录
- 标记期外不记录；另一个堆在标记时本线程的记录不会进它的队列
- 标记期弱读：读了存活 / 不读被回收
- minor 把 SATB 记录当根：有记录存活 / 无记录被回收
