# Design: JIT struct 值路径（P5-A helper 桥接）

## Architecture

```
JIT 函数体 (cranelift)
  ├─ 算术 / 控制流 / 普通调用 …………………… native code（整函数收益来源）
  └─ struct 指令 → call helper（Rust）→ ctx.struct_arena
       StructAlloc         → jit_struct_alloc(frame, ctx, dst, type_ptr, size)
       StructCopy          → jit_struct_copy(frame, ctx, dst, src, size)
       StructFieldGetPrim  → jit_struct_field_get_prim(frame, ctx, dst, base, byte_off, kind)
       StructFieldSetPrim  → jit_struct_field_set_prim(frame, ctx, base, byte_off, kind, val)

JitFrame { regs: Vec<Value>, ret, env_arena, frame_id: u32 }   ← 新增 frame_id
                                                       ↑
        每个 JIT 帧创建点 ctx.next_frame_id() 分配（与 interp Frame 同一分配器）

struct_arena（per-VmContext）: interp 与 JIT 共用同一个 arena。
  LIFO base 由现有 push_frame(stamp struct_base) / pop_frame(truncate) 管理（JIT 入口+jit_call 已调）。
  frame_id 悬垂校验由本变更补上。
```

interp 与 JIT 共享同一 `VmContext.struct_arena` 与同一 `next_frame_id` 分配器——一次执行里 interp 帧与
JIT 帧的 frame_id 全局单调唯一，混合调用（JIT→interp→JIT）时 arena 语义天然一致。

## Decisions

### Decision D1: helper 桥接 vs 原生内联字节访问

**问题：** JIT 如何执行 4 条 struct 指令？

**选项：**
- **A helper 桥接**：每条 struct 指令 emit 成对 Rust helper 的 call，helper 操作 struct_arena，复用
  interp 字节编解码 + arena 逻辑。struct op ≈interp 速度；整函数其余部分 native。
- **B 原生内联**：FieldGet/SetPrim 直接 emit cranelift native load/store 到 arena 字节。struct op 本身
  更快，但需向 cranelift 暴露 arena 内存布局、逐 TypeTag 生成 load/store、refs 侧表仍得 helper，且裸
  指针指进 `Vec<StructSlot>` 与移动 GC / realloc 交互有健全性风险。

**决定：选 A（User 2026-08-12 裁决）。** 理由：① 两者「消除整函数 bail」的收益相同——这是 P5 的 95%
价值；② struct op 极少是内层最热算术（热点通常在数值/数组遍历），B 相对 A 的边际提速有限，却承担裸
指针×移动 GC×realloc 的健全性风险；③ A 与现有 JIT helper 架构（obj_new/vcall/closure/装箱全走 helper，
从不在 native 裸操作堆内存）一致，复用 interp 逻辑、风险低、单 PR 可落。**B 记 Deferred**——待 benchmark
证明某热路径卡在 struct 字段读写的 helper 开销上，再针对性做原生内联（数据会指明该内联哪几条）。

### Decision D2: JitFrame 的 frame_id 来源与生命周期

**问题：** JIT helper 产 `StructRef{idx, frame_id}` 需要 frame_id，`JitFrame` 没有。

**决定：** `JitFrame` 加 `frame_id: u32` 字段（默认 `0`），采用**纯惰性分配**——只在**分配型 struct
helper**（`jit_struct_alloc` / `jit_as_cast` 拆箱 / `copy_array_elem_out`）里，若 `frame_id==0` 则从
`vm_ctx.next_frame_id()` 取一个真值（`struct_ops::frame_id_of`）。

**为什么纯惰性（而非每个帧创建点 eager 设）**：`frame_id` 只被**分配**路径用到——`StructFieldGetPrim`/
`Copy` 等 deref 用的是 `StructRef` 句柄里**内嵌**的 frame_id（`as_struct_ref` 从值里取），不看当前帧。
故只有真正 alloc struct 的帧才需要 id，一处惰性赋值即覆盖入口/所有嵌套 callee（call/vcall/ctor/closure），
**零 per-site churn**（否则要改 ~11 个帧创建点，且易漏）。`next_frame_id()` 从 `1` 起自增、interp 帧也
用它（`exec_function_body` mod.rs:725），故 `0` 是安全的「未分配」哨兵、惰性赋值与 interp 帧永不撞号。

**OSR 例外（必须 eager 继承）**：OSR（`from_interp_regs`，interp mod.rs:618）续接**同一逻辑活动记录**，
其 JitFrame 必须**继承 interp 帧的 frame_id**——OSR 前循环已在 arena 分配的 struct 局部（frame_id =
interp 的）在交接后仍要能 deref，OSR 内新分配也须与之一致。故此处 eager 设 `osr.frame_id =
frame.frame_id`（先于任何惰性赋值，非 0 → 惰性跳过）。

