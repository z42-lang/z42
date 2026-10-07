# 对象与值表示 ABI（Object & Value ABI）

> 待办：Value ABI 版本化规范、统一对象头（`gc_word`）、GcRef 可重定位、card table、移动/分代 GC 尚未实施（值/对象表示本身已实现）；去掉每对象 `Mutex` 之前的剩余项见 §3「字段单元的内存序」。
>
> 把当前**隐式**的跨引擎值/对象表示固化成**显式、版本化的 ABI**（组件化的"共享契约"本体），并为**移动/分代 GC**预留空间、**统一所有堆对象**（含字符串）到一个对象头。
>
> 精确 GC 的另一半"谁是 ref"在此（与 [safepoint.md](safepoint-design.md) 的"GC map@安全点"互补）；消费方：interp / JIT / AOT 三引擎 + GC + [load-context.md](load-context.md)。

---

## 1. 现状（已成形，但隐式且脆弱）
- **Value = Rust tagged enum**（[metadata/types/value.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/types/value.rs)），`#[repr(C, u8)]` + 显式判别值：`I64=0/F64=1/Bool=2/Char=3`（内联值）、`Str(Str)=4`、`Null=5`、`Array(GcRef<ArrayObj>)=6`、`Object(GcRef<ScriptObject>)=7`、`PinnedView=8`、`FuncRef(Str)=9`、`Closure(VarGcRef)=10`、`Ref=12`、`StackObject=14`、`StackArray=15`、`StructRef=16`、`BoxedStruct(GcRef<ScriptObject>)=17`、`StructRefHeap=18`（11、13 空号）。`{idx, frame_id}` 形的变体都是 8B arena 句柄（瞬态的 3 个见 §2.2）。**`Value` 是 `Copy`（16B POD，无 `Drop` glue）**。
- **ScriptObject** = `{ type_desc: Arc<TypeDesc>, storage: ObjStorage, extras: Option<Box<ObjExtras>> }`（[metadata/types/object.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/types/object.rs)）。`storage` 是单次分配的 `[refs: Value × n_refs][bytes: u8 × n_bytes]` 块（[obj_storage.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/types/obj_storage.rs)）：`bytes` 放基元字段与 8 B 引用字，`refs` 只剩型参字段与内联 struct 的引用叶子，布局见 §3「字段存储布局」；`extras` 装冷字段 `native: NativeData`（WeakRef / Type / LoadContext / Assembly / Monitor 句柄）与泛型实参 `type_args`，两者都空时不分配。
- **GcRef** = 8B 标记指针（低 48 位 `RegionEntry` 地址、高 16 位窄 generation 快照，见 §2.1）。`RegionEntry` = `{ value: Mutex<T>, marked, alive, gen_age, generation, finalizer, … }`：mark 位和代龄在 entry 上，对象本身没有 GC 字；每个对象带一把 `Mutex`；chunk 是 Box-owned、永不重定位 → **当前非移动堆**。
- **JIT 与 interp 共享内存 Value 表示**：JIT 直接 `store tag`+payload 到帧的 Value 寄存器数组，**硬编码 tag 值 + 偏移**。
- 内存管理：`Value` 能到达的堆数据全在 GC 堆（`GcRef`：Object / Array / BoxedStruct；`VarGcRef` 变长块：Str / Closure / 数组元素）；`Arc` 只留给内部元数据（`TypeDesc`、帧名等，§5、§7）；瞬态句柄的 payload 在 per-`VmContext` arena（§2.2）。

**核心问题**：已有一份**事实上的跨引擎 Value ABI**，但它绑死 rustc 对 enum 的布局——隐式、脆弱。**本文 = 把它固化成显式版本化规范。**

---

## 2. 值表示（Value）
- **固化为稳定 ABI**：`#[repr(C)]`（或文档化布局）+ **tag 值表 + payload 偏移规范**，interp/JIT/AOT 对契约编码，不靠 rustc 心情。tag 值（`I64=0`…）已是公开判别值，纳入规范并冻结（变更 = ABI 版本 bump）。
- **保留 fat tagged 值**（tag + payload，~16–24B）作 v1；**NaN-box / tagged-pointer 压缩进 Deferred**（后续优化，复杂度大，现 fat enum 够用）。
- 跨引擎契约：一个 Value slot 的 `{tag 偏移, payload 偏移, 总大小}` 是 ABI 一部分；JIT/AOT 据此 load/store。

### 2.1 引用压到平台指针大小 → `Value` 16B（路 A：标记指针）

