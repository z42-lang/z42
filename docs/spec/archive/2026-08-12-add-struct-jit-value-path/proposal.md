# Proposal: JIT 值路径——struct 值类型指令 JIT 化（P5-A helper 桥接）

## Why

struct 值类型的 interp 功能面已闭合（值复制/传参/返回/嵌套字段/`==`/装箱/泛型容器/堆对象内联字段/`struct[]`/foreach）。
但 **JIT 完全不支持 struct**：一旦函数体出现任一条 struct 值类型指令（`StructAlloc` /
`StructCopy` / `StructFieldGetPrim` / `StructFieldSetPrim`），JIT 直接 `bail!` → **整个函数回退
interp**（`jit/translate.rs:1504-1509`）。后果：任何用到 struct 的函数——哪怕 struct 只是几个字段的值
对象、真正的热点是周边的算术/循环/数组遍历——都拿不到任何 JIT 收益。

根因唯一：interp 的 struct 值语义靠 per-context 字节 arena（`VmContext.struct_arena`），寄存器持
`Value::StructRef{idx, frame_id}` 句柄，`frame_id` 来自 interp `Frame.frame_id`（LIFO 截断 + 悬垂
校验）。而 `JitFrame` **没有 frame_id 字段**，无法产出合法 StructRef，故只能整体 bail。

本变更接通 JIT 的 struct 值路径（helper 桥接路线，见 design.md 决策 D1），消除 bail，让含 struct 的
函数被 JIT 编译。**原生内联字节访问（更快但复杂+GC 风险）记 Deferred，待 benchmark 证明需要再做。**

## What Changes

- **给 `JitFrame` 加 `frame_id: u32`**，在每个 JIT 帧创建点（入口 / 嵌套 jit_call / OSR）用
  `next_frame_id()` 分配——JIT 帧就此参与 struct_arena 的悬垂校验（LIFO base 已由现有
  `push_frame`/`pop_frame` 管理）。
- **新增 4 个 JIT helper**（`jit_struct_alloc` / `jit_struct_copy` / `jit_struct_field_get_prim` /
  `jit_struct_field_set_prim`），操作 `ctx.struct_arena`，复用 interp 的字节编解码 + arena 逻辑，
  base 多态（arena `StructRef` / 堆 `Object` 内联字段 / `StructRefHeap` 数组元素）与 interp 一致。
- **`translate.rs` 把 4 条 struct 指令的 bail 换成 helper call**。
- **`jit_array_get` 对 `StructBytes` backing 产 `StructRefHeap`**（而非现在的 `get_boxed`→BoxedStruct
  快照）——接通 JIT 下 `arr[i].x` 的 struct[] 元素访问（镜像 interp `array_get` #170）。
- **`jit_as_cast` 对 `BoxedStruct` 精确匹配时拆箱→arena `StructRef`**（现因无 frame_id 保持 boxed
  →interp only；有了 frame_id 即可拆箱，镜像 interp `as_cast`/`unbox_struct`）。
