# Proposal: 压缩每个堆对象的实际占用（shrink-object-footprint）

> 状态：📝 待 User 确认 | 创建：2026-09-04 | 类型：vm（走完整流程）
> 来源：三面评审二轮 P3「对象分配单块化」。**先量后改**——量完发现「两次额外 malloc」
> 只是三项里最小的那一项，所以范围比原条目大，标题也改了。

## Why

**实测**（同机，`z42vm --release`，2026-09-04，main = #423）：

一个留住 200 万个 `Node`（4 个基元字段 + 2 个引用字段）的程序，
peak RSS 516.75 MB；同一程序 `n=1` 时 12.29 MB。扣掉 200 万格的数组（约 32 MB），
**每个存活对象约 236 字节**。

`size_of` 探针给出这 236 字节的构成：

| 项 | 字节 | 说明 |
|---|---:|---|
| `RegionEntry<ScriptObject>` | **128** | GC 槽位；其中 payload 72、**头部 56** |
| ↳ `Mutex<ScriptObject>` | 80 | 8 字节锁 + 72 字节对象 |
| ↳ **`Mutex<Option<FinalizerFn>>`** | **24** | **每个对象都带一个终结器槽** |
| ↳ marked / alive / gen_age / generation / location / soft_count | 24 | 真正必须的 GC 元数据 |
| `bytes` 块（基元叶子，独立 malloc）| ~48 | 布局 37 → 对齐 40 → mimalloc 档 48 |
| `refs` 块（引用叶子，独立 malloc）| ~32 | 2 × `Value`(16) |
| 分配器 / region 余量 | ~28 | 观测值减去上面之和 |

三个可回收的头部：

1. **终结器槽 24 字节，而且全仓没有一个调用方**。`grep register_finalizer` 在
   `src/` 下只命中**定义**（`heap.rs` trait、`interface.rs` 实现、`refs.rs`）和
   `arc_heap_tests/finalization.rs`——**没有任何生产代码或 z42 侧 builtin 注册过终结器**。
   即：每个对象都在为一个当前无人使用的能力付 24 字节（占槽位的 **19%**）。
2. **`ScriptObject` 里两个胖指针 = 32 字节**，指向两块**分别 malloc** 的区域
   （`TypeDesc::object_regions()` 每次 `vec![0u8; nb]` + `vec![Value::Null; nr]`）。
   合成单块可省 16 字节的指针 + 一次 malloc + 一份分配器头。
3. **`native` 16 + `type_args` 16 = 32 字节**，两者对**绝大多数对象都是空的**
   （`NativeData::None` / 零长 `Box<[String]>`）——只有 WeakRef / Type / LoadContext /
   Assembly 四种内建盒子和泛型实例化用得上。

CPU 侧同一负载的 profile（1328 采样）：`alloc_object` 289（22%），其中
`object_regions` 的两次 malloc 约 78（**6%**）。所以这不只是内存题。

不做会怎样：z42 的「每对象 236 字节」对一个 4 基元 + 2 引用的类来说是 **4–6 倍**于
字段本身（37 字节）。评审目标里的「运行时内存像正式语言看齐」在对象密集负载上过不去。

## What Changes

按**实测字节数**排序（每项都可独立落地、独立度量）：

- **P1 终结器槽移出 `RegionEntry`**（−24 B/对象，−19% 槽位）。改为 region 级的
  `HashMap<(chunk_idx, entry_idx), FinalizerFn>` 侧表，只有真注册过的槽位才占内存。
  `register_finalizer` / `cancel_finalizer` / sweep 的对外语义完全不变，
  `arc_heap_tests/finalization.rs` 原样通过即为验收。
- **P2 `bytes` + `refs` 合成单块**（−16 B/对象 −1 次 malloc −1 份分配器头，CPU −6%）。
  新增一个自包含的 `ObjStorage`：一次分配，布局 `[refs: Value; nr][bytes: u8; nb]`
  （引用在前，天然 16 对齐），对外只暴露 `bytes()` / `bytes_mut()` / `refs()` /
  `refs_mut()` 四个安全切片访问器，`unsafe` 全部关在这一个类型里。
- **P3 `native` / `type_args` 瘦身**（−16 B/对象）。两者都改成「空即 8 字节」的形态
  （`Option<Box<…>>`），代价是内建盒子多一次间接——它们本来就是冷路径。