> 采用**路 A（标记指针）**，`Value` 为 **16B**：
> - `GcRef`/`WeakGcRef` 是 8B 单标记指针（低 48 位 RegionEntry 地址、高 16 位窄 generation，deref mask）。**保留非移动 region GC**（generation 变窄，ABA 窗口 2^16 已接受，见 §4）。wasm32（usize 32 位）按 `target_pointer_width` cfg-gate 成 `{ptr:NonNull(4B), generation:u32(4B)}` 仍 8B。
> - `Value::Str` 是 8B 细指针 `Str`（[`metadata/vstr.rs`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/vstr.rs)，长度进块头；GC 化见 §5）。
> - `Value::FuncRef` 是 `Str`（8B 细）。每个 payload ≤ 8B → `#[repr(C,u8)]` 给出 tag(1B padded to 8) + 8B = **16B**。由 [`metadata/types/value.rs`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/types/value.rs) 的 `const _: () = assert!(size_of::<Value>()==16)` 编译期锁死；JIT 的 `VALUE_STRIDE` 取 `size_of::<Value>()`（单一真相，[`jit/reg_access.rs`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/jit/reg_access.rs)）。
>
> payload 偏移固定（tag@0、payload@8）。native FFI 的 `Z42Value` 是**独立冻结的 16B ABI struct**（`{tag:u32, reserved:u32, payload:u64}`，[z42-abi](https://github.com/z42-lang/z42/tree/main/src/runtime/crates/z42-abi)），与内部 `Value` enum 表示解耦，marshal 显式转换。

**动机**：CLR/JVM 的对象引用 = **单个平台指针（8B）**。若引用带 `generation:u32`（ABA 防护）对齐成 16B、字符串用 `Arc<str>` 胖指针（ptr8+len8），`Value` 最大 payload = 16B → `Value` 会被钉在 24B。把最大 payload 压到 8B，`Value` 即为 **16B**：每个寄存器 / 数组 boxed 元素 / 对象槽省 33%，全 VM 密度 + cache 收益。

> **前提校正**：主要收益是**内存/cache 密度**，**不是 native 交互**——托管引用（带 generation 的 region 句柄 / Arc）本就不能直接交给 native；FFI 零 marshaling 靠 struct **基元字节打包**（见 [struct-value-semantics.md] D1-a），与引用宽度无关。

**两条路（generation 是障碍；本系统取 A）**：
- **A｜标记指针（改动小）**：x86-64/ARM64 虚拟地址仅 48 位，把窄 generation 塞进指针高 16 位（tagged pointer），deref mask 掉。**保留现有 region + 非移动 GC**，generation 变窄（ABA 窗口需评估）；deref 一次 mask（廉价）+ 与 ARM MTE/PAC、ASAN 交互需注意。参考 V8/JVM compressed oops（甚至到 4B）。
- **B｜移动式 tracing GC（改动大）**：引用永远指活对象、GC 移动统一改写 → 从构造上无悬垂，generation 直接不需要（CLR 模型）。与本 doc §6「移动/分代预留」同向，但最重。

**String 侧**：长度进堆对象头换成细指针 = 8B（CLR/JVM 模型，与 §5「字符串改 GC」合流）；代价 = 取 len 多一次解引用。

**范围**：全 VM 横切（GcRef 句柄模型 + String 表示 + `Value` 布局 pin + JIT 寻址 + `value_layout` 断言）。路 A vs 路 B 的最终取舍需与 §6 移动/分代 GC 的 `gc_word`/forwarding 设计一并评估。

### 2.2 `Value` 是 `Copy` —— 3 个瞬态变体用 arena 句柄

> **动机（实测驱动）**：interp-bound workload（z42c 前端）profile 中，若 `Value` 挂 **`Box` 冷变体**
> （`Ref`/`PinnedView`/`StructRefHeap`）或 `GcRef` 带显式 no-op `Drop`，编译器会把每次 clone 编成
> 「match 判别号 + drop-glue」、把 `Vec<Value>` 析构编成逐元素循环，**无法退化成平凡 memcpy / O(1) 释放**
> （实测 `Value::clone` 是头号 leaf 11.4%、`drop_in_place<Frame>` 6.0%）。堆模型统一后 clone 本已无 refcount
> （`GcRef::clone`=8B memcpy、`Str`=`Copy`），所以要让 `Value` 成为真正的 POD。

**做法**：这 3 个「仅在创建帧的调用栈内存活、创建后不可变」的瞬态变体用 8B
`{ idx:u32, frame_id:u32 }` 句柄，payload 存进 per-`VmContext` 的 **`TransientArena`**
（[`interp/transient_arena.rs`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/interp/transient_arena.rs)）；`GcRef` 无显式
`Drop` 且 `Copy` → **`Value` 派生 `#[derive(Copy)]`**。

- **`TransientArena` 生命周期模型**：与 `StackArena`/`StructArena` 同构——`Vec<TransientSlot>`（`Mutex`
  保护）、`frame_id` staleness 守卫、`push_frame` 戳 `transient_base` / `pop_frame` LIFO `truncate`、
  每次 GC 作 **root 扫描**（`scan_roots`）。interp 与 JIT 共用同一 arena + `push_frame`/`pop_frame`
  base（JIT 经 `struct_ops::frame_id_of` 懒分配帧 id，与既有 `StructRef` 句柄同法）。
- **GC**：arena 是 root → payload 内 GcRef（`Ref` 的 Array/Field 目标、`StructRefHeap` 的 backing 数组）
  恒被标记；故 `Value::visit_gc_children` / `arc_heap::mark_if_unmarked` 对这 3 变体是 **no-op**
  （同 `StructRef`/`StackObject`）——GC mark 热路径无额外工作，无需写屏障（root 每次重扫）。
- **相等 / stringify 退化**：3 变体 `==` 按 `{idx,frame_id}` 句柄相等（同 `StackObject`）；`value_to_str`
  返回通用占位串——照 `StackObject`/`StructRef` 先例（ToString 是 escape sink，这些瞬态句柄永不到达
  用户可见 stringify 路径）。有 `ctx` 的消费点（`deref_ref`/`UnpinPtr`/FFI marshal/FieldGet `.ptr/.len`/
  `StructFieldGet(Set)Prim`）经 `arena.with(idx,frame_id,…)` 读真 payload。
- **native marshal**：`value_to_z42`（无 `ctx`）的 `PinnedView` 防御臂退化为明确错误——编译器路径本就
  先 `FieldGet ptr/len`（经 arena 解析）再传标量，从不把 raw view 交给 marshal。

`size_of::<Value>()==16`（8B 句柄）。纯运行时表示，不涉及 zbc/zpkg 格式。§2.1 把 `Value` 压到 16B，§2.2 让它成为真正的 POD（`Copy` + 无 `Drop` glue）。

### 2.3 `Value::Ref` —— `ref`/`out`/`in` 的运行期表示

用户侧规则见参考手册 [参数修饰符](https://z42-lang.github.io/z42/reference/language/parameter-modifiers.html)。本节讲**为什么这样表示**
以及它带来的约束。

#### 设计约束：引用永远不离开调用栈帧

z42 只保留**参数位**的 `ref` / `out` / `in`，砍掉 ref local / ref return / ref field / `ref struct` /
`scoped` / `ref readonly`。关键判断是：C# `ref` 体系约 80% 的复杂度来自**让引用离开调用栈帧**的那些扩展，
而那正是 lifetime 系统、`scoped` 默认规则、`ref struct` 传染性约束的唯一来源。把这些位置一律砍掉，
整条复杂度链塌缩为一条结构性不变式——**一个 `Value::Ref` 的存活期不超过创建它的那次调用**。

这条不变式直接支撑了 §2.2 的瞬态 arena 表示：`Value::Ref` 能做成 `{idx, frame_id}` 句柄、payload 放
per-`VmContext` 的 `TransientArena`、随帧 LIFO 截断释放，**前提就是它不会逃出栈帧**。若将来引入 ref
local / ref return，这套表示要连同 §2.2 一起重做。

#### 表示

- `Value::Ref { idx: u32, frame_id: u32 } = 12`（[`metadata/types/value.rs`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/types/value.rs)），
  8B 句柄指向 `TransientArena` 中的 `RefKind` payload。
- `RefKind` 三变体（[`metadata/types/value_aux.rs`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/types/value_aux.rs)）：
  `Stack { frame_idx, slot }` / `Array { gc_ref, idx }` / `Field { gc_ref, field_name }`。
- **GC 协调**：arena 本身是 root，`Array` / `Field` 里的 `GcRef` 因此恒被扫到，底层数组/对象在调用期间存活；
  `Stack` 不持 `GcRef`（帧在调用栈上自然存活）。故 `Value::visit_gc_children` 对 `Ref` 是 no-op。

#### 入口 copy-in / 出口 copy-out（Decision R2：sidecar 方案）

1. **Codegen** 在 ref/out/in 实参展开处发地址加载指令，结果即 `Value::Ref`。
2. **入口 copy-in**：`exec_function_body` 在 callee Frame 建好后扫参数寄存器，持 `Value::Ref` 的被
   deref 成底层值，原 `RefKind` 存进 `frame.ref_writebacks` sidecar。**callee 体内每条指令读到的都是普通
   `Value`** → 80+ 个指令 handler 一个都不用改。
3. **出口 copy-out**：每条 return / throw 路径前调 `run_ref_writebacks`（`interp/exec_support.rs`），把参数
   寄存器的**终值**按 `RefKind` 写回 caller 的 lvalue。
4. **嵌套透传**自然成立：`Outer(ref x) { Inner(ref x) }` 中 Outer 的 `x` 已是底层值，对 `Inner(ref x)` 发的
   地址加载指向 Outer 自己的槽；Inner 写 Outer 槽，Outer 出口再写回原 caller，两步 indirection。

**为什么选 sidecar 而不是"在 `frame.get/set` 内部 deref"**：后者要改 80+ 个指令 handler 的调用点（每个都得
传 ctx）；sidecar 只在帧入口/出口工作。语义等价——用户观察不到调用中途状态（与 C# 一致）；开销是 1 次 Vec
分配 + n 次 deref/store-through（n = ref 参数个数）。

#### 现状缺口（与上面的模型不符的部分）

- **三个修饰符在 AST 上塌成一个布尔**：`MemberParser` 对 `ref`/`out`/`in` 一律置 `Param.IsRef`
  （`src/compiler/z42c.syntax/src/Decl.z42`），调用点同理塌成 `RefArgExpr`。于是 `out` 定值分析、`in` 写保护、
  修饰符参与重载**全都无处可挂**，也确实都没实现。
- **`callee 的 IR 看不见 `ref``**：`Param.IsRef` 只影响 **caller** 侧发不发地址加载指令，callee 寄存器类型不变。
  逃逸分析因此没有任何指令可以认出"写回汇点"，只能保守地把**被函数体重新定义过的参数槽一律标逃逸**——
  详见 [escape-analysis.md](escape-analysis.md)。
- **只有 `LoadLocalAddrInstr` 真正落地**：`z42.package` 里没有 `LoadElemAddrInstr` / `LoadFieldAddrInstr`，
  `ExprEmitter` 对任何 `BoundRefArg` 都先把 inner 发射成一个寄存器再取该寄存器的地址。于是 `ref arr[i]` /
  `ref obj.field` 编译通过但写回落在临时槽上——**写入静默丢失**。`RefKind::Array` /
  `Field` 两个变体目前在 z42c 产物里没有生产者。

#### 延后形态（设计期主动决定不引入）

| # | 形态 | 延后理由 | 重启触发 |
|---|---|---|---|
| D1 | `ref` 局部变量 `ref int x = ref expr` | 主要服务于值类型原地修改；原语数组场景收益薄 | user struct 落地；profiling 显示原语数组热路径需要 |
| D2 | `ref` 返回 `ref T M()` | 没有 D1 配套则 caller 接不住；escape analysis 成本高（需 ref-safe-context） | D1 落地后 |
| D3 | `ref` 字段 | 绑定 D4，独立无意义 | D4 落地（即"永不"） |
| D4 | `ref struct` 类型 | 传染性约束（不能装箱 / 不进泛型容器）会把类型系统劈成两半；GC 语言里 slice 用 GC 对象表达性能可接受 | 极端零分配 buffer 场景 profiling 证明 GC slice 不可接受 |
| D5 | `scoped` | 是 C# 在缺 lifetime 下为 `ref struct`/`ref return` 打的补丁；z42 砍掉那些位置后自然不需要 | D1/D2/D4 任一落地（推荐永不） |
| D6 | `ref readonly`（任何位置） | 参数位由 `in` 顶替（修正 C# `in`/`ref readonly` 双形态冗余）；其他位置在没有 `mut` 体系时 holder-side"我不写"承诺意义弱 | 推荐永不 |

D1 / D2 若真要重启，简化预案是：ref local 永远块作用域（不可 return / 不可存字段 / 不可被 lambda 捕获）；
ref return 只允许三种 lvalue（参数 / 引用类型字段 / 数组元素），用结构性 escape check 代替 lifetime 标注。

相关决策：`mut` 修饰符**永不引入**（`in` 是 callee 端 API 契约，不是 caller 端可变性标注，与 mut 体系正交）；
生命周期标注**永不引入**（靠砍掉栈帧外的 ref 位置天然规避）。

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
| 普通 ref 对象（用户类） | 字段单元（见下「字段存储布局」） | 侧表 `Value` + 每个非零引用字 |
| **字符串（改 GC，§5）** | len + UTF-8 字节 | 无内部 ref，跳过 |
| 字节/原始缓冲 | 原始字节 | 无内部 ref，跳过 |
| ref 数组 | element_type + 元素 Value[] | 扫元素 |
| 弱引用对象 | weak handle | **不 trace target** |
| Type 对象（反射） | 引用 TypeDesc | 该引用是保留边（§7 边界） |
| **不透明 native（未来 Stream/FileHandle）** | 原始 native ptr + **finalizer** | 无 ref；收集时跑 finalizer（§5.1） |

→ `ScriptObject.native: NativeData` ad-hoc 字段**消除**；`WeakRef`/`TypeHandle`/未来 `FileHandle` 变成上述 kind。

### 字段存储布局（= 对象内存布局本体，跨引擎 ABI）

实例字段按**字节布局**存放在 `ScriptObject.storage`，布局由加载期组合出的 `ObjectLayout`
（[metadata/types/layout.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/types/layout.rs) 的 `compose_object_layout`）决定，`alloc` 时定长。
编译器的对象布局给每个引用字段留 8 B、每个基元字段按自身宽度对齐（`StructLayout._alignOf`），运行时照单使用，**不改字节码格式**。
每个直接字段落在一种**单元**里（`FieldAccess::cell()` → `FieldCell`）：

| 单元 | 哪些字段 | 位置与宽度 | 读 / 写 |
|---|---|---|---|
| 基元 `Prim` | `int` / `long` / `double` / `bool` / `char` … | `bytes` 的组合偏移，声明宽度 | 同宽 relaxed 原子 |
| 引用字 `Ref` | `string`、类、数组、接口、`object`、委托 | `bytes` 的组合偏移，8 B（`ObjectLayout.ref_cells`） | release 写 / acquire 读 |
| 侧表 `Value` | 型参字段（`T F;`、`T? F;`） | `refs[ref_slot]`，16 B `Value` | 持对象锁读写 |
| struct 根 `Struct` | 内联值 struct 字段 | 叶子摊在 `bytes`（基元）与 `refs`（引用） | `StructFieldGetPrim` / `SetPrim` |

**引用字**（[metadata/types/ref_word.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/types/ref_word.rs)）是自描述的：
低 3 位记种类，其余位是句柄原始位（`GcRef::to_tagged_bits` / `VarGcRef::to_bits`，`RegionEntry` 与变长块头都 8 字节对齐，低 3 位恒为 0），
整字为 0 = `null`（零初始化即正确的默认值）。

| 种类 | `Value` | 句柄 |
|---|---|---|
| 0（整字 0） | `Null` | — |
| 1 / 6 | `Object` / `BoxedStruct` | `GcRef<ScriptObject>` |
| 2 | `Array` | `GcRef<ArrayObj>` |
| 3 / 5 | `Str` / `FuncRef` | `VarGcRef`（GC 字符串块） |
| 4 | `Closure` | `VarGcRef`（闭包块） |
| 7 | 装箱的任意 `Value` | `GcRef<ArrayObj>`，单元素只读数组 |

- **种类 7 是逃生口**：`object` / 接口字段能经擦除泛型收到裸 `I64` / `F64` / `Bool` / `Char`（`void Set<T>(H h, T t) { h.O = t; }` 以 `T = int` 调用），
  这些值装不进 8 B。写入时装进单元素数组，读出时取回原值——对用户代码透明。z42c 工作区整套构建一次也不走这条路。
  写屏障拿到的是盒子（`try_set_field_value` 返回 `FieldWrite::Boxed`，调用方经 `with_barrier_value` 发屏障），GC 追踪经 `decode_for_trace` 也看到盒子。
- **型参字段留在侧表**：它们的值在擦除下可以是任何 `Value`，`List<int>` 节点之类每次写都装箱不可接受。
  判据在 `compose_object_layout`：编译器种类为 `GCREF_CLOSURE`、声明类型是本类的型参名（含 `T?`）。
  闭合实例化（`Box<int>` 的描述符，见 [compiler/generics.md](../compiler/generics.md)）的字段类型已代换，`int` 落基元单元、`string` 落引用字。
- **内联 struct 的引用叶子留在侧表**：struct blob（帧 arena、装箱 struct、`struct[]` 元素）的引用叶子还是 16 B，对象里内联的 struct 与独立 blob 逐字节同布局，
  整块拷进拷出（`snapshot_box`、反射）不做转换；数组改 8 B 单元时一起改。
- **合成布局**（没有编译器对象块的类型：Rust 构造的类型、单测）保守地把每个引用放侧表。
- 名→字段下标由 `TypeDesc.field_index`（类级共享）。**继承：基类字段在前、子类追加**，子类区从 `align_up(base.size, 8)` 开始，
  基类部分的单元归属原样继承。
- 访问 `obj.f` = 按字段下标查 `ObjectLayout.field_access[i]`（`{offset, width, tag, ref_slot}`，加载期算好），按单元读写
  （`metadata/types/object_fields.rs`：`field_value` / `try_set_field_value`）。`Value` ↔ 单元的转换只在这里与 `ref_word`；
  两个引擎经 [objops](interp-jit-semantics.md#对象操作objops) 调到它们。JIT 对基元字段的读写、对引用字的读直接按字节偏移生成原生
  load/store（见 [jit.md](jit.md)）→ **字段偏移、宽度、引用字编码与 Value 大小是 ABI 一部分，须固化**。
- GC 只经 `ScriptObject::visit_refs`（侧表 + 每个非零引用字）遍历一个对象的出边，标记、栈对象根扫描、保留图共用；
  清扫断边用 `clear_refs_for_sweep`。

### 字段单元的内存序

堆上每个可变单元 ≤ 8 B、用同宽原子访问，这是为去掉每对象 `Mutex` 铺路：16 B `Value` 只留在寄存器，
堆上不会出现「写到一半」的单元，任何时刻读到的都是某次完整写入的值。

- **基元**：relaxed。x86-64 / AArch64 上就是普通对齐 load / store，JIT 内联码直接用普通 load / store。
- **引用字**：release 写、acquire 读（AArch64 上 `stlr` / `ldar`，JIT 内联读用 Cranelift `atomic_load`）。
  于是「在一个线程里构造对象、经字段交给另一线程」是安全的：读到引用的线程一定看得到构造期写入的字段。
- **SATB 删除屏障**要的是被覆盖的真旧值：major 标记进行中（`satb::marking_any()`），引用字的写是 `swap`，
  旧字交给 `record_overwrite`；不在标记期就是普通 release store。这样屏障不依赖对象锁，并发写也不会漏记。
- **跨字段没有一致性**：两个字段之间、读改写之间都没有原子性，用户侧需要组合一致时用 `Mutex<T>` / `Channel<T>`
  （参考手册 threading 页「线程之间怎么传值」写了这份契约）。

现状与后续（对象模型 R1 之后）：

- 每对象 `Mutex` 还在（`GcRef::borrow` / `borrow_mut`），解释器与 JIT helper 的字段读写仍持锁；JIT 内联快路本来就不持锁。
  去掉它（R4）之前还要处理的单元：型参字段与内联 struct 引用叶子的 16 B 侧表（R2：型参字段的标签在分配时按实例化写死、之后只原子改 8 B 负载；
  struct 叶子随数组改造）、`bytes()` / `refs()` 的切片视图（整块拷贝要改逐单元原子拷贝，写原语改为 `&self`）、`try_set_field_value` 里的 `BoxedStruct` 拆箱读。
- 数组元素仍是 16 B `Value` 或打包基元，由数组锁保护（R3：REF / PRIM / MIXED 三种模式）。

### 槽位零初始化

**不变式：值类型的存储槽永不含 `Value::Null`。** 值类型必须在**分配点**一次解决，而不是在读取侧打补丁——若存储初始化不看声明类型
（`vec![Value::Null; n]`），会长出一族内部错误
（`__box_prim: expected integer value, got Null` / `type mismatch in arithmetic: Null vs I64(1)`）。

口径：`alloc_object` 不逐字段填默认值，而是按
`TypeDesc::object_storage()` composed layout 分配**整块零字节区 + `Null` 引用区**
（`gc/arc_heap/interface.rs`）。于是：

| 槽的种类 | 零值 | 为什么对 |
|---|---|---|
| 基元值字段（int/bool/char/double/long…） | 字节零 ⇒ `0` / `false` / `'\0'` / `0.0` | 值落在 bytes 区 |
| 引用字段 | 引用字为 0（侧表单元为 `Value::Null`） | `null` 本就是引用类型的零值；整字 0 就是 `null` 的编码 |
| 数组元素 | `default_value_for_tag(elem_tag)` | `ArrayNew`（interp + JIT 两份）按元素 tag 取；`ArrayObj::typed_filled` 直接写进 GC 块（是该 backing 的零就不逐元素写）|
| **型参字段**（`class GBox<T> { T V; }`） | 按**实例化**取：`default_value_for(type_args[i])` | 见下 |

**型参字段要单独一条**，因为布局是**按声明**算的：声明里 `T` 不是基元 ⇒ 该槽被分类成
**引用槽** ⇒ 整块零初始化给它的零值是 `Null`，而不是 `GBox<int>` 该有的 `0`。
实例自己带着实参（`ObjNew` 写入 `set_type_args`），所以真正的零值在**分配点**可以还原：
把字段的 `type_tag` 按名字映射到 `TypeDesc::type_params()` 的下标，再取该实参的零值
（`metadata/types/field.rs::generic_field_zero_overrides`，interp 的堆/栈两支 + JIT 三处共用）。

口径**刻意窄**，与 `ArrayNew` 同一条线：**只有基元值实参**才改写。解析出的 *struct* 实参
不能在这里强推 struct backing（泛型容器按引用存 struct，会炸
`struct_generic_container: VCall: expected object, got StructRefHeap`）；引用实参的零值
本来就是 `Null`，无事可做。

编译期那一侧配套堵住「写 null 进值类型槽」（E0475 / E0476 / E0483），
`object` → 值类型的**拆箱**则按两段报错，见下。

> 泛型实例化单调化覆盖了继承与泛型 struct 两种形态：
> `class D : GBox<int> {}` 的继承字段、`struct GS<T> { T F; }` 的型参字段，
> **存储零值都对**（interp / jit 一致，钉在 `src/tests/types/value_field_zero/`）。
> ⇒ 不变式「值类型的存储槽永不含 `Value::Null`」**全域成立**。
>
> ⚠️ 但**存储**对了不等于**编译期类型**也代换了：继承来的型参字段在编译期仍被当成 `T`，
> `d.V + 1` / `if (d.V)` 各报 E0402（泛型 struct 那格已代换，两者不是同一条路径）。
> 那是编译期代换的缺口，见 [compiler/generics.md](../compiler/generics.md)。

### `object` → 值类型的拆箱：两段检查

拆箱失败分**两种**错，各报各的（用例
`src/tests/types/hard_cast_value/`，interp + JIT 行为一致）：

| 情形 | 异常 | 消息 |
|---|---|---|
| 收者是 null | `NullReferenceException` | ``cannot unbox null to `int` `` |
| 收者类型不符 | `InvalidCastException` | ``cannot cast string to `int` `` |

**顺序不能反**：null 没有类型，先查类型会得到一条误导的消息。两者都是**用户级可 `catch`
的真异常**，不是内部 `bail!`（内部错误 `catch (Exception)` 抓不到）。

### 反方向：装箱点收到 `Null` = 不变式被破

`__box_prim` 收到 `Value::Null` 时 **debug 报错、release 放行为 `null`**
。

既然上面那条不变式成立，用户代码就**没有任何合法写法**能把 `Null` 送到装箱点
（E0475 / E0476 / E0483 在编译期堵住）⇒ 走到那里只可能是 **VM / 编译器缺陷**。

| 档 | 行为 | 为什么 |
|---|---|---|
| debug | `bail!`，消息指明「不变式被破、去查这个值从哪个槽读出来的」 | **e2e golden 语料默认跑 debug VM**（`_activeVm(root, "debug")`）⇒ 整个语料成为这条不变式的探测器，缺陷炸在**发生点** |
| release | 原样返 `null` | 语料零命中只说明现有语料没踩到，不等于不存在；不拿用户的崩溃换诊断能力 |

⚠️ **这里不能报用户级异常**（与拆箱那侧的关键差别）：拆箱是**用户写的**转换，所以抛
`NullReferenceException` / `InvalidCastException` 是对的；装箱点的 `Null` 不是用户的错，
报用户级异常会把责任指向错误的一方。

---

## 4. GcRef 语义
- 现:8B 标记指针 = `RegionEntry` 地址 + 高 16 位窄 generation(ABA 防护，§2.1)。**改名 `generation`→`epoch`**:避免与**分代 GC 的 young/old generation** 混淆。
- **必须"可重定位"**（为移动 GC，§6）。两方案(fork,待 benchmark):
  - **(a) 稳定 entry + 重定位 payload + 精确 fixup**:GC 把所有 GcRef 改写到新址(evacuation+fixup)。访问无额外间接;移动时全堆 fixup。
  - **(b) 句柄表间接**:GcRef→表→对象;移动只改表一格,访问多一跳。
  - young 复制式偏好 (a)+bump 分配。**fork 留文档,实现期 benchmark 定。**
- 访问含 `epoch` 校验(use-after-free 安全);JIT 可在可证明安全处 elide。

---

## 5. 字符串是 GC 对象

> `Value::Str` 的字节在单一 GC 堆内。`Str`（[`metadata/vstr.rs`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/vstr.rs)）是 **8B `VarGcRef`**（`gc/var_region.rs` 的变长块，`BlockType::Str`，`{GcBlockHeader,
> inline UTF-8}` 单次分配）——无 refcount，GC 管生死（mark/sweep）。分配走 **ambient 堆**
> （`gc/ambient.rs`，引擎入口的 `HeapGuard` 设 thread-local，`Str::new`/`.into()` 无需显式堆参数）；无堆上下文
> （无 VM 的单测）回退 leaked 块。变长 payload 全部在 GC 堆内，统一堆模型闭合。

- `Value::Str(VarGcRef)` = **GC 字符串对象**：8B 细指针指变长块，与 Object/Array 同一堆的
  mark/sweep（string 是**不可变叶子**，trace 无出边）。字段存储：string 字段落对象 `refs` 侧表
  （`STRUCT_LEAF_ARCSTRING` → `TAG_STR`），被 `trace_children` / `scan_object_refs` 扫描 →
  string-in-object 正确可达；`is_heap_ref(Str)=true` → 存进堆槽触发写屏障（分代 card）。
- **驻留/字面量串**：**lazy per-context interning**——加载期**不**物化（无堆），首次 `ConstStr(idx)`
  用活堆分配 GC string + 缓存进 `VmContext.interned_cache`（`(module ptr, idx)` 键），缓存项经
  external root scanner 注册为 **GC root**；后续命中拷 8B 句柄（运行期全走 `intern_const_str`）。
- **safepoint 安全**：GC 只在显式 safepoint（interp 回边/调用边界）/`ForceCollect` 运行，从不在单条
  指令/builtin 的 Rust 执行中途 → 临时 string（表达式中间值）落寄存器前天然安全，与既有
  Object/Array 临时值同一不变式（分配器 `maybe_auto_collect` 只置标志、延到 safepoint）。
- 代价:纳入 GC → 多点 GC 压力(换掉 Arc 确定性释放，string-heavy 的 z42c 自编译最敏感);收益:统一一套堆 + 为可移动/压缩/去重铺路。架构统一优先于短期性能。
- **闭包与访问器**：`ClosureData.fn_name` 是 GC `Str`（8B），闭包块全 POD（region_var 仅 `ArrayValue` 需 finalizer）；
  这个名字串按 `MkClos` 站点在每个 `VmContext` 里驻留一次（`VmContext::intern_fn_name`，复用 `interned_cache` 这个 GC 根，
  命中按内容复核），循环里建闭包不再每次新分配一个名字块；闭包改为在创建时绑定函数 id 之后，这层驻留随之删除；
  mark 与枚举共用单一访问器 `Value::visit_gc_children(for_marking, …)`。
- **不迁移的 `Arc<str>`**：栈帧名/文件名（`Function.frame_meta`）**保留 `Arc<str>`**——它们是
  **诊断/栈回溯元数据、非 `Value::Str` GC payload**，加载时算一次，生成栈回溯时 O(1) clone
  （`VmFrame` 只存 `*const Function`，调用路径不碰它们）。
- **数组元素类型名 = 驻留句柄**：`ArrayObj.element_type: ElemType`
  （[`metadata/types/elem_type.rs`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/types/elem_type.rs)）
  是指向进程级驻留条目 `ElemTypeInfo{name: Arc<str>, kind: ElemKind}` 的 **8 B 细指针**（`Copy`，
  `Deref<Target=str>`）。条目按名字驻留一次、永不释放（元素类型名是已加载代码里的有限集），所以建数组
  不分配字符串、不做原子计数；条目缓存的 `ElemKind` 直接决定 backing。驻留发生在**指令解码**
  （`ArrayNewInsn` / `ArrayNewLitInsn` 的 `element_type` 已是句柄）：interp 直接传句柄，JIT 把
  `as_raw()` 作为指针常量烤进代码、helper 用 `from_raw` 还原。其余 Rust 侧构造点（`Array.CreateInstance`、
  `alloc_array_typed`）走 `ElemType::intern`（读锁 + FxHash）。需要 `Arc<str>` 的旁路（struct[] 元素拷出
  到 arena）用 `arc()`，只 +1 引用计数。配合 `StructBytes` 去掉冗余的 `elem_size`（= `layout.size`），
  `ArrayObj` 48 B、`RegionEntry<ArrayObj>` 88 B。
- (Deferred)小字符串内联优化(SSO)。

### 5.1 Finalizer
不透明 native(FileHandle/Stream)被收集时释放底层资源 → 需 **finalizer 队列**。经典坑(非确定/顺序/resurrection)→ **首选显式 close/dispose,finalizer 仅兜底**。

---

## 6. 移动 / 分代 GC 的 ABI 预留（不锁死非移动）

对象 ABI 现在就为移动/分代留空间(算法细节归未来 GC 设计文档,本文只留 ABI 室):
- **对象头 `gc_word`**:mark/color + age/gen 位;**复制期复用为 forwarding pointer**。
- **精确 GC 是移动前提**(回扣 [safepoint.md §7](safepoint-design.md)):移动须找到并更新所有 ref;per-slot tag 自描述 + 按 kind 扫 → 可精确 fixup。
- **GcRef 可重定位**(§4)。
- **写屏障 → card table / remembered set**:分代追 old→young,minor GC 不必扫整个 old。z42 已有写屏障(分代 card table)→ 复用/扩。
- **pinned 与移动冲突**(回扣 [safepoint.md §4](safepoint-design.md) InNative):native/FFI 期 pinned 不可移 → pin set 跳过,或 pinned 分配在**非移动 pin 区**。
- **per-generation 不同策略**(目标):young = 复制/evacuate(移动、bump 分配)、old = mark-sweep 或 mark-compact;`gc_word` 的 age/gen 位指明所在代。**具体算法 = 未来 GC 设计文档。**

---

## 7. 内存管理边界（精确"统一"到哪）
- **用户可见堆对象**(普通对象/字符串/缓冲/数组/弱引用/Type/不透明native)→ **全 GC、一个头**。
- **内部元数据 `TypeDesc`** → **不进 GC 堆**,归 **context-arena**([load-context.md](load-context.md) teardown 确定性释放)。Type 这个 **GC 对象引用 TypeDesc** = 一条保留边(`whyRetained` 可见)。
- **不过度统一**:把 TypeDesc 也 GC 化会让类型生命周期被 GC 可达性绑架,破坏 load-context 的确定性卸载 → **不做**。
- `Arc` 收敛到仅"内部共享元数据"(TypeDesc,context-arena 托管);瞬态 payload 留在 per-`VmContext` arena(§2.2)。

---

## 8. 决策记录
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
2. 统一对象头(加 `gc_word`)+ 对象 kind 化,去 `native` 字段(字符串已是 GC 对象,§5)。
3. GcRef 改名 epoch + 可重定位接口(先 non-moving 实现满足接口)。
4. 写屏障 → card table / remembered set;pin 区。
5. 移动/分代实现(young 复制 / old mark-sweep)——**单独 GC 设计文档**驱动,本 ABI 已就位。

## 10. 交叉引用
- 精确 GC@安全点(另一半契约):[safepoint.md](safepoint-design.md) · OSR/tier:[tiered-execution.md](tiered-execution.md)
- context-arena / TypeDesc 生命周期 / `whyRetained`:[load-context.md](load-context.md)
- 组件化共享契约:[componentized-runtime.md](componentized-runtime.md) · 诊断:[diagnostics.md](diagnostics.md)
- 当前架构:[vm-architecture.md](vm-architecture.md) · **移动/分代 GC 算法:未来 GC 设计文档**