**LIFO base 无需新增**：JIT 入口（`mod.rs:174`）与 `jit_call` 系列 helper 已 `push_frame`/`pop_frame`，
`push_frame` 已 stamp `frame.struct_base = struct_arena.base()`、`pop_frame` 已 `truncate`——arena 的
分配区间随帧正确回收。本变更只补 frame_id 悬垂校验维度。

> **正确性注记**：即便某帧 frame_id 停留 0（从不 alloc struct），也无害——arena 靠 LIFO `struct_base`
> 回收 + `idx` 唯一标识活 struct（非逃逸 + LIFO ⟹ 任一时刻 idx 唯一对应一个活 struct），frame_id 仅
> 是抓「引用越过帧退出」的防御性 guard。惰性真值让该 guard 对所有实际 alloc struct 的帧生效。

### Decision D3: helper 复用 interp 逻辑的方式

**问题：** interp `exec_struct.rs` 的函数签名吃 interp `Frame`，JIT 是 `JitFrame`，不能直接调。

**决定：** JIT helper 在 `jit/helpers/struct_ops.rs` 里薄封装，读写 `JitFrame.regs`，**arena 操作 +
字节编解码核心逻辑复用 interp 的自由函数**：把 `exec_struct.rs` 的 `decode_prim` / `encode_prim` /
`prim_width` / `is_ref_tag` / `resolve_layout` 由 `pub(super)` 提为 `pub(crate)`（零逻辑改动），JIT
helper 直接调。arena 方法（`alloc`/`copy_into`/`with`/`with_mut`/`get_ref`）已 `pub(crate)`。

base 多态（arena StructRef / 堆 Object 内联字段 / StructRefHeap 数组元素）在 helper 内以与 interp
`struct_field_get_prim`/`set_prim` **同构的 match 三臂**实现——三态语义必须逐字节等价 interp（含堆
Object 引用叶子写的 `write_barrier_field`）。

> 这是 z42 JIT helper 的既定惯例：helper 与 interp 逻辑并行存在、共享可复用核心（编解码/arena），
> 帧访问各用各的寄存器类型。避免把 interp `Frame` 泛化成 trait（过度抽象、热路径不值）。

### Decision D4: jit_array_get 的 StructBytes 特判

**问题：** `jit_array_get`（`array.rs:127`）对任何数组用 `borrowed.get_boxed(i)`——对 StructBytes
backing 产 **BoxedStruct 快照**，不是 `StructRefHeap`。这样 `arr[i].x` 的后续 FieldGetPrim（base 期望
StructRefHeap/StructRef）会拿到 BoxedStruct base 而错。

**决定：** `jit_array_get` 加 StructBytes 特判，镜像 interp `array_get`（`exec_array.rs:181`）：
`ArrayBacking::StructBytes` → 产 `Value::StructRefHeap{arr: rc.clone(), index: i}`；其余 backing 保持
`get_boxed`。StructRefHeap 引用堆 `GcRef<ArrayObj>`+index，**无需 frame_id**——是三块里最独立的。

### Decision D5: jit_as_cast 拆箱 BoxedStruct

**问题：** `jit_as_cast`（`object.rs:460`）现对 BoxedStruct 精确匹配时**保持 boxed**（注释说 JIT 无
frame_id 不能产 StructRef，消费拆箱结果的 struct 指令又会使整函数 bail，故留 interp）。前提消失后应拆箱。

**决定：** `jit_as_cast` 对 `BoxedStruct` 精确类型匹配时，调 struct arena 把堆 blob 拷回当前 JIT 帧
（等价 interp `unbox_struct`）返 `StructRef{idx, frame_id}`；`Std.Object`/`Object`/基类/接口等
「保持 boxed」的既有分支不变；类型不符返 Null。删去「JIT 无 frame_id」相关注释。

### Decision D6: jit_array_new 必须造 StructBytes backing（实施期发现）

**问题（实测抓到）：** `jit_array_new` 原用 `alloc_array_typed(elem_type, vec![default; n])` 造**普通
boxed 数组**（元素 = `default_value_for_tag`，对 struct = Null），**不**走 interp `array_new` 的
`try_struct_backed` 路径。于是 JIT 下 `new Point[3]` 造出 Null 元素数组，`arr[i]`（jit_array_get）返
Null（非 StructBytes → 我的 StructRefHeap 分支不触发），后续 `StructFieldGetPrim` base=Null → 崩。

