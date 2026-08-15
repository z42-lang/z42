# Design: 统一 GC 堆模型（变长 GC 分配器 A'）

> 状态：DRAFT（待 6.5 gate）| 创建：2026-08-15
> 前置：`unify-object-byte-layout`（Value 16B / 8B 引用句柄 / `GcRef` 8B 标记指针 / 对象字节布局）已完成
> User 三决策（2026-08-15）：动机=架构统一+为移动 GC 铺路（接受 GC 压力权衡）；分配器=**A'**；scope=string+closure+array **全包**

---

## 1. 当前状态地图（已核实，file:line）

### 1.1 GC 分配器 = 类型化定长槽
- `Region<T>`（`gc/region.rs:202`）：`chunks: Vec<Box<[MaybeUninit<RegionEntry<T>>; 256]>>`（`CHUNK_SIZE=256`，`:47`）。**槽宽 = `size_of::<RegionEntry<T>>()`，每类型定长**。
- `RegionEntry<T>`（`region.rs:54`）= per-object GC 元数据 **很重**：`value: Mutex<T>` + `marked: AtomicU8` + `alive: AtomicBool` + `gen_age: AtomicU8` + `generation: AtomicU32` + `finalizer: Mutex<Option<FinalizerFn>>` + `location: (u16,u16)` + `soft_ref_count: AtomicU32`。
- 分配：`alloc`（`region.rs:274`）free-list pop 快路 / bump 慢路；`tombstone`（`:358`）翻 alive + bump generation + push free-list；`iterate_alive`（`:508`）线性扫存活；generational（`young_list`/`card_dirty`）。
- **只 2 个 GC region**：`region_object: Mutex<Region<ScriptObject>>`（`arc_heap.rs:287`）+ `region_array: Mutex<Region<ArrayObj>>`（`:290`），在 `ArcMagrGC`（`:263`）。GC 模式 `GcMode`（`gc/mode.rs:18`）StwMarkSweep(默认)/Concurrent/Generational。
- **mark/sweep**：`mark_phase`（`arc_heap.rs:707`）BFS，初始队列 = `inner.roots`（`RcHeapInner.roots: HashMap`，`:152`）+ `external_root_scanner`（surfaces `VmContext.static_fields` + `pending_exception`）；`sweep_phase`（`:1184`）STW 全扫 `iterate_alive` → 存活 clear_mark、死者取 finalizer + 断强边（null `obj.refs` + `clear_inline_refs`）+ tombstone。write-barrier `write_barrier_field`（`:1933`）/`write_barrier_array_elem`（`:1971`）按模式 shade-gray/card。
- **两个近重复 ref-visitor**：`Value::trace_children`（`types.rs:1861`，权威中）+ `ArcMagrGC::scan_object_refs`（`arc_heap.rs:2073`，trial-deletion 用）——types.rs:1859 注「P3 删 trial-deletion 后 trace_children 变唯一权威」。**本程序为变长 payload 加 trace 边时两处都要改（或顺带收敛）**。
- `RegionHandle`（`region.rs:193`）= `{chunk_idx:u16, entry_idx:u16, generation:u32}`；`GcRef`（`gc/refs.rs:244`）PR-3 后 = **8B 标记指针**（低 48 位 RegionEntry 地址、高 16 位窄 generation，`Tagged<T>` cfg-gate wasm32，`:86/:89`）；`to_tagged_bits`/`from_tagged_bits`（`:406`）字节内联 codec。

