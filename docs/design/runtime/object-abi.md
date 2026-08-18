# 对象与值表示 ABI（Object & Value ABI）

> **状态：DESIGN（值/对象表示已实施，规范化+演进未实施）** · 创建 2026-06-21
>
> 把当前**隐式**的跨引擎值/对象表示固化成**显式、版本化的 ABI**（组件化的"共享契约"本体），并为**移动/分代 GC**预留空间、**统一所有堆对象**（含字符串）到一个对象头。
>
> 精确 GC 的另一半"谁是 ref"在此（与 [safepoint.md](safepoint.md) 的"GC map@安全点"互补）；消费方：interp / JIT / AOT 三引擎 + GC + [load-context.md](load-context.md)。

---

## 1. 现状（已成形，但隐式且脆弱）
- **Value = Rust tagged enum**（[metadata/types.rs](../../../src/runtime/src/metadata/types.rs)）：`I64=0/F64=1/Bool=2/Char=3`（内联值）、`Str(Str)=4`、`Array(GcRef<ArrayObj>)=6`/`Object(GcRef<ScriptObject>)=7`、`Closure(VarGcRef)`、`Ref/PinnedView/StackClosure/StructRefHeap`(→ 8B `{idx,frame_id}` transient-arena 句柄，见 §2.2)。**`Value` 现为 `Copy`（16B POD，无 `Drop` glue）**。
- **ScriptObject** = `{ type_desc: Arc<TypeDesc>, slots: Box<[Value]>, native: NativeData }`。
- **GcRef** = `NonNull<RegionEntry<T>> + generation`（ABA 防护）；RegionEntry **Box-owned 永不重定位 → 当前非移动堆**。
- **JIT 与 interp 共享内存 Value 表示**：JIT 直接 `store tag`+payload 到帧的 Value 寄存器数组，**硬编码 tag 值 + 偏移**。
- 三套内存管理：Arc(`TypeDesc`/`Str`)、GcRef(`Array`/`Object`)、Box(`Closure`)。

**核心问题**：已有一份**事实上的跨引擎 Value ABI**，但它绑死 rustc 对 enum 的布局——隐式、脆弱。**本文 = 把它固化成显式版本化规范。**

---

## 2. 值表示（Value）
- **固化为稳定 ABI**：`#[repr(C)]`（或文档化布局）+ **tag 值表 + payload 偏移规范**，interp/JIT/AOT 对契约编码，不靠 rustc 心情。tag 值（`I64=0`…）已是公开判别值，纳入规范并冻结（变更 = ABI 版本 bump）。
- **保留 fat tagged 值**（tag + payload，~16–24B）作 v1；**NaN-box / tagged-pointer 压缩进 Deferred**（后续优化，复杂度大，现 fat enum 够用）。
- 跨引擎契约：一个 Value slot 的 `{tag 偏移, payload 偏移, 总大小}` 是 ABI 一部分；JIT/AOT 据此 load/store。

### 2.1 引用压到平台指针大小 → `Value` 24B→16B（✅ 已落地，路 A，2026-08-15 `unify-object-byte-layout` PR-3~5）

