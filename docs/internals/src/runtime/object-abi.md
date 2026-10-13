# 对象与值表示 ABI（Object & Value ABI）

> 待办：Value ABI 版本化规范、GcRef 可重定位、card table、移动/分代 GC 尚未实施（值/对象表示本身已实现，对象已是「GC 字 + 类型字」一个 16 B 头）；对象模型的剩余项见 §3「字段单元的内存序」。
>
> 把当前**隐式**的跨引擎值/对象表示固化成**显式、版本化的 ABI**（组件化的"共享契约"本体），并为**移动/分代 GC**预留空间、**统一所有堆对象**（含字符串）到一个对象头。
>
> 精确 GC 的另一半"谁是 ref"在此（与 [safepoint.md](safepoint-design.md) 的"GC map@安全点"互补）；消费方：interp / JIT / AOT 三引擎 + GC + [load-context.md](load-context.md)。

---

## 1. 现状（已成形，但隐式且脆弱）
- **Value = Rust tagged enum**（[metadata/types/value.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/types/value.rs)），`#[repr(C, u8)]` + 显式判别值：`I64=0/F64=1/Bool=2/Char=3`（内联值）、`Str(Str)=4`、`Null=5`、`Array(GcRef<ArrayObj>)=6`、`Object(ObjRef)=7`、`PinnedView=8`、`FuncRef(FnId)=9`、`Closure(VarGcRef)=10`、`Ref=12`、`StackObject=14`、`StackArray=15`、`StructRef=16`、`BoxedStruct(ObjRef)=17`、`StructRefHeap=18`（11、13 空号）。`{idx, frame_id}` 形的变体都是 8B arena 句柄（瞬态的 3 个见 §2.2）。**`Value` 是 `Copy`（16B POD，无 `Drop` glue）**。
- **对象** = 变长 GC 区（`region_var`）里的**一个** `BlockType::Object` 块，一次分配、**一个 16 B 头**（对象模型 R6 / R8）：

  ```
  GcWord (8 B) │ type_desc (8 B) │ fields: [UnsafeCell<u8>] … │ [meta (8 B)]
  └── 块头 = 对象头（16 B）─────┘ └── payload：块头 size 字节 ────────────┘
  ↑ ObjRef::as_ptr（*const ObjHeader，细指针，= 块头地址）
  └──────────── &ScriptObject（胖引用：地址 + payload 长度）──────────────────┘
  ```

  块头（`GcBlockHeader`，[gc/var_region/block.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/gc/var_region/block.rs)）对所有变长块同形：
  前 8 字节是 GC 字（`GcWord`：16 位 generation，最低位即存活位 / mark / `type_tag` / 32 位 payload 长度），后 8 字节是「种类字」——
  对象的 `Arc<TypeDesc>`，其余块恒为空。所以对象头**就是**块头，类型字之后直接是字段单元（布局与取舍见 [gc.md](gc.md)「变长块堆」）。
  `ScriptObject`（[metadata/types/object.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/types/object.rs)）是一个 `#[repr(C)]` **动态大小类型**：
  `{ gc: GcWord, type_desc, payload: [UnsafeCell<u8>] }`，盖在整个块上；`&ScriptObject` 是胖引用，元数据是 payload 长度，
  **引用的范围恰好盖住头 + 字段 + meta 字**。`payload_len()`（字段区长度）= payload 长度减去 meta 字（有 meta 时 8 B，看块头 `type_tag`
  bit 6，`OBJ_META_BIT`），`cells()` 只覆盖字段区——每次字段访问都在引用自己的范围里，从不从引用走到它外面的内存
  （Stacked Borrows 的 retag 覆盖 `size_of_val`；Miri 核过）。胖引用只在持有整块 provenance 的指针上构造（`ScriptObject::from_block`：
  `ObjRef::borrow`、`OwnedObject`、分配器、drop glue），长度取自块头的 `size`——它是唯一的长度来源
  （装箱基元按标量宽度、装箱 struct 按 blob、类实例按 `TypeDesc::object_bytes_len`，再加 meta 字）。头的定长视图是 `ObjHeader`
  （同样 `#[repr(C)]` 的 `{gc, type_desc}`，16 B，`offset_of!(type_desc)` 与块头种类字的偏移由常量断言钉住），分配器经
  `ObjHeader::install` 只写类型字（不按值构造任何一个），GC 字由变长区写。`payload` 是 `UnsafeCell`：字段经 `&self` 原子写。
  字段区是零初始化的：基元字段、8 B 引用字（含内联 struct 的引用叶子）与 16 B 型参单元都在里面，没有 16 B `Value` 侧表，布局见 §3「字段存储布局」。
  热路径的结构与 ABI 只放细句柄（`ObjRef`、`ObjRef::as_ptr` 的头地址），不放 `&ScriptObject`；只看类型的路径（虚调用 IC、类型检查）经
  `ObjRef::header()` 拿细的 `&ObjHeader`，不读 payload 长度。
  堆外的对象（栈分配对象、堆外构造的对象）是同样形状的**脱离块**（`OwnedObject`，[owned_object.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/types/owned_object.rs)：
  一个不属于任何 region 的 `{头, 字段, [meta]}`），所以 `&ScriptObject` 不论对象在哪都一样读。
  典型尺寸（头 + 字段，按 size class 取整：16 / 24，再往上每八度 4 档）：空类 16 B、`{int}` / `{long}` 24 B、`{int; string}` 32 B；
  泛型实例与带原生底座的对象再多一个 8 B meta 字。
  `meta` 是一个 8 B 字，**只有泛型实例与带原生底座的对象才有**（在 payload 末尾、字段区之后，可能不按 8 对齐，整字非对齐读写；
  分配点就知道要不要：`alloc_instance` 看实参是否为空，`OwnedObject` 在构造期补实参 / 原生底座时把块加长一个字），装两样冷数据：
  - 泛型实参 `type_args`：按实例化**进程级驻留**的 `TypeArgs`（[type_args.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/types/type_args.rs)，同 `ElemType` 的套路：8 B 细指针、同内容同址、永不释放）。同一实例化的所有实例指向同一个条目，每个对象不为实参分配任何东西。驻留发生在解码 / 注册期：`ObjNewInsn::type_args` 解码时驻留，实例化描述符（`Demo.Box<int>`）名字里解析出的实参在 `build_type_registry` 驻留进 `TypeDescCold::type_args`；`ObjNew`（interp 堆 / 栈两支、JIT `jit_obj_new`）只拷句柄：两引擎共用 `interp/obj_new_resolve.rs::instance_type_args` 挑出实参（站点自己的，否则描述符的），再经 `ScriptObject::install_type_args` 一处写入并改写型参字段零值。进程级而非 VM 级，因为包惰性加载、多个 VM 可并存。
  - 原生底座 `native: NativeData`（WeakRef / Type / LoadContext / Assembly / Monitor 句柄）：只有这类对象才分配一个 `ObjExtras` 盒子；带原生底座的泛型对象（如 `Mutex<T>`）把实参句柄也放进盒子。
  - 编码：`0` = 非泛型且无原生底座（绝大多数对象）；偶指针 = 驻留的 `TypeArgs` 条目；奇指针（低位标签）= `Box<ObjExtras>`。
  - 读者（`DefaultOf`、`__class_type_arg` / `__class_default`、`GetType().GetGenericArguments()`、数组 `T` 零值、型参字段零值）拿到的是 `&'static [String]`。擦除代码里的 `new Box<T>()` 以开放实例化名 `Box<T>` 分配，实参是字面 `["T"]`，同样驻留成一个共享条目。
  - 已知边界：擦除分配的实例只知道 `T`，于是 `GetGenericArguments()` 报 `T`、`default(T)` 得 `null`、`is Box<int>` 为 `false`。真实实参要从分配时所在方法的泛型上下文（接收者的 `type_args` 或帧的 `method_type_args`）按型参名解析，目前没有做。