### 1.2 三类变长数据现全在 GC 外
- **string** = `Value::Str(Str)`（tag=4，`types.rs:1583`）；`Str`（`metadata/vstr.rs:46`）= 手写 thin-Arc-DST，8B 细指针 `NonNull<StrHeader>`；`StrHeader{strong:AtomicUsize, len:usize}`（`:37`）+ 内联 UTF-8 字节紧跟同分配（`DATA_OFFSET=16`，`:66`）；Arc 语序 clone=Relaxed/drop=Release+Acquire fence+dealloc（`:152/:164`）。**完全在 GC 外**，**~323 处 `Value::Str(`（~241 非测试）**——热点 `corelib/reflection.rs`(47)/`jit/helpers/vcall.rs`(17)/`arith.rs`(15)/`corelib/repl.rs`(15)。interned 串池 = `Module.interned_strings: Vec<Str>`（`bytecode.rs:54`，`loader.rs:683` 由 `string_pool: Vec<String>` 建；`jit/vm_interface.rs:68/95` 访问）——**Module 拥有、refcount、GC 外**。
- **closure/delegate** = `Value::Closure(Box<ClosureData>)`（tag=10，`types.rs:1629`；`ClosureData{env: GcRef<ArrayObj>, fn_name: String}`，`:1775`——**env 已 GC 是唯一 trace 边，Box 本体 + fn_name 对 GC 不可见**）；`StackClosure(Box<StackClosureData>)`（tag=11，`:1641`，`{env_idx:u32, fn_name:String}`，栈 arena、逃逸分析产物）；`FuncRef(Str)`（tag=9，`:1619`，名字 = thin-Arc）。**~28 Closure + ~24 FuncRef** 构造点（`interp/exec_call.rs:319/339`、`jit/helpers/closure.rs:88/99`、`corelib/object.rs:158`）。
- **array 元素数据** = `ArrayObj{element_type: Arc<str>, backing: ArrayBacking}`（`types.rs:1237`）；`ArrayBacking` enum（`:1256`）= `Boxed(Vec<Value>)`(GC 扫) / `Bool/Bytes/I32/I64/Chars/F64(Vec<..>)`(**packed 基元、GC 跳过**) / `StructBytes{elem_size,bytes:Vec<u8>,refs:Vec<Value>,layout}`(`struct[]` 内联，基元 packed、引用叶子在 `refs` 侧表)。**元素缓冲 = Vec 自有外部分配**（在 `RegionEntry<ArrayObj>.value: Mutex<ArrayObj>` 内），GC 只跟 `ArrayObj` 头（`region_array`）；`gc_refs()`（`:1464`）返回引用切片，packed → `&[]` 跳过。**⚠️ `element_type: Arc<str>` 也是 GC 外 string**（顺带迁移面）。

---

## 2. 核心设计决策

### D1. 分配器方向 = A'（变长块 region）—— User 裁决
把 `Region` 泛化成支持**变长条目**：一个 GC 对象 = 单个变长块 `{GcBlockHeader, inline payload…}`，`GcRef` 8B 细指针指块头，**单次分配 + 内联数据**（等价现 vstr 紧凑度、纳入 GC）。string/array/closure 复用同分配器。

**否决 A（头在定长 region + 字节 arena 两级）**：每 string 2 次分配，不如现 Arc 1 次内联。**否决 B（废类型化 region、全变长）**：触及每条 GcRef 路径、重写最大；作为 A' 之后的收敛目标（out of scope）。

### D2. 变长块头 = 轻量 GC 头（区别于重 `RegionEntry<T>`）
现 `RegionEntry<T>` 的 `Mutex<T>` + soft_ref + finalizer 对**不可变叶子 string** 是纯浪费。变长块用**精简头** `GcBlockHeader`（见 §3.2）：只留 GC 必需（mark/alive/generation/size-或-size-class/type-tag），**string/array-of-primitives 叶子无 finalizer/soft_ref**。定长 `Region<ScriptObject/ArrayObj>` 的重头保持不动（对象仍需 per-entry Mutex + finalizer）。→ 变长 region 与定长 region **并存**，非替换（B 才替换）。

### D3. string 块头替换 Arc 引用计数
`StrHeader{strong:AtomicUsize, len}` → GC 块（`GcBlockHeader{…, len 或 size}` + 内联 UTF-8）。`strong` 原子计数**删除**（GC 管生死）；`len` 进块头或由 size 派生。`Value::Str(Str)` → `Value::Str(GcRef<…>)`（仍 8B）。interned 串池的 `Vec<Str>` → GC roots（进程存活的 interned 串是根，不被回收）。