> **状态（2026-08-15）**：本节从「Deferred 候选」提为**已采纳并落地**，走**路 A（标记指针）**，`Value` 现为 **16B**。分 PR 落地：
> - **PR-3**：`GcRef`/`WeakGcRef` 16B→8B 单标记指针（低 48 位 RegionEntry 地址、高 16 位窄 generation，deref mask）。**保留非移动 region GC**（generation 变窄，ABA 窗口 2^16 已接受，见 §4 / Decision 2）。wasm32（usize 32 位）按 `target_pointer_width` cfg-gate 成 `{ptr:NonNull(4B), generation:u32(4B)}` 仍 8B。
> - **PR-4**：`Value::Str` 从 `Arc<str>`(16B 胖) 换成手写 thin-Arc-DST `Str`（[`metadata/vstr.rs`](../../../src/runtime/src/metadata/vstr.rs)，8B 细指针，长度进 `StrHeader`）。**interim**：仍 Arc refcount，非 tracing GC；string 全 GC 化（`Value::Str`→`GcRef<StrHeader>`）是后续专项（需变长 GC 分配器，与 §5 合流）。
> - **PR-5**：`Value::FuncRef` 从 `Box<str>`(16B 胖) 换成 `Str`（8B 细）——这是最后一个 16B payload。至此每个 payload ≤ 8B → `#[repr(C,u8)]` 给出 tag(1B padded to 8) + 8B = **16B**。由 [`types.rs`](../../../src/runtime/src/metadata/types.rs) 的 `const _: () = assert!(size_of::<Value>()==16)` 编译期锁死；JIT 的 `VALUE_STRIDE`/`STRIDE` 从硬编码 24 改为 `size_of::<Value>()`（单一真相，不再漂移，[`jit/translate.rs`](../../../src/runtime/src/jit/translate.rs)）。
>
> payload 偏移**不变**（tag@0、payload@8）；只有总 stride 24→16。native FFI 的 `Z42Value` 是**独立冻结的 16B ABI struct**（`{tag:u32, reserved:u32, payload:u64}`，[z42-abi](../../../src/runtime/crates/z42-abi/)），与内部 `Value` enum 表示解耦，marshal 显式转换 → 本变更不触及 native ABI。

**动机**：CLR/JVM 的对象引用 = **单个平台指针（8B）**，我们的是 **16B**。两个来源（已核对）：① `GcRef` = `NonNull<RegionEntry>`(8B) + `generation:u32`(4B, ABA 防护) 对齐 16B（[refs.rs](../../../src/runtime/src/gc/refs.rs)）；② `Value::Str` = `Arc<str>` 胖指针 = ptr8+len8 = 16B。因 `Value` 最大 payload = 16B → **`Value` enum 被钉在 24B**（`#[repr(C,u8)]`，JIT 按 `regs_base + reg_idx*24` 内联寻址）。若最大 payload 降到 8B，`Value` 可 **24B→16B**：每个寄存器 / 数组 boxed 元素 / 对象槽省 33%，全 VM 密度 + cache 收益。

> **前提校正**：主要收益是**内存/cache 密度**，**不是 native 交互**——托管引用（带 generation 的 region 句柄 / Arc）本就不能直接交给 native；FFI 零 marshaling 靠 struct **基元字节打包**（见 [struct-value-semantics.md] D1-a），与引用宽度无关。

**两条路（generation 是障碍）**：
- **A｜标记指针（改动小）**：x86-64/ARM64 虚拟地址仅 48 位，把窄 generation 塞进指针高 16 位（tagged pointer），deref mask 掉。**保留现有 region + 非移动 GC**，generation 变窄（ABA 窗口需评估）；deref 一次 mask（廉价）+ 与 ARM MTE/PAC、ASAN 交互需注意。参考 V8/JVM compressed oops（甚至到 4B）。
- **B｜移动式 tracing GC（改动大）**：引用永远指活对象、GC 移动统一改写 → 从构造上无悬垂，generation 直接不需要（CLR 模型）。与本 doc §6「移动/分代预留」同向，但最重。

**String 侧**：`Arc<str>` 的"胖"只在 8B 长度 → 换 `Arc<StrHeader{len,[u8]}>` 细指针 = 8B（长度进堆对象头，CLR/JVM 模型，与 §5「字符串改 GC」合流）；代价 = 取 len 多一次解引用。

**范围**：全 VM 横切（GcRef 句柄模型 + String 表示 + `Value` 布局 pin + **JIT 24B 硬编码寻址** + `value_layout` 断言），属 **B-radical 统一值类型模型**子目标，**不在 struct P3b**（P3b 保持 16B 引用不动）。落地前需与 §6 移动/分代 GC 的 `gc_word`/forwarding 设计一并评估路 A vs 路 B。