- **ObjRef**（对象，[gc/obj_ref.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/gc/obj_ref.rs)）= 对象块的 `VarGcRef`：8B 标记指针（低 48 位块头地址、高 16 位 generation 快照——头里的 generation 本身就是 16 位，见 §2.1）。
  mark 位、代龄、generation（含存活位）都在头的 GC 字上；`ObjRef::borrow()` 比一次 generation（活着的块 generation 为奇数，句柄存的就是它，所以相等同时说明「活着」且「没被复用」）后直接给 `&ScriptObject`，**没有每对象锁**（见 §3「字段单元的内存序」）。
  弱引用 `WeakObjRef` 额外记下所在 chunk 的复用 epoch（见 [gc.md](gc.md)「GC heap backing」）。
- **GcRef**（数组头）= 8B 标记指针（低 48 位 `RegionEntry` 地址 + generation 快照）。`RegionEntry` = `{ value: T, marked, alive, gen_age, generation, finalizer, location, soft_ref_count }`；数组头槽 64 B。
  两种 chunk 都永不重定位 → **当前非移动堆**。
- **JIT 与 interp 共享内存 Value 表示**：JIT 直接 `store tag`+payload 到帧的 Value 寄存器数组，**硬编码 tag 值 + 偏移**。
- 内存管理：`Value` 能到达的堆数据全在 GC 堆（`GcRef`：Array；`VarGcRef` 变长块：Object / BoxedStruct（`ObjRef`）、Str、Closure、数组元素）；`Arc` 只留给内部元数据（`TypeDesc`、帧名等，§5、§7）；瞬态句柄的 payload 在 per-`VmContext` arena（§2.2）。

**核心问题**：已有一份**事实上的跨引擎 Value ABI**，但它绑死 rustc 对 enum 的布局——隐式、脆弱。**本文 = 把它固化成显式版本化规范。**

---