### D4. StackClosure 不进 GC
`StackClosure`（逃逸分析产物、帧作用域栈分配）**保持栈**，与布局程序 `StackObject`/`StackArray` 同规则——只有逃逸的 closure 进 GC 堆。`Value::Closure(Box)` → `Closure(GcRef<ClosureData>)`；`ClosureData.fn_name: String` → GC string（复用 D3）；`env: GcRef<ArrayObj>` 已 GC，trace 边不变。

### D5. array backing 进 GC 变长块
`ArrayBacking` 的 `Vec<…>` 元素缓冲区 → GC 变长块内联。`ArrayObj` 定长头留 `region_array`（或并入变长块，实施定）；元素数据 = 变长块 payload，GC 直接扫（`Vec<Value>` 元素是 trace 边、`Vec<u8>`/`Vec<i64>` packed 是叶子字节）。`struct[]` 内联字节按对象引用位图扫。

### D6. 无格式 bump（预期）
string/closure/array 的**运行时表示**变，zbc/zpkg **序列化格式不变**（同布局程序 PR-3/4/5 纯运行时）。逐 PR 复核确认（无 fixture 重生 / 无两代自举墙 / 自举字节不动）。

### D7. 交付 = 5 PR，每 PR 独立 GREEN + rebase
仿布局程序纪律；不做单个巨红 PR。

---

## 3. A' 变长块分配器详细设计（PR-1）

### 3.1 目标
新增 `VarRegion`（或泛化 `Region`）：分配任意字节大小的 GC 块，返回 8B `GcRef`，接入现有 mark/sweep/generation/write-barrier。**inert 落地**——PR-1 只加分配器 + 单测，无消费者（string/closure/array 仍旧路径）。

### 3.2 块布局
```text
一个 GC 变长块（单次分配）：
┌────────────────────────────┬─────────────────────────────┐
│ GcBlockHeader (定长, 对齐 8) │ inline payload (变长)         │
│  ├ mark: AtomicU8           │  string: UTF-8 bytes          │
│  ├ alive: AtomicBool        │  array<prim>: packed bytes    │
│  ├ generation: AtomicU32    │  array<Value>: [Value; n]     │
│  ├ gen_age: AtomicU8        │  closure: ClosureData 字段     │
│  ├ size: u32 (payload 字节) │                               │
│  └ type_tag: u8 (Str/Arr…)  │                               │
└────────────────────────────┴─────────────────────────────┘
   ↑ GcRef 8B 标记指针指块头（低 48 位地址 + 高 16 位窄 generation）
```
- **type_tag** 让 GC sweep / trace 知道 payload 如何扫（叶子字节 vs Value 数组 vs closure）——变长 region 混装多类型，不像定长 `Region<T>` 靠 T 静态已知。
- **size** 让 sweep 知块大小以推进/回收；dealloc 用它算 `Layout`（同 vstr `layout_for`）。
- 无 `Mutex<T>`（string/array-of-prim 不可变或元素级另议）/ 无 finalizer / 无 soft_ref（叶子不需要）——比 `RegionEntry` 轻。closure 若需 finalizer 再评估（多半不需）。

### 3.3 内部布局：size-class free-list over 字节 chunk（初拟，Open Q）
- 变长 region 持 `Vec<Box<[u8; VAR_CHUNK]>>` 字节 chunk（chunk 内 bump 分配对齐 8 的块）。
- 回收：按 **size-class** 分桶 free-list（幂次或固定档），tombstone 的块按 size-class 入桶，alloc 优先同桶复用。
- **地址稳定**：字节 chunk Box-owned 不搬迁（同现 Region），块头地址稳定供 `GcRef` 身份 hash——直到该块被 sweep tombstone。
- **ABA**：generation 快照沿用（块头 `generation` + GcRef 窄 16 位快照，同 PR-3 标记指针）。
- **备选**：纯 bump + sweep 整理（无 free-list，碎片靠未来压缩）——A' 不做压缩，故需 free-list 控碎片。size-class vs 精确 best-fit 是 Open Q，PR-1 定。

