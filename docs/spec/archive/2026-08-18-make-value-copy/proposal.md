# Proposal: 让 `Value` 成为 `Copy`（4 个 Box 瞬态变体 → arena 句柄）

## Why

`Value::clone` 是 interp-bound workload（z42c 前端 lex+parse+typecheck）的**头号 leaf**——
profile 实测 **423/3721 = 11.4%**，`drop_in_place<Frame>` 再占 **6.0%**（195+29 样本）。

memory 旧结论认为「clone = 16B 拷贝 + refcount，drop<Frame> 不可避免」。**这已过时**：
unify-gc-heap 之后 clone 早已**没有 refcount**（`GcRef::clone` = 纯 8B memcpy、`Str` = `Copy`、
`GcRef::Drop` = no-op）。clone/drop 之所以昂贵，**唯一根因**是 `Value` 还挂着 4 个 `Box` 冷变体
（`Ref`/`PinnedView`/`StackClosure`/`StructRefHeap`）+ `GcRef` 的显式 no-op `Drop`，逼编译器把每次
clone 编成「match 判别号 + drop-glue」、把 `Vec<Value>` 析构编成逐元素循环，**无法退化成平凡 memcpy /
O(1) 释放**。

**实测 spike**（把 4 变体临时改 `&'static` 泄漏引用 + `GcRef` 加 `Copy` → `Value` 派生 `Copy`）：
前端 **7.231s → 5.977s = 1.21× / 17.3% faster**，输出**逐字节一致**。这是迄今最大的单一 interp 杠杆
（超过已合并的 fxhash 11.4%）。`Value::clone` 从 profile 头号 leaf **完全消失**，`drop<Frame>` 195→68。

不做的代价：interp 最大的一块可回收开销一直在每次调用/Mov/字段读/参数传递上白白流失。

## What Changes

把 4 个「仅在创建帧的调用栈内存活、创建后不可变」的瞬态 `Box` 变体，改为已被
`StackObject`/`StackArray`/`StructRef` **三次验证过的 per-`VmContext` arena 句柄模式**：
payload 存进 arena，`Value` 里只留 `{ idx: u32, frame_id: u32 }` 8B `Copy` 句柄 + frame_id
staleness 守卫；arena 作 GC root 扫描（`scan_roots`）、随创建帧 LIFO truncate。

- `Value::Ref(Box<RefKind>)` → `Value::Ref { idx, frame_id }`
- `Value::PinnedView(Box<PinnedViewData>)` → `Value::PinnedView { idx, frame_id }`
- `Value::StackClosure(Box<StackClosureData>)` → `Value::StackClosure { idx, frame_id }`
- `Value::StructRefHeap(Box<StructArrayElem>)` → `Value::StructRefHeap { idx, frame_id }`
- `GcRef<T>`：删除显式 no-op `Drop`，加 `impl Copy`（其 backing 是 POD 标记指针，drop 本就是 no-op）
- `Value` 加 `#[derive(Copy)]` → clone 变平凡 memcpy、`Vec<Value>` 析构 O(1)