## 2. 值表示（Value）
- **固化为稳定 ABI**：`#[repr(C)]`（或文档化布局）+ **tag 值表 + payload 偏移规范**，interp/JIT/AOT 对契约编码，不靠 rustc 心情。tag 值（`I64=0`…）已是公开判别值，纳入规范并冻结（变更 = ABI 版本 bump）。
- **保留 fat tagged 值**（tag + payload，~16–24B）作 v1；**NaN-box / tagged-pointer 压缩进 Deferred**（后续优化，复杂度大，现 fat enum 够用）。
- 跨引擎契约：一个 Value slot 的 `{tag 偏移, payload 偏移, 总大小}` 是 ABI 一部分；JIT/AOT 据此 load/store。

### 2.1 引用压到平台指针大小 → `Value` 16B（路 A：标记指针）

> 采用**路 A（标记指针）**，`Value` 为 **16B**：
> - `GcRef`/`WeakGcRef`（数组）、`ObjRef`（对象）、`VarGcRef`（其余变长块）都是 8B 单标记指针（低 48 位 RegionEntry / 块头地址、高 16 位窄 generation，deref mask）。**保留非移动 region GC**（generation 变窄，ABA 窗口 2^16 已接受，见 §4）。wasm32（usize 32 位）按 `target_pointer_width` cfg-gate 成 `{ptr:NonNull(4B), generation:u32(4B)}` 仍 8B。
> - `Value::Str` 是 8B 细指针 `Str`（[`metadata/vstr.rs`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/vstr.rs)，长度进块头；GC 化见 §5）。
> - `Value::FuncRef` 是目标函数的 `FnId`（4B，P1-2 PR 8；此前是 8B 的 `Str` 名字）。每个 payload ≤ 8B → `#[repr(C,u8)]` 给出 tag(1B padded to 8) + 8B = **16B**。由 [`metadata/types/value.rs`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/types/value.rs) 的 `const _: () = assert!(size_of::<Value>()==16)` 编译期锁死；JIT 的 `VALUE_STRIDE` 取 `size_of::<Value>()`（单一真相，[`jit/reg_access.rs`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/jit/reg_access.rs)）。
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
> 注:当前对象头就是这个形状（对象模型 R8）：16 B 块头 = 8 B GC 字（generation + 存活位、mark、`type_tag`（种类 / 代龄 / 记忆集 / 侧表位）、
> payload 长度）+ 8 B 类型字（`Arc<TypeDesc>`）。GC 字目前不能兼作 forwarding pointer（非移动堆），**移动 GC 需要的转发字是 §6 的事**。

### 对象种类
| kind | payload | 精确 GC 扫描 |
|---|---|---|
| 普通 ref 对象（用户类） | 字段单元（见下「字段存储布局」） | 每个非零引用字 + 型参单元标签字里的引用 |
| 装箱 struct | struct blob（引用叶子是引用字） | struct 布局的每个引用叶子 |
| **字符串（改 GC，§5）** | len + UTF-8 字节 | 无内部 ref，跳过 |
| 字节/原始缓冲 | 原始字节 | 无内部 ref，跳过 |
| 数组 | element_type + 一个元素块，模式见下「数组元素」 | REF 引用字 / MIXED 标签字 / `struct[]` 引用叶子 / FROZEN `Value` |
| 弱引用对象 | weak handle | **不 trace target** |
| Type 对象（反射） | 引用 TypeDesc | 该引用是保留边（§7 边界） |
| **不透明 native（未来 Stream/FileHandle）** | 原始 native ptr + **finalizer** | 无 ref；收集时跑 finalizer（§5.1） |

→ `ScriptObject.native: NativeData` ad-hoc 字段**消除**；`WeakRef`/`TypeHandle`/未来 `FileHandle` 变成上述 kind。

### 字段存储布局（= 对象内存布局本体，跨引擎 ABI）

实例字段按**字节布局**存放在对象头之后的字段区（`ScriptObject::cells()`），布局由加载期组合出的 `ObjectLayout`
（[metadata/types/layout.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/types/layout.rs) 的 `compose_object_layout`）决定，`alloc` 时定长。
编译器的对象布局给每个引用字段留 8 B、每个基元字段按自身宽度对齐（`StructLayout._alignOf`），运行时照单使用，**不改字节码格式**。
每个直接字段落在一种**单元**里（`FieldAccess::cell()` → `FieldCell`）：

| 单元 | 哪些字段 | 位置与宽度 | 读 / 写 |
|---|---|---|---|
| 基元 `Prim` | `int` / `long` / `double` / `bool` / `char` … | `bytes` 的组合偏移，声明宽度 | 同宽 relaxed 原子 |
| 引用字 `Ref` | `string`、类、数组、接口、`object`、委托 | `bytes` 的组合偏移，8 B（`ObjectLayout.ref_cells`） | release 写 / acquire 读 |
| 型参 `TypeParam` | 型参字段（`T F;`、`T? F;`） | 标签字在组合偏移（`ObjectLayout.tparam_cells`），负载字追加在编译器布局之后，各 8 B | 标签字 release 写 / acquire 读，负载字 relaxed |
| struct 根 `Struct` | 内联值 struct 字段 | 独立 blob 的布局原样摊在 `bytes`：基元叶子 + 引用叶子的引用字（`ObjectLayout.ref_cells`） | `StructFieldGetPrim` / `SetPrim` |