### 3.4 mark / sweep / generation 集成
- `iterate_alive` 等价物：变长 region 线性扫块（按 size 推进游标，跳 tombstone），交 visitor。
- **trace**：mark 阶段遇 `GcRef` 指变长块 → 按 type_tag 决定是否递归扫 payload 内的 Value（array<Value>/closure 有引用边；string/array<prim> 是叶子终止）。
- **sweep**：未 mark 块 → tombstone（alive=false + bump generation + 按 size-class 入 free-list）。
- **write-barrier**：array<Value> 元素写、closure 字段写触发 barrier（同现 `write_barrier_field`）；string 不可变无 barrier。
- generational：变长块也带 gen_age，纳入 young_list / card（或初版仅 STW，generational 后补——Open Q）。

### 3.5 PR-1 交付 + 验证
- inert `VarRegion` + `GcBlockHeader` + alloc/tombstone/iterate/trace-hook + size-class free-list。
- **单测**（含 Miri/ASAN 敏感 UB）：alloc 各 size / free-list 复用 / generation ABA / sweep tombstone / 地址稳定 / trace payload 分类。
- 无消费者 → 行为不变、自举字节不动、无格式 bump。

---

## 4. string 进 GC（PR-2）

- `Str`（vstr.rs）Arc 头 → GC 块头（D3）；`Value::Str(Str)` → `Value::Str(GcRef<StrHeader>)`（8B 不变）。
- **interned 串池** `Vec<Str>` → GC roots（`bytecode.interned_strings` / jit `frame.string_pool` / `interned_strings()`/`try_lookup_string()` 等，PR-4-of-layout 已全迁 `Vec<Str>`，此处改为 root 注册 + GcRef）。const-str O(1) clone 变 GcRef clone（更便宜）。
- str_meta 缓存身份 key（现 `Str::as_ptr`）→ GcRef 块头地址。
- 删 vstr Arc drop（GC 管生死）；`Str: Send+Sync` 语义由 GC 堆的线程模型给出。
- **~287 处迁移** + benchmark（z42c 自编译 string-heavy 最敏感——GC 压力实测门禁）。
- `PartialEq` 按内容不变；`len()` 从块头 O(1)。

## 5. delegate/closure 进 GC（PR-3）

- `Value::Closure(Box<ClosureData>)` → `Closure(GcRef<ClosureData>)`；`ClosureData` 进变长块（`fn_name` 复用 GC string、`env` 已 GcRef）。
- `FuncRef(Str)` 名字复用 GC string。
- `StackClosure` 保持栈（D4，逃逸分析产物不进 GC）。
- trace：closure 块扫 `env` 边 + `fn_name`（GC string）边。~40 处迁移。

## 6. array backing 进 GC（PR-4）—— 最大最敏感

- `ArrayBacking` `Vec<Value>`/`Vec<u8>`/`Vec<i64>`… → GC 变长块内联（D5）。
- `ArrayObj` 头与元素块关系：头留 `region_array` 指变长元素块，或头也并入变长块（实施定，倾向后者以彻底单块）。
- **packed 基元数组**（typed backing）= 叶子字节块；**`struct[]`** = 内联 struct 字节块按引用位图扫；**`Value[]`** = Value 数组块逐元素 trace。
- 触碰 packed 数组 / struct[] / 逃逸分析 StackArray（保持栈）——爆炸半径最大，benchmark 敏感。可能进一步内部分块。

## 7. 收敛 + 文档（PR-5）

- 删 Arc/Box/外部 Vec 双路径残留；单一堆 mark/sweep/barrier/统计口径统一。
- `docs/design/runtime/gc.md`/`gc-handle.md`（变长分配器 + 单一堆）、`object-abi.md` §5/§6、`roadmap.md` 收口。

---

## 8. 权衡与风险（见 proposal §⚠️，此处补实施风险）