新增统一 `TransientArena`（一个 `Vec<TransientSlot>`，`TransientSlot` 是这 4 个 payload 的 enum），
接进 `VmContext` + `push_frame`/`pop_frame` base 戳记/truncate + GC root 扫描（interp 与 JIT 共用，
二者均已走同一 `push_frame`/`pop_frame`，JIT 已用相同 `{idx,frame_id}` 句柄读 `stack_arena`）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/interp/transient_arena.rs` | NEW | `TransientArena` + `TransientSlot`（4 payload enum）+ alloc/with/truncate/scan_roots |
| `src/runtime/src/interp/transient_arena_tests.rs` | NEW | arena alloc/staleness/truncate/scan 单元测试 |
| `src/runtime/src/interp/mod.rs` | MODIFY | `mod transient_arena`；`deref_ref`/`get_deref`/`set_thru_ref` 经 arena 解 `RefKind` |
| `src/runtime/src/metadata/types.rs` | MODIFY | 4 变体 → `{idx,frame_id}` 句柄；`derive(Copy)`；`PartialEq`/`visit_gc_children`/`tag`/`heap_size` 各臂；`RefKind`/`PinnedViewData`/`StackClosureData`/`StructArrayElem` 保留为 arena payload |
| `src/runtime/src/gc/refs.rs` | MODIFY | `GcRef` 删 `Drop`、加 `Copy` |
| `src/runtime/src/vm_context.rs` | MODIFY | `transient_arena` 字段 + 2 处构造初始化；`VmFrame` 加 `transient_base`；`push_frame` 戳记 / `pop_frame` truncate；GC root `scan_roots` 接线 |
| `src/runtime/src/gc/arc_heap.rs` | MODIFY | 移除 `Ref`/`StructRefHeap` 的 `mark_if_unmarked`/`mark_phase`/`gen_age` 穿句柄臂（arena root 覆盖） |
| `src/runtime/src/interp/exec_address.rs` | MODIFY | `Ref` 构造 → arena alloc |
| `src/runtime/src/interp/exec_native.rs` | MODIFY | `PinnedView` 构造 + `UnpinPtr` 消费经 arena |
| `src/runtime/src/interp/exec_array.rs` | MODIFY | `StructRefHeap` 构造 → arena alloc |
| `src/runtime/src/interp/exec_struct.rs` | MODIFY | `StructRefHeap` 消费（`StructFieldGetPrim/SetPrim`）经 arena |
| `src/runtime/src/interp/exec_call.rs` | MODIFY | `StackClosure` 构造 + `CallIndirect` 消费经 arena |
| `src/runtime/src/interp/exec_object.rs` | MODIFY | `PinnedView` `.ptr`/`.len` FieldGet + `StructRefHeap` 消费经 arena |
| `src/runtime/src/interp/exec_vcall.rs` | MODIFY | `matches!(_, Value::Ref(_))` 模式改 `{..}`（trivial） |
| `src/runtime/src/corelib/convert.rs` | MODIFY | `value_to_str` 4 变体降级为通用串（照 `StackObject`/`StructRef` 先例，ToString 是 escape sink → 句柄不到达） |
| `src/runtime/src/corelib/object.rs` | MODIFY | `builtin_delegate_target/fn_name/eq` 读 `StackClosure` 经 arena（已有 `ctx`） |
| `src/runtime/src/corelib/threading.rs` | MODIFY | `matches!(_, Value::StackClosure(_))` 模式改 `{..}`（trivial） |
| `src/runtime/src/native/marshal.rs` | MODIFY | `PinnedView` marshal FFI 读 `pv.ptr/len` 经 arena（已有 `ctx`） |
| `src/runtime/src/jit/helpers/closure.rs` | MODIFY | JIT `StackClosure` 构造 → arena alloc |
| `src/runtime/src/jit/helpers/array.rs` | MODIFY | JIT `StructRefHeap` 构造 → arena alloc |
| `src/runtime/src/jit/helpers/object.rs` | MODIFY | JIT `StructRefHeap` 消费经 arena |
| `src/runtime/src/jit/translate.rs` | MODIFY | 变体 match/注释同步（如有真实分支） |
| `src/runtime/src/metadata/types_tests.rs` | MODIFY | 4 变体构造测试改 arena 或改断言 |
| `src/runtime/src/corelib/tests.rs` | MODIFY | `StackClosure` 构造测试改 arena |
| `src/runtime/src/corelib/threading_tests.rs` | MODIFY | 同上 |
| `src/runtime/src/interp/exec_struct_tests.rs` | MODIFY | `StructRefHeap` 构造测试改 arena |
| `docs/design/runtime/value-representation.md` | NEW | `Value` 16B 布局 + `Copy` 化 + 瞬态 arena 句柄模式的机制文档（知识上浮） |
| `src/runtime/src/interp/README.md` | MODIFY | 功能索引 + 核心文件加 `transient_arena.rs` |

**只读引用**（理解上下文，不改）：
- `src/runtime/src/interp/struct_arena.rs` / `stack_alloc.rs` — arena 模式模板
- `src/runtime/src/exception.rs`（`VmFrame`）— push/pop base 字段承载处（若 `VmFrame` 在此则并入 vm_context 行）

## Out of Scope

- 不改 zbc/zpkg 格式（这 4 变体是**运行时** `Value`，从不序列化）→ **无格式 bump**
- 不改 `Value::Closure`（已是 `VarGcRef` GC 句柄，`Copy`）/`StackObject`/`StackArray`/`StructRef`
  （已是句柄）/`BoxedStruct`（`GcRef`，随 `GcRef: Copy` 自动 `Copy`）
- 不动 dispatch 主循环 / 符号解析 hashmap（另线杠杆）
- 不追求 4 变体各自的进一步语义优化，只做「Box → arena 句柄」的最小机械转换

## Open Questions

- [ ] `VmFrame` 定义在 `exception.rs` 还是 `vm_context.rs`？（base 字段落点，实施首步确认，不影响方案）