### 2.2 `Value` 成为 `Copy` —— 4 个 Box 瞬态变体 → arena 句柄（✅ 已落地，2026-08-18 `make-value-copy`）

> **对齐**：2026-08-18。**动机（实测驱动）**：interp-bound workload（z42c 前端）profile 中 `Value::clone`
> 是**头号 leaf（11.4%）**，`drop_in_place<Frame>` 再占 **6.0%**。根因**不是**堆操作——`unify-gc-heap`
> 之后 clone 已无 refcount（`GcRef::clone`=8B memcpy、`Str`=`Copy`、`GcRef::Drop`=no-op）——而是
> `Value` 仍挂 **4 个 `Box` 冷变体**（`Ref`/`PinnedView`/`StackClosure`/`StructRefHeap`）+ `GcRef` 的
> 显式 no-op `Drop`，逼编译器把每次 clone 编成「match 判别号 + drop-glue」、把 `Vec<Value>` 析构编成
> 逐元素循环，**无法退化成平凡 memcpy / O(1) 释放**。

**改动**：把这 4 个「仅在创建帧的调用栈内存活、创建后不可变」的瞬态变体，从 `Box<T>` 改为 8B
`{ idx:u32, frame_id:u32 }` 句柄，payload 存进 per-`VmContext` 的 **`TransientArena`**
（[`interp/transient_arena.rs`](../../../src/runtime/src/interp/transient_arena.rs)）；`GcRef` 删除显式
no-op `Drop` 并加 `Copy` → **`Value` 派生 `#[derive(Copy)]`**。

- **`TransientArena` 生命周期模型**：与 `StackArena`/`StructArena` 同构——`Vec<TransientSlot>`（`Mutex`
  保护）、`frame_id` staleness 守卫、`push_frame` 戳 `transient_base` / `pop_frame` LIFO `truncate`、
  每次 GC 作 **root 扫描**（`scan_roots`）。interp 与 JIT 共用同一 arena + `push_frame`/`pop_frame`
  base（JIT 经 `struct_ops::frame_id_of` 懒分配帧 id，与既有 `StructRef` 句柄同法）。
- **GC**：arena 是 root → payload 内 GcRef（`Ref` 的 Array/Field 目标、`StructRefHeap` 的 backing 数组）
  恒被标记；故 `Value::visit_gc_children` / `arc_heap::mark_if_unmarked` 对这 4 变体是 **no-op**
  （同 `StructRef`/`StackObject`）——**净效果是从 GC mark 热路径移除工作**，无需写屏障（root 每次重扫）。
- **相等 / stringify 退化**：4 变体 `==` 按 `{idx,frame_id}` 句柄相等（同 `StackObject`）；`value_to_str`
  返回通用占位串——照 `StackObject`/`StructRef` 先例（ToString 是 escape sink，这些瞬态句柄永不到达
  用户可见 stringify 路径）。有 `ctx` 的消费点（`deref_ref`/`UnpinPtr`/FFI marshal/FieldGet `.ptr/.len`/
  `CallIndirect`/`StructFieldGet(Set)Prim`/`__delegate_*`）经 `arena.with(idx,frame_id,…)` 读真 payload。
- **native marshal**：`value_to_z42`（无 `ctx`）的 `PinnedView` 防御臂退化为明确错误——编译器路径本就
  先 `FieldGet ptr/len`（经 arena 解析）再传标量，从不把 raw view 交给 marshal。

**效果（实测）**：前端 typecheck big.z42 **7.37s→6.19s = 1.19×（16% faster）**，输出逐字节一致，
`Value::clone` 离开 profile 头部、`drop<Frame>` 195→68 样本。`size_of::<Value>()==16` 不变（8B 句柄）。
**无 zbc/zpkg 格式 bump**（纯运行时表示）。这是 §2.1 布局线的自然延续：§2.1 把 `Value` 压到 16B，
§2.2 把它变成真正的 POD（`Copy` + 无 `Drop` glue）。

---