- **UB 面大**：手写变长分配器 + 标记指针 + 不安全 payload 读写 → Miri/ASAN 是硬门禁（PR-1 起）。
- **GC 压力回归**：string-heavy 自编译纳入 GC → 内存峰值/GC 频率↑，benchmark 门禁量化，超阈值需调 GC 触发策略。
- **并发**：变长 region 的 alloc/sweep 锁粒度（现定长 region 用 `Mutex<Region>`）；ConcurrentMarkSweep 下变长块 mark race（同现 stale-mark race 风险面）。
- **all-or-nothing 迁移**：PR-2/3/4 各自是 payload 类型的全量切换（~287 Str / ~40 Closure / array 全部），须一次落地才绿——同布局程序 PR-2 经验。

---

## 附：Decision Log（实施中追加）

### D13. array backing 迁移设计（PR-3）—— 代码地图已核实（Explore）
**关键事实**：z42 数组 **定长**（C# `T[]`，无 grow/resize opcode）→ 固定 GC 块足够；JIT hoist 已假设 buffer 不搬迁（`helpers/array.rs:112`）→ A' 非移动块契合；`StackArray`（逃逸分析，arena-owned，非 GcRef）**保持 arena inline Vec、不进 GC**（parallel 非 GC 策略）。爆炸半径 ~50 站点（~16 构造 + ~10 读 + ~6 写 + ~8 trace + JIT inline 4emit/2helper + ~6 FFI）。

**表示（已落地）**：`ArrayBacking` 各变体 `Vec<T>` → **GC 变长块**（`VarGcRef` into `region_var`），`ArrayObj` 头仍在 `region_array`、Mutex 保护，块由头唯一拥有、经头的 `borrow`/`borrow_mut` 锁访问 = 可变安全。每变体存 `{ block: VarGcRef, len: usize }`（`len` 内联在 backing，避免热路径 `len()` 走块头；数组定长 → `len` 不变、不与块脱同步）。变体保留判别（决定 boxing 语义 + 块布局）：
- **packed（I32/I64/F64/Bytes/Bool/Chars）**：`BlockType::ArrayPrim` 块 = 紧凑 `[T; n]` 字节（POD 无 drop-glue）。`packed_num_ptr`/`as_bytes` = 块 payload 指针（连续、JIT/FFI 直取）。
- **Boxed**：`BlockType::ArrayValue` 块 = `[Value; n]`（16B/元素）。drop-glue drop 每个 Value（Str Arc refcount 等）。`gc_refs` = 块内 Value 切片。
- **StructBytes（struct[]）** = **两块**（实施定案，与 groundwork 的 `var_drop_glue` 分派一致；非初稿的「同一块两区」）：`bytes` 块（`BlockType::ArrayStruct`，`len*elem_size` POD 字节，drop-glue 不 drop）+ `refs` 块（`BlockType::ArrayValue`，`len*ref_count` Value 侧表，drop-glue drop 每个 Value）。分两块避免混合 drop 语义 + 对齐 pad 数学；`gc_refs` = refs 块整切片，`struct_bytes()`/`struct_bytes_mut()`/`struct_refs_mut()` 暴露子区给 exec 层做 leaf 偏移读写。ArrayObj 存 `elem_size/len/layout`（`layout` = 类型元数据 `Arc`，非 per-instance 数据、留在 GC 外）。
- **StackVec（逃逸分析栈数组，非 GC）**：`ArrayBacking::StackVec(Vec<Value>)` —— 非逃逸数组的 arena inline `Vec`，**不进 GC**（同 `StackObject` off-GC Box / `StackClosure` arena env，escape-analysis 产物故意绕开堆，D13 铁律）。`ArrayObj::stack_typed`（无 heap）产出；元素作 GC root 扫描，`mark_backing` no-op、无块。