三项做完，按上表推算 236 → **约 164 字节/对象（−30%）**。

**不在本次范围**（明说，避免范围蔓延）：
- 把对象整体搬进 `VarRegion`（unify-gc-heap 的方向 A′）——那是另一个量级的改造，
  P2 的 `ObjStorage` 正好是它的前置。
- `Mutex<ScriptObject>` 的 8 字节 per-object 锁（要动并发模型，独立提案）。
- GC 头部剩下的 24 字节（marked/alive/gen_age/generation/location/soft）——都在用。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/gc/region.rs` | MODIFY | P1：`RegionEntry` 去掉 `finalizer` 字段；`Region` 加侧表 + take/set/clear |
| `src/runtime/src/gc/refs.rs` | MODIFY | P1：`GcRef::set_finalizer` / `cancel_finalizer` 改走侧表 |
| `src/runtime/src/gc/arc_heap/collect.rs` | MODIFY | P1：sweep 取终结器改走侧表；P2：`obj.refs` 扫描改走 `ObjStorage::refs_mut` |
| `src/runtime/src/gc/arc_heap/generational.rs` | MODIFY | P2：同上 |
| `src/runtime/src/gc/arc_heap/interface.rs` | MODIFY | P2：`alloc_object` / `alloc_boxed_prim` 构造改走 `ObjStorage` |
| `src/runtime/src/gc/arc_heap/alloc.rs` | MODIFY | P2：`script_object_size_estimate` 按新布局算 |
| `src/runtime/src/metadata/types/obj_storage.rs` | NEW | P2：`ObjStorage` 单块存储 + 其单测 |
| `src/runtime/src/metadata/types/object.rs` | MODIFY | P2/P3：`ScriptObject` 换 `storage`；`native` / `type_args` 瘦身 + 访问器 |
| `src/runtime/src/metadata/types/type_desc.rs` | MODIFY | P2：`object_regions()` → `object_storage()`（单块） |
| `src/runtime/src/metadata/types/mod.rs` | MODIFY | P2：导出 `obj_storage` |
| `src/runtime/src/interp/exec_object.rs` | MODIFY | P2：栈分配构造点 |
| `src/runtime/src/corelib/convert.rs` | MODIFY | P2：装箱路径构造点 |
| `src/runtime/src/corelib/reflection/accessors.rs` | MODIFY | P3：`type_args` 访问器 |
| `src/runtime/src/gc/region_tests.rs` | MODIFY | P1：侧表单测（注册→sweep 触发→cancel 不触发） |
| `src/runtime/src/metadata/types_tests.rs` | MODIFY | P2/P3：布局与访问器单测 |
| `bench/scenarios/09_alloc_ctorless.z42` | — | 复用（不改）：CPU 侧 A/B 载体 |
| `docs/book/src/dev/*.md` | MODIFY | 若 P1 改变了终结器的对外文档表述则同步 |
| `docs/spec/changes/shrink-object-footprint/*` | NEW | 本变更文档 |

## 验收判据（每项独立）

- **内存**：同一「留住 200 万 `Node`」程序的 peak RSS，逐项给改前/改后同机数字；
  三项做完目标 ≥ **−25%**。
- **CPU**：`bench/scenarios/09_alloc_ctorless` 同-runner A/B 不回归；P2 期望 ≥ +5%。
- **正确性**：`xtask test` GREEN（含 `arc_heap_tests/finalization.rs` 原样通过）、
  `cargo test --lib`、wasm32 检查 0 error。
- **启动**：按 `benchmarking.md`「布局彩票」的配方判定——看 instructions retired +
  死字段对照组，不看墙钟单点。

## 拍板结果（2026-09-04）

User：**三项都做，一起上**（一个 PR，三个 commit，每个带独立数字）。

实施时对 P1 有一处偏离：改成 entry-local 的 `AtomicPtr`（省 16 字节）而不是
Region 级侧表（省满 24）——理由见 `design.md`，简言之为最后 8 字节引入跨模块
API 改动 + sweep 每对象一次哈希查不划算。最终三项合计 **−24.4%**，
与 proposal 推算的 −27%（含侧表的满额）差的正好是这 8 字节。