## 3. 统一对象头 + 对象种类（去掉 ad-hoc `native`）

**所有堆对象共用一个头**，按 **object kind** 区分 payload（精确 GC 据 kind 扫）。**普通用户对象不再带 `native` 字段**（省空间）。

### 统一头
```
ObjectHeader {
    gc_word:   usize,   // mark/color 位 + age/generation 位 + lock/hash 位；
                        // GC 复制期复用为 forwarding pointer（JVM mark-word 式）
    type/kind: ptr,     // → TypeDesc（含字段布局/vtable/反射）或 kind 判别
}
```
> 注:当前 mark 在 `RegionEntry` 上、对象无 GC 字。**为移动/分代,规范要求对象自带 `gc_word`**（见 §6）。

### 对象种类
| kind | payload | 精确 GC 扫描 |
|---|---|---|
| 普通 ref 对象（用户类） | `slots: Value[]` | 逐 slot 看 tag（`Array`/`Object` 才 trace） |
| **字符串（改 GC，§5）** | len + UTF-8 字节 | 无内部 ref，跳过 |
| 字节/原始缓冲 | 原始字节 | 无内部 ref，跳过 |
| ref 数组 | element_type + 元素 Value[] | 扫元素 |
| 弱引用对象 | weak handle | **不 trace target** |
| Type 对象（反射） | 引用 TypeDesc | 该引用是保留边（§7 边界） |
| **不透明 native（未来 Stream/FileHandle）** | 原始 native ptr + **finalizer** | 无 ref；收集时跑 finalizer（§5.1） |

→ `ScriptObject.native: NativeData` ad-hoc 字段**消除**；`WeakRef`/`TypeHandle`/未来 `FileHandle` 变成上述 kind。

### slots 布局（= 对象内存布局本体，跨引擎 ABI）
- `slots` 是**实例字段存储**:定长 `Value[]`,槽数 = `TypeDesc.fields.len()`,`alloc` 时定死不增长。
- 名→槽由 `TypeDesc.field_index`（类级共享）。**继承:基类字段在前、子类追加**（基类槽号父子稳定）。
- 访问 `obj.f` = `slots[常量槽号]`（O(1)）；JIT = `slots 基址 + 槽号×sizeof(Value)` 的 Value 大小 load/store → **槽偏移 + Value 大小是 ABI 一部分,须固化**。

---

## 4. GcRef 语义
- 现:`NonNull<RegionEntry> + generation`(ABA 防护)。**改名 `generation`→`epoch`**:避免与**分代 GC 的 young/old generation** 混淆。
- **必须"可重定位"**（为移动 GC，§6）。两方案(fork,待 benchmark):
  - **(a) 稳定 entry + 重定位 payload + 精确 fixup**:GC 把所有 GcRef 改写到新址(evacuation+fixup)。访问无额外间接;移动时全堆 fixup。
  - **(b) 句柄表间接**:GcRef→表→对象;移动只改表一格,访问多一跳。
  - young 复制式偏好 (a)+bump 分配。**fork 留文档,实现期 benchmark 定。**
- 访问含 `epoch` 校验(use-after-free 安全);JIT 可在可证明安全处 elide。

---

## 5. 字符串改 GC 对象

> **✅ 已落地（unify-gc-heap PR-4，2026-08-16）**：`Value::Str` 的字节**已纳入单一 GC 堆**。
> `Str`（[`metadata/vstr.rs`](../../../src/runtime/src/metadata/vstr.rs)）从「手写 thin-Arc + 原子
> refcount」换成 **8B `VarGcRef`**（`gc/var_region.rs` 的变长块，`BlockType::Str`，`{GcBlockHeader,
> inline UTF-8}` 单次分配）——**refcount 删除，GC 管生死**（mark/sweep）。分配走 **ambient 堆**
> （`gc/ambient.rs`，每帧 `HeapGuard` 设 thread-local，`Str::new`/`.into()` 保持不变）；无堆上下文
> （无 VM 的单测）回退 leaked 块。**这是最后一类离开「GC 外」的变长 payload，统一堆模型闭合。**
> 实现原理（变长块 `VarRegion` / A' 分配器 / D3 块头替 Arc / D11 ambient 堆）详见变更容器
> [`docs/spec/changes/unify-gc-heap/design.md`](../../spec/changes/unify-gc-heap/design.md) 与本节下方；
> `gc.md` / `gc-handle.md` 的统一堆机制页归 **PR-5 收敛**统一落地（tasks 5.3）。