**引用字**（[metadata/types/ref_word.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/types/ref_word.rs)）是自描述的：
低 3 位记种类，其余位是句柄原始位（`GcRef::to_tagged_bits` / `ObjRef::to_tagged_bits` / `VarGcRef::to_bits`，`RegionEntry` 与变长块头都 8 字节对齐，低 3 位恒为 0），
整字为 0 = `null`（零初始化即正确的默认值）。

| 种类 | `Value` | 句柄 |
|---|---|---|
| 0（整字 0） | `Null` | — |
| 1 / 6 | `Object` / `BoxedStruct` | `ObjRef`（对象块的 `VarGcRef`） |
| 2 | `Array` | `GcRef<ArrayObj>` |
| 3 / 5 | `Str` / `FuncRef` | `VarGcRef`（GC 字符串块） |
| 4 | `Closure` | `VarGcRef`（闭包块） |
| 7 | 装箱的任意 `Value` | `GcRef<ArrayObj>`，单元素只读数组 |

- **种类 7 是逃生口**：`object` / 接口字段能经擦除泛型收到裸 `I64` / `F64` / `Bool` / `Char`（`void Set<T>(H h, T t) { h.O = t; }` 以 `T = int` 调用），
  这些值装不进 8 B。写入时装进单元素数组，读出时取回原值——对用户代码透明。z42c 工作区整套构建一次也不走这条路。
  写屏障拿到的是盒子（`try_set_field_value` 返回 `FieldWrite::Boxed`，调用方经 `with_barrier_value` 发屏障），GC 追踪经 `decode_for_trace` 也看到盒子。
- **型参字段是 16 B 单元**（[metadata/types/tparam_cell.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/types/tparam_cell.rs)）：它们的值在擦除下可以是任何 `Value`。
  判据在 `compose_object_layout`：编译器种类为 `GCREF_CLOSURE`、声明类型是本类的型参名（含 `T?`）。
  闭合实例化（`Box<int>` 的描述符，见 [compiler/generics.md](../compiler/generics.md)）的字段类型已代换，`int` 落基元单元、`string` 落引用字，不是型参单元；
  型参单元出现在没有实例化描述符的实例上：跨包泛型（`LinkedListNode<int>`、`Lazy<int>`）与擦除代码里的分配（`new Node<T>()`，实参是字面的 `T`）。见下「型参单元」。
- **堆上的值 struct 只有一种布局**（[metadata/types/struct_blob.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/types/struct_blob.rs)）：
  装箱 struct 的整个 payload、对象里内联的 struct 字段、`struct[]` 的每个元素都是编译器的 struct 布局（`StructTypeLayout`），
  引用叶子是编译器给它留的 8 B 槽里的引用字。于是内联 struct 与独立 blob 逐字节同布局，偏移只有一套。
  帧 arena（`StructRef`）是寄存器侧的形态：`bytes` + 按 `ref_offsets` 顺序的 16 B `Value`（arena 是 GC 根）；
  `struct_blob::read` / `write` 在两种形态之间转换（读出时引用槽清零，快照里从不带句柄位；写入时每个引用叶子经引用字写原语，
  SATB 记旧字，装不进 8 B 的值装箱，落下的引用交给调用方的写屏障）。
  擦除泛型 struct 的型参叶子（定义布局里按引用叶子排）收到基元时走种类 7 装箱，对用户透明。
- **合成布局**（没有编译器对象块的类型：Rust 构造的类型、单测、`make_fallback_type_desc`）把每个引用字段排成型参单元
  （分不出哪个是型参字段，型参单元什么 `Value` 都装得下）。
- 名→字段下标由 `TypeDesc.field_index`（类级共享）。**继承：基类字段在前、子类追加**，子类区从 `align_up(base.size, 8)` 开始，
  基类部分的单元归属原样继承。