**决定：** `jit_array_new` / `jit_array_new_lit` 补 `try_struct_backed` 分支，对 value-struct 元素造
`ArrayBacking::StructBytes` backing（`alloc_array_obj`），literal 用 `pack_struct_elem` 逐元素打包——
**复用 interp `exec_array` 的这两个函数**（提 `pub(crate)`，零逻辑改动），与 interp `array_new` 逐字节
等价。`jit_array_new_lit` 返回类型从 `()` 改 `u8`（struct 打包 / OOM 可抛，translate 加 `check!`）。

> 这是 spec「struct[] element read/write」场景的**前置**（没有 struct-backed 数组，`arr[i]` 无从产
> StructRefHeap）——原 Scope 只列了 `jit_array_get`，实施期补上 `jit_array_new`/`new_lit` + interp
> 函数暴露（proposal Scope 已同步更新）。

## Implementation Notes

- **helper 注册三处**（`registry.rs`）：`reg!` 注册 symbol、`decl!` 声明 FuncId 签名、`HelperIds` 加
  字段；`translate.rs` 用 `imp!(helper_ids.struct_*)` 取 FuncRef 后 `builder.ins().call(...)`。
- **helper 返回值约定**：struct helper 可能 `bail!`（悬垂/布局缺失）——遵循现有 helper 的异常约定
  （`set_exception` + 返回非 0 u8，translate 侧检查后跳异常路径），与 jit_array_get 一致。alloc/copy/
  get/set 失败都要能触发异常而非 UB。
- **参数封送**：`type_name`（StructAlloc）作 `(ptr, len)` 传（复用现有 emit string 常量的模式，如
  obj_new 的 cls_ptr/cls_len）；`byte_off`/`kind`/`size`/`dst`/`base`/`val` 作 i32/i8 立即数。
- **GC 安全**：arena 是 GC 根（`scan_roots` 已接入 `vm_context` 的 mark 根扫描）——helper 内加锁写
  arena，与 interp 同一套，无新增写屏障（堆 Object base 的引用叶子写已有 `write_barrier_field`，helper
  照抄）。JIT native 代码不持有 arena 裸指针（A 路线的核心安全性）。
- **exec_struct.rs 可见性**：仅改 5 个自由函数 `pub(super)`→`pub(crate)`，无逻辑改动——self-host 不涉及
  runtime 源，z42c 逐字节不变。

## Testing Strategy

- **单元测试**（`struct_ops_tests.rs`）：构造 JitFrame + VmContext，直接调 4 个 helper，断言
  ① 分配+基元读写 round-trip；② copy 值独立性；③ 嵌套 offset；④ 引用叶子经 refs 侧表；⑤ 悬垂
  frame_id 不符 → err；⑥ base=StructRefHeap（构造 StructBytes 数组）读写元素叶子。
- **Golden（JIT 模式）**：新增 `src/tests/types/struct_jit.z42` 综合用例（本地 struct + 嵌套 +
  string 叶子 + struct[] index/foreach + 装箱拆箱），在 `xtask test e2e --mode jit` 下 EXIT=0；
  同时确认既有 `struct*.z42` golden 在 JIT 模式全过（之前它们在 JIT 下靠 bail→interp 才过，现在真正
  走 JIT struct 路径）。
- **等价性**：同一 golden interp 模式 vs JIT 模式输出一致（值语义不因执行模式而异）。
- **GREEN**：`cargo test --lib`（含新单测）+ `xtask test`（不传 Z42_HOME）+ **JIT 专腿**
  `xtask test e2e --mode jit`（含全部 struct golden）+ self-host 5/5（z42c 零改动，应逐字节不变）。
- **CI**：`test-vm-jit(linux-x64)`（job key `vm-jit-consistency`）跑全量 golden 的 JIT 一致性——这是
  本变更的主门。格式中立 → 无 fixture 重生、无两代自举。

## Deferred / Future Work

### struct-jit-value-path-future-native-inline: JIT 原生内联字节访问（P5-B）

- **来源**：本变更 design.md 决策 D1（A/B 分叉）。
- **触发原因**：A（helper 桥接）已拿到 P5 的整函数 JIT 收益；原生内联的边际提速仅在「struct 字段
  读写是内层热循环」时显著，却引入裸指针×移动 GC×realloc 健全性复杂度——过早优化。
- **前置依赖**：benchmark 证明某热路径的 struct FieldGet/SetPrim helper-call 开销是瓶颈；一套安全的
  「arena 字节指针在 GC safepoint 间有效性」契约（或把该 struct 提升到不会 realloc 的稳定存储）。
- **触发条件**：出现 struct 字段读写密集的真实 workload 且 profile 指向 helper 边界开销。
- **当前 workaround**：helper 桥接（本变更）——正确且拿到整函数收益，只是 struct op 本身 ≈interp 速度。