- `Value::Str(VarGcRef)` = **GC 字符串对象**：8B 细指针指变长块，与 Object/Array 同一堆的
  mark/sweep（string 是**不可变叶子**，trace 无出边）。字段存储：string 字段落对象 `refs` 侧表
  （`STRUCT_LEAF_ARCSTRING` → `TAG_STR`），被 `trace_children` / `scan_object_refs` 扫描 →
  string-in-object 正确可达；`is_heap_ref(Str)=true` → 存进堆槽触发写屏障（分代 card / 并发 mark-queue）。
- **驻留/字面量串**：**lazy per-context interning**——加载期**不**物化（无堆），首次 `ConstStr(idx)`
  用活堆分配 GC string + 缓存进 `VmContext.interned_cache`（`(module ptr, idx)` 键），缓存项经
  external root scanner 注册为 **GC root**；后续命中拷 8B 句柄。原 `Module.interned_strings`（加载期
  `Vec<Str>`）+ 其 JIT 镜像 `JitModuleCtx.string_pool` + `build/populate_interned_strings` no-op
  producer **已于 PR-5 删除**（write-only 死代码，运行期全走 `intern_const_str`）。
- **safepoint 安全**：GC 只在显式 safepoint（interp 回边/调用边界）/`ForceCollect` 运行，从不在单条
  指令/builtin 的 Rust 执行中途 → 临时 string（表达式中间值）落寄存器前天然安全，与既有
  Object/Array 临时值同一不变式（分配器 `maybe_auto_collect` 只置标志、延到 safepoint）。
- 代价:纳入 GC → 多点 GC 压力(换掉 Arc 确定性释放，string-heavy 的 z42c 自编译最敏感);收益:统一一套堆 + 为可移动/压缩/去重铺路。**User 已接受（架构统一优先，非短期性能）**。
- **✅ PR-5 收敛（2026-08-17）**：`ClosureData.fn_name` `String` → GC `Str`（8B），闭包块随之全 POD
  → 删 `var_drop_glue` 的 `BlockType::Closure` 分支（region_var 现仅 `ArrayValue` 需 finalizer）；
  `trace_children`（mark）/ `scan_object_refs`（枚举）两个近重复访问器合并为单一
  `Value::visit_gc_children(for_marking, …)`。
- **不迁移的 `Arc<str>`（事实校正）**：frame 栈帧名/文件名（`VmFrame.func_name`/`file`、
  `Function.frame_meta`）**保留 `Arc<str>`**——它们是**诊断/栈回溯元数据、非 `Value::Str` GC payload**，
  且刻意 `Arc<str>` 以与 JIT `FnEntry` 共享、每次调用 O(1) clone（`perf-frame-name-precompute` 的收益）；
  迁进 GC 堆会**回退**该热路径、增加分配，与本程序意图相反 → **不动**。`ArrayObj.element_type: Arc<str>`
  触及 heap-less/leaked/test 构造点，**延后**（非 payload 闭合所需）。
- (Deferred)小字符串内联优化(SSO)；`ArrayObj.element_type` 迁 GC string / type-id。

### 5.1 Finalizer
不透明 native(FileHandle/Stream)被收集时释放底层资源 → 需 **finalizer 队列**。经典坑(非确定/顺序/resurrection)→ **首选显式 close/dispose,finalizer 仅兜底**。

---

## 6. 移动 / 分代 GC 的 ABI 预留（不锁死非移动）