- 访问 `obj.f` = 按字段下标查 `ObjectLayout.field_access[i]`（`{offset, width, tag, cell, aux}`，加载期算好），按单元读写
  （`metadata/types/object_fields.rs`：`load_access` / `store_access`）。站点的 `FieldIC` 把解析结果缓存成
  `TypeId | FieldLoc`（`FieldLoc` = 单元字节偏移 16 位 + 基元宽度 + 种类：基元的精确 `TAG_*` / 引用字 / 型参单元（偏移位存字段下标）/
  struct 根），命中只比较一次 `TypeId` 就做单元访问，不查名字、不读布局（只有型参单元回布局取第二个坐标）。`Value` ↔ 单元的转换只在这里、
  `ref_word` 与 `tparam_cell`；两个引擎经 [objops](interp-jit-semantics.md#对象操作objops) 调到它们。JIT 生成码直接读站点 IC 的
  entry 0，按对象头 → `TypeDesc::id` 守卫后按字节偏移原生读写基元与引用字，引用写内联屏障判定（见 [jit.md](jit.md)「字段访问：
  原生单态字段 IC」）→ **对象头（类型字在 +8、`type_tag` 在 +3、字段从 +16 起）、`TypeDesc::id` 的位置、引用字编码与 Value 大小是 ABI 一部分，须固化**
  （`obj_jit_layout`，由 `object_tests::jit_layout_matches_the_accessors` 钉住）。
- GC 只经 `ScriptObject::visit_refs`（每个非零引用字 + 型参单元标签字里的引用；装箱 struct 按 struct 布局，合成布局按型参单元）遍历一个对象的出边，标记、栈对象根扫描、保留图共用；
  清扫断边用 `clear_refs_for_sweep`。

#### 型参单元

两个 8 B 字：**标签字**在编译器布局里该字段自己的 8 B 槽位，**负载字**追加在编译器布局（`ObjectLayout.size`，8 对齐）之后，
第 i 个型参单元在 `size + 8i`（`tparam_payload_offset`）。负载字放在编译器区之外，所以编译器烘焙的偏移
（内联 struct 叶子、子类的 own 区从 `align_up(base.size, 8)` 起）都不动；子类组合时把继承来的型参单元的负载字
重新排到自己的布局之后。`bytes` 的长度是 `ObjectLayout::bytes_len()`。

| 标签字 | 值 | 负载字 |
|---|---|---|
| `0` | `null` | 从未用过 |
| 引用字，种类 1–6 | 这个引用 | 从未用过 |
| 引用字，种类 7 | 盒子里的值；裸 `7`（空句柄）= `null` | 不再读 |
| `PRIM(k)` = `k << 4` | 种类 `k` 的基元（1 `I64` / 2 `F64` / 3 `Bool` / 4 `Char`） | 原始位 |
| `NULL(k)` = `k << 4 \| 8` | `null`，种类已定为 `k` | 保留 `k` 种位 |

- **种类一生只定一次**，定下的方式是标签字 CAS `0 → NULL(k)`（「认领」）。实参是已知基元时，`ObjNew` 写零值
  （`generic_field_zero_overrides`）就是第一次基元写，分配时即定下；实参是引用或未知（字面的 `T`、没带实参）时，
  种类等到第一次存基元才定。分配点常常不知道实参，若这种实例一律按引用编码，`LinkedList<int>` 的每个节点都要装箱。
- **负载字一生只装一种基元的位**：`0` 与直接引用字表示「从未存过基元」，从它们出发的改写一律 CAS；一旦认领，
  标签字只在 `PRIM(k)` / `NULL(k)` / 种类 7 之间变。所以读者 acquire 到 `PRIM(k)` 后 relaxed 读负载字，
  读到的一定是某次写入的 `k` 种位，不会撕裂；负载字从不装引用，GC 也不看它。
- **写**：`k` 种基元写进 `PRIM(k)` 的单元只改负载字（快路）；`NULL(k)` → 先写负载字再 release 写 `PRIM(k)`。
  `null` 与引用不用负载字：从未存过基元的单元里引用就是标签字本身、`null` 是 `0`，已定种类的单元里 `null` 是 `NULL(k)`。
- **慢路（装箱，种类 7）**：已定种类后来了引用或别种基元、直接引用状态下来了基元、栈句柄。这是擦除泛型下
  「同一个 `T` 字段先后放不同类型的值」才走的路，读出原值对用户透明；进入种类 7 后单元留在标签字里
  （它存过哪种基元已不可知，不能再回负载字）。z42c 工作区整套构建一次也不走这条路，e2e 全量只有把值 struct 句柄存进 `T` 字段的用例会走。
- **屏障**：每次改写标签字都拿到旧字（CAS，或标记期的 swap），旧字是引用就交给 SATB；新落进标签字的引用
  （或盒子）经 `FieldWrite` 交给调用方的 `write_barrier_field`。

### 数组元素

`ArrayObj` 是定长头 + 一个 GC 变长块（`region_var`），元素的存法（**模式**）在分配时由元素类型定下、之后不变
（[metadata/types/array.rs](https://github.com/z42-lang/z42/blob/main/src/runtime/src/metadata/types/array.rs)；
读写在 `array_elems.rs`，GC / 拷贝在 `array_access.rs`）。每个元素单元同样 ≤ 8 B、同宽原子访问：

| 模式 | backing / 块类型 | 元素 | 哪些数组 | 读 / 写 |
|---|---|---|---|---|
| PRIM | `I32` / `I64` / `F64` / `Chars` / `Bytes` / `Bool`，`ArrayPrim` | 打包基元 | `int[]`、`long[]`、`double[]`、`char[]`、`byte[]`、`bool[]` … | 同宽 relaxed 原子 |
| REF | `Refs`，`ArrayRef` | 8 B 引用字（`ref_word`） | `string[]`、`object[]`、类 / 接口 / 委托 / 数组元素类型 | acquire 读 / release 写，标记期 swap |
| MIXED | `Cells`，`ArrayCell` | 16 B 型参单元（`tparam_cell`，元素 i 的标签字在 `16i`、负载字在 `16i+8`） | 擦除的 `T[]`、enum、非 blob struct、类型表里查不到的、Rust 合成的无类型数组 | 型参单元协议 |
| STRUCT | `StructBytes`，`ArrayStruct` | 值 struct blob（引用叶子是引用字，见上） | blob 值 struct 的 `P[]` | 各叶子按自己的单元 |
| FROZEN | `Frozen`，`ArrayValue` | 16 B `Value`，发布前写一次、之后只读 | 闭包环境、种类 7 的值盒子（里面可能是栈句柄，不能再按单元编码） | 普通读 |

- **模式判定**（`objops::array::elem_mode`，`ArrayNew` / `ArrayNewLit` / `Array.CreateInstance` 共用）：
  名字就能定的先定（`ElemKind::of`：基元 → PRIM；`string` / `object` / `X[]` → REF）；元素是型参（指令的 `type_param_kind`）→ MIXED；
  其余查类型表：blob 值 struct → STRUCT，enum 与非 blob struct → MIXED，类 / 接口 / 委托 → REF，查不到 → MIXED。
  这次查表本来就在建数组的路上（判 struct）。
- **MIXED 复用型参单元协议，不用分片锁**：擦除的 `T[]` 编译期不知道元素种类，16 B 是必须的；型参单元的
  「负载字一生只装一种基元、种类由唯一一次 CAS 认领」保证 16 B 元素读永不撕裂（与型参字段同一个证明），
  没有每对象锁也不需要任何锁。实参是基元时（`new T[n]` 的 `class_type_param_zero`，如 `List<int>` 的 backing），
  数组记下该基元种类（`Cells::zero`）、不写任何字：从未写过的单元（标签字 `0`）读作这个零值，第一次写才认领——
  预留容量的页面从不被碰。`List<string>` 的单元里引用就是标签字本身，负载字不用。不符的写（同一个单元先后放不同种类的基元）装箱成种类 7，对用户透明。
- **写屏障**：元素写返回 `FieldWrite`，屏障拿的是真正落进单元的引用（装箱时是盒子）；整元素 `struct[]` 写与批量写
  （`Array.Copy` / 排序）之后按 `elem_ref_slots` 对区间内每个引用槽发 `write_barrier_array_elem`（`objops::array_bulk`）。
  被覆盖的旧引用由各写原语交给 SATB。
- **GC** 只经 `ArrayObj::visit_refs` 遍历一个数组的出边（标记、栈数组根扫描、保留图共用），块本身由 `mark_backing` 标活。
- **JIT**：PRIM 的读写按 `packed_data` 内联；从不被重赋值的 REF / MIXED 数组读进整 `Value` 寄存器（`Ref` / `Str` / 擦除的 `T`）时按
  `ref_data`（基址、长度、步长 8 / 16，入口块取一次）内联：acquire 读元素的（标签）字、按 `KIND_TO_VALUE_TAG` 查表打 tag；
  种类 7 盒子与 MIXED 的基元标记字（低 3 位 0、整字非 0）回落 helper。引用写一律走 helper（要写屏障）。

### 字段单元的内存序

堆上每个可变单元 ≤ 8 B、用同宽原子访问，所以对象不需要锁：16 B `Value` 只留在寄存器，
堆上不会出现「写到一半」的单元，任何时刻读到的都是某次完整写入的值。

- **基元**：relaxed。x86-64 / AArch64 上就是普通对齐 load / store，JIT 内联码直接用普通 load / store。
- **引用字**：release 写、acquire 读（AArch64 上 `stlr` / `ldar`，JIT 内联读用 Cranelift `atomic_load`）。
  于是「在一个线程里构造对象、经字段交给另一线程」是安全的：读到引用的线程一定看得到构造期写入的字段。
- **SATB 删除屏障**要的是被覆盖的真旧值：major 标记进行中（`satb::marking_any()`），引用字的写是 `swap`，
  旧字交给 `record_overwrite`；不在标记期就是普通 release store。这样屏障不需要锁，并发写也不会漏记。
- **跨字段没有一致性**：两个字段之间、读改写之间都没有原子性，用户侧需要组合一致时用 `Mutex<T>` / `Channel<T>`
  （参考手册 threading 页「线程之间怎么传值」写了这份契约）。

- **型参单元**：标签字 release 写 / acquire 读，负载字 relaxed；负载字只在标签字为 `PRIM(k)` 时才被读，
  而它一生只装 `k` 种位（见上「型参单元」），所以两个字分开读写也不会读到「一半标签 + 一半负载」。

**没有每对象锁**（对象模型 R4）：`ObjRef::borrow()` 给 `&ScriptObject`、`GcRef::borrow()` 给 `&ArrayObj`，对象与数组跨线程共享，
字段 / 元素的读写全部是 `&self` 上的原子单元访问（`metadata/types/cells.rs` 的 `Cells`，读写都经同一个视图）。
堆上没有 16 B 的可变 `Value` 单元（FROZEN 数组发布前写一次、之后只读，发布后的写入报错）。

- **整块拷贝也是原子的**：没有对象 / 数组载荷的普通切片视图。快照（装箱 struct 的 blob、对象内联 struct 的基元字节段、
  FFI / I/O 拿走的 `byte[]`，`Cells::copy_out`）与写入（`struct_blob` 的基元段，`Cells::copy_in`）按「当前位置对齐、
  剩余长度放得下的最宽 8 / 4 / 2 / 1 B」分块，每块一次原子访问；单元都按自身宽度对齐且 ≤ 8 B，所以分块从不劈开一个单元
  （一块可能盖住几个更窄的单元——混合宽度访问，x86-64 / AArch64 上单拷贝原子，与 JIT 的普通宽度内联访问同一个前提）。
  8 B 块用 acquire 读：块里可能是引用字，数组克隆把它带进新数组后，读新数组的线程要看得到被引用对象的构造。
  PRIM 数组之间的批量移动（`Array.Copy`、同一数组内的重叠移动）按元素宽度逐元素 relaxed 读写（`Cells::copy_prims`，
  memmove 方向），数组克隆（`deep_copy`）读源块用 `copy_out`。
- **批量操作对别的线程不是原子的**：`Array.Copy` / 排序 / struct 整体读写都是一串单元访问，别的线程可能看到做了一半，
  但每个单元是完整的值。没有取锁顺序问题：两条线程同时互相 `Array.Copy` 两个数组也只是逐元素竞争。
- **需要独占的场合各有机制**，都不经 `GcRef`：
  - 构造：对象在发布之前建好。`ObjNew` 的堆分支（interp、JIT）走 `MagrGC::alloc_instance`：取一个清零的对象块、写对象头、
    在**还没发布**的块上跑泛型一步（`install_type_args`：驻留的实参句柄 + 型参字段零值），然后才记账（可能触发回收）并交出句柄——
    取块与发布之间不分配任何东西，所以没有回收能看到建到一半的对象。栈分支用 `OwnedObject::new_instance` 建同样的对象；
    `Activator.CreateInstance` 与带初值 / 原生底座的 `alloc_object` 先在 `OwnedObject` 里建好（写初值可能要分配装箱盒子），再
    `MagrGC::alloc_script_object` 整块拷进新块。`set_native` / `set_type_args` 只在构造期（`&mut self`）。发布之后对象的 `meta` 字
    （`NativeData`、驻留的泛型实参句柄）不再变。
  - 栈数组（`StackVec`）由每上下文的 stack arena 持有、在它的锁下访问，写入走 `ArrayObj::set_boxed_exclusive`（`&mut self`）。
  - GC：对象块被 tombstone 时由 drop glue 释放对象头（`Arc<TypeDesc>`、extras 盒子）；数组头在槽位复用 / decommit /
    region drop 时 drop。此时块 / 条目已死、持 region 锁或在停顿里。
  - 原生数据自带同步：`NativeData::Monitor`（`Std.Threading.Mutex/RwLock/Channel` 的底座）用自己的内部锁，
    WeakRef / Type / LoadContext / Assembly 句柄发布后只读。
- **`lock(obj)`**：语言里还没有 `lock` 语句、也没有 `Monitor.Enter(obj)` 这类按对象加锁的 API（z42 代码的锁都是
  `Std.Threading` 的 `Mutex<T>` / `RwLock<T>`，底座是对象自己的 `NativeData::Monitor`），所以去掉每对象锁不需要过渡机制。
  对任意对象加锁的语义由 R5 的 Monitor 侧表提供（首次加锁才膨胀，按对象地址索引，GC 停顿时清理）。

**静态字段**用同一套单元：每个静态字段一个地址稳定的 16 B 型参单元（`vm_context/static_table.rs`，`VmCore.statics`，
`SegVec` 分段只追加、id 即下标），读写不拿锁，内存序与实例字段相同。按名登记时不知道声明类型，型参单元什么 `Value` 都装得下：
基元首写时认领种类、之后只改负载字；引用就是标签字；种类不符的写装箱。值 struct 类型的静态字段由编译器装箱存放，单元里只有一个
引用字，所以不需要 seqlock。静态字段是 GC 根（每次回收扫一遍标签字），写入不需要分代屏障，SATB 由单元协议负责。单元旁边缓存
类型初始化屏障的属主判定（免检 / 待查的 `TypeDesc`）和屏障的放行记录（放行时的屏障纪元，见[静态构造函数的按类型初始化](static-ctor-init.md)）：
稳态一次 relaxed load 纪元 + 一次 acquire load 单元，JIT 生成码内联同一判定与单元读写。

后续：

- JIT 的引用字内联读不覆盖型参字段（标记字按引用字查表会读成 `null`），型参字段读写仍走 helper；MIXED 数组的基元元素读也走 helper。
- 原生字段 IC 只比 entry 0：同一站点见到多个接收者类型（继承字段经基类方法访问）时，其余类型走 helper 的 4 槽扫描。
- 对象的类型字是一个 `Arc<TypeDesc>`：每次分配一次原子 +1、每次回收一次 −1。描述符的生存期并不统一（按分配新建的回落描述符、
  只被加载上下文持有的类型、继承 fixup 的 copy-on-write 新版本），改成不计数的细指针 / 32 位类型号之前要先让描述符由一个只增不减的
  arena 持有（R8 没做）。

### 槽位零初始化

**不变式：值类型的存储槽永不含 `Value::Null`。** 值类型必须在**分配点**一次解决，而不是在读取侧打补丁——若存储初始化不看声明类型
（`vec![Value::Null; n]`），会长出一族内部错误
（`__box_prim: expected integer value, got Null` / `type mismatch in arithmetic: Null vs I64(1)`）。

口径：对象分配不逐字段填默认值，而是按 `TypeDesc::object_bytes_len()`（composed layout）取一个**整块清零的对象块**
（`gc/arc_heap/alloc_object.rs`；堆外的 `OwnedObject` 同样清零）。于是：

| 槽的种类 | 零值 | 为什么对 |
|---|---|---|
| 基元值字段（int/bool/char/double/long…） | 字节零 ⇒ `0` / `false` / `'\0'` / `0.0` | 值落在 bytes 区 |
| 引用字段 | 引用字为 0 | `null` 本就是引用类型的零值；整字 0 就是 `null` 的编码 |
| 数组元素 | `default_value_for_tag(elem_tag)` | `ArrayNew`（interp + JIT 两份）按元素 tag 取；`ArrayObj::filled` 直接写进 GC 块（是该模式的零 —— 基元 0 / REF 与 MIXED 的 `null` —— 就不逐元素写；MIXED 的基元 `T` 记作数组的 `zero`，未写过的单元读作它）|
| **型参字段**（`class GBox<T> { T V; }`） | 按**实例化**取：`default_value_for(type_args[i])` | 见下 |

**型参字段要单独一条**，因为布局是**按声明**算的：声明里 `T` 不是基元 ⇒ 该槽是型参单元 ⇒
整块零初始化给它的是标签字 `0` = `Null`，而不是 `GBox<int>` 该有的 `0`。
实例自己带着实参（`ObjNew` 经 `install_type_args` 写入驻留句柄），所以真正的零值在**分配点**可以还原：
把字段的 `type_tag` 按名字映射到 `TypeDesc::type_params()` 的下标，再取该实参的零值
（`metadata/types/field.rs::generic_field_zero_overrides`，由 `ScriptObject::install_type_args` 调用，interp 的堆/栈两支 + JIT 三处共用这一个入口）。
这次写入同时定下型参单元的基元种类（见上「型参单元」）。

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
- 现:8B 标记指针 = `RegionEntry` 地址（数组）/ 变长块头地址（对象 `ObjRef`、字符串、闭包）+ 高 16 位窄 generation(ABA 防护，§2.1)。**改名 `generation`→`epoch`**:避免与**分代 GC 的 young/old generation** 混淆。
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
  mark/sweep（string 是**不可变叶子**，trace 无出边）。字段存储：string 字段是对象里的 8 B 引用字（种类 3）
  （`STRUCT_LEAF_ARCSTRING` → `TAG_STR`），被 `trace_children` / `scan_object_refs` 扫描 →
  string-in-object 正确可达；`is_heap_ref(Str)=true` → 存进堆槽触发写屏障（分代 card）。
- **驻留/字面量串**：**每 VM 惰性驻留**——加载期**不**物化（无堆），某个字符串 id 首次 `ConstStr(idx)`
  用活堆分配 GC string，发布进 `VmCore.strings`（`StrTable`，按 id 的稠密表）并记为 **GC root**；
  后续命中（任一线程）无锁读出 8B 句柄（运行期全走 `VmContext::const_str`）。
- **safepoint 安全**：GC 只在显式 safepoint（interp 回边/调用边界）/`ForceCollect` 运行，从不在单条
  指令/builtin 的 Rust 执行中途 → 临时 string（表达式中间值）落寄存器前天然安全，与既有
  Object/Array 临时值同一不变式（分配器 `maybe_auto_collect` 只置标志、延到 safepoint）。
- 代价:纳入 GC → 多点 GC 压力(换掉 Arc 确定性释放，string-heavy 的 z42c 自编译最敏感);收益:统一一套堆 + 为可移动/压缩/去重铺路。架构统一优先于短期性能。
- **闭包与访问器**：`ClosureData` 是 `{env: GcRef, fn_id: FnId}`，闭包块全 POD（region_var 仅 `ArrayValue` 需 finalizer）；
  目标在 `MkClos` 时绑定成函数 id（P1-2 PR 8，见 [vm-architecture.md「闭包 dispatch」](vm-architecture.md#闭包-dispatchclosure--funcref)），
  闭包里不再有名字串，PR 7 那层每线程的名字驻留（`fn_name_cache`）随之删除；
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