- Golden / 单元测试在 **JIT 模式**下验证既有 struct 用例端到端等价 interp。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/jit/frame.rs` | MODIFY | `JitFrame` 加 `frame_id: u32` 字段 + 三个构造函数（`new`/`new_args_from`/`from_interp_regs`）初始化 |
| `src/runtime/src/jit/mod.rs` | MODIFY | 入口帧创建后用 `ctx.next_frame_id()` 填 frame_id |
| `src/runtime/src/jit/helpers/call.rs` | MODIFY | 嵌套 jit_call 的 callee 帧分配 frame_id |
| `src/runtime/src/jit/helpers/struct_ops.rs` | NEW | 4 个 struct helper（alloc/copy/field_get_prim/field_set_prim），操作 JitFrame.regs + struct_arena |
| `src/runtime/src/jit/helpers/struct_ops_tests.rs` | NEW | helper 单元测试（值语义/嵌套/引用叶子/悬垂校验） |
| `src/runtime/src/jit/helpers/mod.rs` | MODIFY | `mod struct_ops;` + 导出 |
| `src/runtime/src/jit/helpers/registry.rs` | MODIFY | 注册 4 个 helper（symbol + FuncId decl + HelperIds 字段） |
| `src/runtime/src/jit/helpers/array.rs` | MODIFY | `jit_array_get` StructBytes→StructRefHeap 特判；`jit_array_new`/`jit_array_new_lit` 对 value-struct 元素造 StructBytes backing（复用 interp `try_struct_backed`/`pack_struct_elem`，镜像 interp `array_new`）——否则 JIT 下 `new Point[]` 造普通 Null 数组、`arr[i]` 无 StructRefHeap |
| `src/runtime/src/jit/helpers/object.rs` | MODIFY | `jit_as_cast` BoxedStruct 精确匹配→拆箱 StructRef |
| `src/runtime/src/jit/translate.rs` | MODIFY | 4 条 struct 指令 bail→helper call + import helper ids |
| `src/runtime/src/interp/exec_struct.rs` | MODIFY | 抽出 frame 无关的 `*_val` 核心（`struct_alloc_val`/`struct_copy_val`/`struct_field_get_val`/`struct_field_set_val`）+ 把字节编解码自由函数（`decode_prim`/`encode_prim`/`prim_width`/`is_ref_tag`/`resolve_layout`）+ `unbox_struct`/`copy_array_elem_out`（改吃 `frame_id:u32`）由 `pub(super)` 提为 `pub(crate)` 供 JIT 复用；interp 函数变薄封装（行为不变） |
| `src/runtime/src/interp/exec_array.rs` | MODIFY | `try_struct_backed`/`pack_struct_elem` 由私有 `fn` 提为 `pub(crate)` 供 JIT `jit_array_new`/`new_lit` 复用（无逻辑改动）；`array_new` 的 unbox/copy 调用点改传 `frame.frame_id` |
| `src/runtime/src/interp/exec_object.rs` | MODIFY | `unbox_struct`/`copy_array_elem_out` 调用点改传 `frame.frame_id`（配合 `*_val` 重构） |
| `src/runtime/src/interp/mod.rs` | MODIFY | `exec_struct` / `exec_array` 模块可见性提为 `pub(crate)`（供 JIT 复用 `*_val`/`try_struct_backed`）；OSR `from_interp_regs` 处继承 interp 帧 frame_id |
| `src/tests/types/struct_jit.z42` | NEW | JIT 模式 golden：本地 struct + 嵌套 + struct[] + 装箱拆箱综合用例 |
| `docs/book/src/runtime/struct-value-semantics.md` | MODIFY | 加「JIT 值路径」节（helper 桥接机制 + frame_id + Deferred 原生内联） |
| `docs/roadmap.md` | MODIFY | Deferred 索引加「P5-B JIT 原生内联字节访问」条 |

**只读引用**（理解上下文，不修改）：

- `src/runtime/src/interp/struct_arena.rs` — arena API（alloc/copy_into/with/get_ref）
- `src/runtime/src/interp/exec_array.rs` — interp `array_get` 的 StructRefHeap 产出参照
- `src/runtime/src/vm_context.rs` — `next_frame_id()` / `push_frame`/`pop_frame` 的 struct_base 管理
- `src/runtime/src/metadata/types.rs` — `Value::StructRef`/`StructRefHeap`/`BoxedStruct` 定义

## Out of Scope

- **P5-B 原生内联字节访问**（FieldGetPrim/SetPrim 直接 emit cranelift load/store 到 arena 字节，
  跳过 helper call）→ Deferred（design.md Deferred 段 + roadmap 索引）。
- **AOT 值路径** —— 仍 interp/JIT 优先，AOT 不在本变更。
- **格式变更** —— 本变更零 zbc/zpkg 格式改动（无新指令、无新字段），无 bump、无两代自举。
- **JIT 栈分配 struct**（把非逃逸 struct 放 JIT frame-local arena 而非 per-context arena）——
  沿用现有 per-context arena，不引入 JIT frame-local struct arena。

## Open Questions

- [ ] 无（方案 A 已经 User 裁决；实现细节 design.md 已定）