**访问改造（已落地）**：`get_boxed`/`set_boxed`/`len`/`gc_refs`/`as_bytes`/`packed_num_ptr`/`to_boxed_vec`/`iter_boxed`/`elem_storage_bytes`/`struct_bytes`/`write_struct_elem` 从 `v[i]`/`v.as_ptr()` → 私有 `slice_of::<T>`/`slice_of_mut::<T>`（`block.payload_as_ptr::<T>()` 派生自原始头指针，D8）块 payload 偏移读写。访问在 ArrayObj borrow/borrow_mut 锁下 = 独占/共享安全。**删除**无生产调用方的 `boxed_slice`/`boxed_slice_mut`/`clear`/`capacity`（定长块无 grow/clear）。
**移除 `#[derive(Clone)]`（坑B）**：ArrayObj/ArrayBacking 不再 derive Clone（浅拷会别名块）→ 加 `deep_copy(&self, heap)` 分配新块深拷（`__array_clone` 走它 + `alloc_array_obj` region-alloc，替代 leaking `GcRef::new`）。Value derive Clone 不受影响（`Array(GcRef)` 的 Clone 是句柄级、不 clone ArrayObj）。
**构造需堆**：`ArrayObj::new/typed/from_bytes/struct_backed` 分配元素块 → 收敛 `&dyn MagrGC` 参数（经 `alloc_var_block` trait 方法）。ctx-less 站点（`corelib/string.rs` builtin_str_to_chars、`exec_array.rs`）线程 `ctx.heap()`（多数已有 ctx；不需 string 的 ambient 堆——那是 PR-4）。测试用 leaking `new_leaked`/`struct_backed_leaked`（`VarGcRef::leak_block_for_test`，镜像 `leak_for_test`）。
**GC**：mark ArrayObj(region_array) 时 `trace_children` 的 Array/RefArray/StructRefHeap 臂调 `arr.mark_backing()` 标块（覆盖 STW/minor/concurrent，因所有 mark 循环 `mark_if_unmarked` 后都调 trace_children）；sweep region_var 回收未标块（drop-glue drop Boxed/struct-refs 的 Value）。array 头 tombstone 不再急切 `.clear()` 元素（块 sweep 于同 cycle 由 drop-glue 回收）。
**JIT 铁律**：packed inline get/set 的 `jit_array_data`/`_opt` 经 `packed_num_ptr` 返回块 payload 指针替代 `Vec::as_ptr`；块非移动（A'）、定长故 hoist 缓存的 ptr 全函数有效不变。**benchmark packed 数组**（JIT 热路径）量化无回归。
**复用 PR-2 drop-glue**：`closure_drop_glue` 已（groundwork）扩成 `var_drop_glue`（按 BlockType 分派 Closure→ClosureData / ArrayValue→drop [Value]（Boxed 元素 + struct[] refs 侧表）/ Str·ArrayPrim·ArrayStruct→POD）。

### D12. closure 迁移精确落点（PR-2 核心，实施中）—— 代码地图已核实
**表示**：`Value::Closure(Box<ClosureData>)`（tag=10）→ `Value::Closure(VarGcRef)`（8B 不变，Value 仍 16B）。`ClosureData{env: GcRef<ArrayObj>, fn_name: String}`（`types.rs:1775`，derive Debug/Clone）进 GC 变长块（type_tag=Closure）。**ClosureData 创建后不可变**（env/fn_name write-once）→ 变长块**无需 Mutex**（不同于 ScriptObject），访问走**无锁共享 `&ClosureData`**（靠可达性保活，同 GcRef 模型）。
**已核实站点（grep）**：7 处构造 + 19 处 `Value::Closure(` 模式 + ~17 `.env` + ~13 `.fn_name`。`Value` derive Clone → Closure(Box) 现在**深拷** ClosureData；换 VarGcRef(Copy) 后 Clone = 浅拷句柄（**共享 closure，更符合 GC 语义**，是改进不是回归）。