对象 ABI 现在就为移动/分代留空间(算法细节归未来 GC 设计文档,本文只留 ABI 室):
- **对象头 `gc_word`**:mark/color + age/gen 位;**复制期复用为 forwarding pointer**。
- **精确 GC 是移动前提**(回扣 [safepoint.md §7](safepoint.md)):移动须找到并更新所有 ref;per-slot tag 自描述 + 按 kind 扫 → 可精确 fixup。
- **GcRef 可重定位**(§4)。
- **写屏障 → card table / remembered set**:分代追 old→young,minor GC 不必扫整个 old。z42 已有写屏障(并发模式)→ 复用/扩。
- **pinned 与移动冲突**(回扣 [safepoint.md §4](safepoint.md) InNative):native/FFI 期 pinned 不可移 → pin set 跳过,或 pinned 分配在**非移动 pin 区**。
- **per-generation 不同策略**(目标):young = 复制/evacuate(移动、bump 分配)、old = mark-sweep 或 mark-compact;`gc_word` 的 age/gen 位指明所在代。**具体算法 = 未来 GC 设计文档。**

---

## 7. 内存管理边界（精确"统一"到哪）
- **用户可见堆对象**(普通对象/字符串/缓冲/数组/弱引用/Type/不透明native)→ **全 GC、一个头**。
- **内部元数据 `TypeDesc`** → **不进 GC 堆**,归 **context-arena**([load-context.md](load-context.md) teardown 确定性释放)。Type 这个 **GC 对象引用 TypeDesc** = 一条保留边(`whyRetained` 可见)。
- **不过度统一**:把 TypeDesc 也 GC 化会让类型生命周期被 GC 可达性绑架,破坏 load-context 的确定性卸载 → **不做**。
- `Arc` 收敛到仅"内部共享元数据"(TypeDesc,context-arena 托管);`Box` 留瞬态(stack closure 等)。

---

## 8. 决策记录（2026-06-21）
| # | 决策 |
|---|---|
| 值布局 | `#[repr(C)]`+tag 表+偏移规范化(冻结/版本化);fat enum v1,NaN-box 延后 |
| 对象头 | 统一头 = `gc_word`(mark+age+forwarding) + type/kind;去 ad-hoc `native` |
| 对象种类 | ref-object/字符串(GC)/字节缓冲/ref-array/弱引用/Type/不透明native;按 kind 精确扫 |
| 移动/分代 | **ABI 预留**(gc_word forwarding + GcRef 可重定位 + card table + pin 区 + per-gen 位);实现可 v1 非移动,不锁死 |
| GcRef | `NonNull+epoch`(改名避混);可重定位 fork (a)fixup/(b)句柄 待 benchmark |
| 字符串 | 改 GC 对象;驻留串 context 拥有;finalizer 兜底 |
| 边界 | 用户堆全 GC;TypeDesc 留 context-arena(不 GC 化) |

## 9. 分阶段
1. 固化 Value ABI(`#[repr(C)]`+tag/偏移规范),JIT/AOT 对规范编码。
2. 统一对象头(加 `gc_word`)+ 对象 kind 化,去 `native` 字段;字符串改 GC。
3. GcRef 改名 epoch + 可重定位接口(先 non-moving 实现满足接口)。
4. 写屏障 → card table / remembered set;pin 区。
5. 移动/分代实现(young 复制 / old mark-sweep)——**单独 GC 设计文档**驱动,本 ABI 已就位。

## 10. 交叉引用
- 精确 GC@安全点(另一半契约):[safepoint.md](safepoint.md) · OSR/tier:[tiered-execution.md](tiered-execution.md)
- context-arena / TypeDesc 生命周期 / `whyRetained`:[load-context.md](load-context.md)
- 组件化共享契约:[componentized-runtime.md](componentized-runtime.md) · 诊断:[diagnostics.md](diagnostics.md)
- 当前架构:[vm-architecture.md](vm-architecture.md) · **移动/分代 GC 算法:未来 GC 设计文档**
