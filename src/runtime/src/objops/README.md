# objops — 引擎无关的对象操作

## 职责

字段、数组、静态字段、值 struct 叶子的读写语义只在这里实现一次；interp（`interp/exec_*`）与 JIT（`jit/helpers/*`）
都是薄适配层：从寄存器取 `&Value`、调本模块、写回寄存器，再把 `OpError` 映射到各自的异常通道。
对象与数组单元格的存储表示（字节布局、引用侧表、打包基元、写屏障）对引擎不可见。
不管对象分配与构造器解析（`interp/obj_new_resolve.rs`）、虚调用解析（`interp/vcall_resolve.rs`）、类型判定（`interp/dispatch.rs::isa_td`）。

## 功能索引

| 功能 | 入口 |
|------|------|
| 错误通道：异常类 + 消息文本的唯一定义，物化成异常值 | `error.rs` 的 `OpError`、`OpError::into_exception` |
| 调用的 null 接收者（`VCall` 解析、string builtin 的接收者）、builtin 的 null 实参 | `error.rs` 的 `OpError::null_call` / `null_arg` |
| builtin（返回 `anyhow`）里抛用户异常：`Throw` 原样装进 `anyhow::Error` | `error.rs` 的 `OpError::into_builtin_error`（两引擎出口：`corelib::builtin_error_exception`） |
| `FieldGet` / `FieldSet`（FieldIC、栈对象、`Length` 伪字段、`PinnedView`、装箱 struct、写屏障） | `field.rs` 的 `field_get` / `field_set` |
| JIT 提升快路的字段槽解析（不抛） | `field.rs` 的 `inline_prim_slot` / `inline_ref_slot` |
| `ref obj.f` 的接收者检查与经 ref 读写 | `field.rs` 的 `check_field_addr` / `load_named` / `store_named` |
| `ArrayGet` / `ArraySet` / `ArrayLen`（堆 / 栈数组、struct 数组元素句柄、写屏障） | `array.rs` 的 `array_get` / `array_set` / `array_len` |
| `ArrayNew` / `ArrayNewLit`（struct 数组、栈分配、OOM） | `array.rs` 的 `array_new` / `array_new_lit` |
| JIT 打包数组快路取数（不抛） | `array.rs` 的 `packed_data` |
| `Std.Array` 无类型 / 批量原生：`CopyRange`（任意 backing 间，含 struct[] 的装箱 / 拆箱 / 类型检查）、`GetValue` 的元素装箱、`SetValue` 的校验、批量写入后的逐引用槽写屏障 | `array_bulk.rs` 的 `copy_range` / `elem_get_boxed` / `check_untyped_store` / `barrier_after_range_store` |
| `ref arr[i]` 的检查与经 ref 读写 | `array.rs` 的 `check_elem_addr` / `elem_load` / `elem_store` |
| `StaticGet` / `StaticSet`（初始化屏障、惰性零值、缺符号确证） | `statics.rs` 的 `static_get` / `static_set` |
| `StructFieldGetPrim` / `StructFieldSetPrim` 的核心与 struct 快照 | `struct_leaf.rs` 的 `struct_field_get_val` / `struct_field_set_val` / `snapshot_box` / `snapshot_elem` |

## 基础用法

```rust
// interp 适配（interp/exec_object.rs）
match objops::field::field_get(ctx, frame.get(obj)?, name, ic) {
    Ok(v) => { frame.set(dst, v); Ok(None) }
    Err(e) => super::ops::raise(ctx, module, e),   // Ok(Some(exc)) 或 Err(内部错误)
}
// JIT 适配（jit/helpers/object_field.rs）
match objops::field::field_get(vm_ctx_ref(ctx), recv, name, ic) {
    Ok(v) => { (*frame).regs[dst as usize] = v; 0 }
    Err(e) => raise(ctx, e),                       // pending 异常 + 返回 1
}
```

新增一种对象 / 数组操作：在本模块写实现与单测，两个引擎各加一个适配；用户可见的异常类与消息只写在 `error.rs`。

## 如何测试验证

```bash
(cd src/runtime && cargo test --features z42-test-fixtures --lib objops)   # 本模块单测
./xtask test e2e --dir exceptions        # objops_errors.z42 / null_receiver_call.z42：interp 与 JIT 逐条对照异常类与消息
./xtask test runtime                     # 含两侧适配层的映射单测
```

## 关联文档

- 机制：[interp / JIT 语义单一真相源 · 对象操作：objops](../../../../docs/internals/src/runtime/interp-jit-semantics.md#对象操作objops)
- 对象模型改造（R0–R9）的计划：`docs/runtime-audit.md` §7「对象模型（M10 + M11）已定方案」

## 待办

- 栈数组 / 栈对象、struct 数组元素句柄仍依赖 `interp` 下的 arena（`stack_alloc` / `transient_arena`）。
- `obj_new`、闭包环境数组、反射 builtin（`FieldInfo.GetValue` / `SetValue` 等）还没经过本模块。
- `Std.Array` 的脚本泛型算法（`Fill` / `Reverse` / `IndexOf` …）在 struct[] 上不可用：擦除体里 `ArrayGet` 给的是元素句柄
  （别名），见 internals `struct-value-semantics.md` 的「待办」。

## 核心文件

| 文件 | 职责 |
|------|------|
| `mod.rs` | 模块入口与再导出 |
| `error.rs` | `OpError` / `Throw` / `ArrayOp`、消息文本、物化 |
| `field.rs` | 实例字段 |
| `array.rs` | 数组 |
| `array_bulk.rs` | `Std.Array` 的无类型 / 批量原生 |
| `statics.rs` | 静态字段 |
| `struct_leaf.rs` | 值 struct 叶子与快照 |
| `objops_tests.rs` | 单测 |
| `array_bulk_tests.rs` | `array_bulk` 单测（含老 struct[] 收年轻叶子的 minor 存活对照） |