**落点清单**（cargo build 错误列表驱动）：
- **arc_heap.rs**：① 加 `region_var: Mutex<VarRegion>` 字段（Default 用 `VarRegion::with_drop_glue(closure_drop_glue)` 初始化）；② `closure_drop_glue(bt, p)` = `if bt==Closure { drop_in_place(p as *mut ClosureData) }`；③ `mark_phase`(`:726`)/`mark_if_unmarked`(`:762`) closure 臂 `GcRef::mark(&c.env)` → `c.mark()`（VarGcRef::mark 标块）；④ `sweep_phase`(`:1184`) 加 `region_var.lock().sweep()`（**须在 mark 之后**，drop-glue 自动跑 ClosureData::drop 释放 fn_name）；⑤ `scan_object_refs`(`:2073`) closure 臂同步；⑥ `alloc_closure(env, fn_name) -> Value::Closure(VarGcRef)`（alloc 块 size_of::<ClosureData>() + payload_as_ptr::<ClosureData>().write）+ Heap trait 方法（`heap.rs`）。
- **types.rs**：① 变体 `Closure(VarGcRef)`；② `trace_children`(`:1876`) closure 臂：`unsafe{&*vref.payload_as_ptr::<ClosureData>()}` 读 ClosureData → `visit(&Value::Array(env))`（推 env 让 mark 循环标 env array + 扫元素；等价现「mark 臂标 env + trace 扫元素」）；③ `is_heap_ref` Closure→true 不变（模式 `Closure(_)` 仍匹配）；④ 加 `Value::closure_data(&self)->Option<&ClosureData>`（无锁读，lifetime 系 &self，靠可达性 sound）；⑤ `PartialEq`/`Debug`(derive) closure 臂——derive 用 VarGcRef 的 Clone/Debug 自动 OK；`__delegate_eq` 若按 fn_name 比需走 closure_data。
- **构造点**（7 处：`interp/exec_call.rs:339`、`jit/helpers/closure.rs:88/99`、`corelib/object.rs:158` 等，均有 ctx/vm_ctx_ref）：`Value::Closure(Box::new(ClosureData{env,fn_name}))` → `ctx.heap().alloc_closure(env, fn_name)`。
- **访问点**（~17 `.env`/~13 `.fn_name`，滤掉 StackClosure/env_idx/env_arena）：`c.env`/`c.fn_name` → `c.closure_data().env`/`.fn_name`（`if let Value::Closure(c)` 处）。
**GC 正确性铁律**：mark 必须标 region_var 块（漏标→sweep 早释→UAF）；sweep region_var 必须在 mark 之后；drop-glue 释放 fn_name 恰一次（reuse/teardown 已 Miri 验，D8/2a）。**验证**：cargo --lib + Miri + self-host 5/5 逐字节（**评估无格式 bump**，Value 表示变但不序列化）+ closure e2e（生命周期/捕获/GC 回收/delegate eq）。
> **fn_name 的最终形态**：PR-4（string 进 GC）后 fn_name 变 GC string → ClosureData 全 POD（env GcRef + fn_name VarGcRef）→ **可删 closure drop-glue**。PR-2 的 drop-glue 是 string-未-GC 期的 interim。

### D8. payload 指针必须从原始 NonNull 派生，不经 `&GcBlockHeader`（Miri SB，PR-1）
**症状**：Miri（Stacked Borrows）报 `write_bytes` 越界写——`payload_ptr(&self)` 通过 `&self`（`&GcBlockHeader` 共享引用，provenance 只覆盖 16B 头部）派生 payload 写指针（offset 16），写 payload 是**出该 tag 边界 + 通过 SharedReadOnly 写** = 双重 UB。
**根因**：`&GcBlockHeader` reborrow 把 provenance 收窄到 16B 头；payload 在 [16..]，通过它派生的指针既越界又只读。
**修**：删 `GcBlockHeader::payload_ptr(&self)` 方法 → 改自由函数 `payload_ptr_of(header: NonNull<GcBlockHeader>)`，从**原始 NonNull**（保留整块 chunk-allocation provenance）`.cast::<u8>().add(DATA_OFFSET)` 派生。`payload()`/`payload_mut()` 也改为先在 scoped `&` 里读 guard 元数据（size/gen/alive，都在 [0..16] 内合法），再从原始 `ptr` 派生 payload 指针。**教训：头+内联变长 payload 的模式里，任何跨越头进入 payload 的指针都必须从整块原始指针派生，绝不经窄 `&Header`**——PR-2/3/4 迁 string/closure/array 时同款铁律（vstr.rs 现用 `NonNull` 直接派生也是这个道理）。

### D10. PR 顺序重排 closure→array→string（string 是最难而非最易，User 裁 2026-08-15）
**Explore 堆访问测绘发现（事实校正）**：原设计把 string 排第一个消费者是**风险最高排序**。堆经 `ctx.heap()`（`&VmContext` 线程穿透，`vm_context.rs:1171`；堆在 `VmCore.heap`，`:211/:600`）访问，**无通用 ambient 堆访问器**（仅 `native/exports.rs:33` 的 `CURRENT_VM` thread-local，native-interop 门控 + 加载期不覆盖）。
- **string = 最难**：所有 `Value::Str` 走 `vstr::Str::new` 全局分配器（~188 处 `.into()` 经 `From<&str> for Str`，`vstr.rs:192`），**且 interned 串池在模块加载期建**（`merge.rs:26`/`loader.rs:686`，VM/堆/帧都还不存在）+ 临时 string 的 GC-safepoint 纪律。z42c 自编译 string-heavy → 风险最集中处撞第一枪。
- **closure = 最易**：~28 创建点全有 `ctx`（`interp/exec_call.rs:319/339`、`jit/helpers/closure.rs`）。
- **array = 中**：已走 `ctx.heap().alloc_array(...)`，创建点有 ctx，但面大（packed/struct[]）。
- **NativeFn** `fn(&VmContext,&[Value])`（`corelib/mod.rs:68`）+ JIT `vm_ctx_ref(ctx)`（`jit/helpers/mod.rs:69`）→ corelib/interp/JIT 创建点都有堆访问；纯自由上下文只剩 string 的加载期 interning + `.into()` choke。
- **root 钩子**：`set_external_root_scanner`（`vm_context.rs:672`）——GC string 时 `Module.interned_strings` + `JitModuleCtx.string_pool` 在此注册为 roots。

**裁决**：**执行顺序 = heap 接线 → closure → array → string**（重排后 PR-2=closure、PR-3=array、PR-4=string）。让变长分配器→mark→trace→sweep 全链路先用「创建点有 ctx」的简单 payload（closure）跑通、battle-tested，再啃 string 的弥散分配。string 数据最简（不可变叶子、trace 平凡）但**分配管线最难**，放最后。

### D11. string 分配走「堆作为一等 ambient 服务」的最本质方向（User 裁 2026-08-15，string PR 详设）
User 选「最本质的方向」（非 188 处线程穿透 hack、非凑合）。方向 = **CLR/JVM 模型：GC 堆是 ambient 分配服务**——把现有 `CURRENT_VM` thread-local 泛化成正规 `current_heap()`（ungate native-interop、每个 `exec_function` 已由 `VmGuard` 包裹，`interp/mod.rs:745`），`vstr::Str` 分配从 ambient 堆取（保 188 处 `.into()` 不变）；**interned 串作永久 GC roots 在加载期分配**（堆在 VmCore 早于模块加载即存在，把堆引用/guard 引入 loader/merge 这个有界单点）。**string PR 时展开完整详设 + safepoint 纪律**（此处仅记方向，string 已排最后）。

### D9. PR-1 收敛为独立分配器 + Miri，heap 接线推到 PR-2（inert 不加死代码）
tasks 原列 PR-1 含「ArcMagrGC 加 `region_var` + mark/sweep 驱动」。实践中：PR-1 无任何 payload 消费者 → 往 ArcMagrGC 加 `region_var` 却无 `VarGcRef` 流经任何 `Value` = 要么死代码（`allow(dead_code)`）、要么无法端到端验证的接线（真 mark_phase 没有指向变长块的 root）。按 philosophy「不加死代码/最终方案优先」，**PR-1 = 自包含 `VarRegion` 分配器 + Miri-clean 单测**（mark/sweep 语义已由 `VarRegion::mark/sweep` 单测覆盖）；**heap 接线（`region_var` 字段 + mark_phase/sweep_phase 驱动 + `alloc_var_block`）移到 PR-2 首步**，与首个消费者 string 同落地、被真实分配驱动验证。
