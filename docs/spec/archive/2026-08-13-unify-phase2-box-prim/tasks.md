# Tasks: unify Phase 2 —— R3 装箱统一（基元 → 堆 ScriptObject）

> 状态：🟢 已完成 | 完成：2026-08-13
> 纯 runtime；spec-first（vm 变更）：DRAFT → **User 确认（D1/D2/D3 拍板）** → IMPL → GREEN → PR。
> 环境：worktree z42-uvt，分支 `unify-phase2-box-prim`（基于 origin/main ed3fdcbe，格式 1.32/0.37）。

## 进度概览
- [x] 阶段 0: DRAFT + User 确认 D1（D1-B struct_bytes 存标量）/ D2（type_desc 来源）/ 目标（全对齐 C# 引用身份）
- [x] 阶段 1: box 侧 —— `MagrGC::alloc_boxed_prim`（堆 ScriptObject，标量存 struct_bytes）+ `box_prim_to_heap` + `builtin_box_prim` 改产 `Value::BoxedStruct` → `cargo test --lib`
- [x] 阶段 2: 逐个收敛 `Value::Boxed(b)` 双写 helper 臂（每改 `cargo test --lib`）
- [x] 阶段 3: 删 `Value::Boxed(Box<BoxedPrim>) = 13` 变体 + `BoxedPrim` struct；`grep` 清零
- [x] 阶段 4: cargo `--lib` boxed-prim 单测 + golden e2e（引用身份/GetType/ToString/is·as/roundtrip）
- [x] 阶段 5: 全量 `xtask test` GREEN + self-host 5/5 逐字节 + 无格式 bump → 归档 + PR

## 阶段 1: box 侧堆装箱
- [x] 1.1 `gc/heap.rs`：`MagrGC::alloc_boxed_prim(type_desc, struct_bytes)` trait 方法（默认回落 alloc_object）
- [x] 1.2 `gc/arc_heap.rs`：`ArcMagrGC::alloc_boxed_prim` 覆盖（struct_bytes 按标量宽度定尺，不走 inline_regions）+ 提取 `finish_alloc` 共享尾部
- [x] 1.3 `metadata/well_known_names.rs`：`int_wrapper_scalar_spec(name) -> Option<(width, signed)>`
- [x] 1.4 `corelib/convert.rs`：`box_prim_to_heap` + `builtin_box_prim` 改产 `BoxedStruct`
- [x] 1.5 `metadata/types.rs`：`ScriptObject::boxed_prim_i64()` 拆箱（宽度+符号还原）

## 阶段 2: 消费点收敛（Value::Boxed → BoxedStruct-prim）
- [x] 2.1 拆箱 `exec_object.rs` as_cast（基元盒→裸标量 / struct 盒→arena StructRef）+ is_instance 合并臂
- [x] 2.2 `convert_value`（`exec_value.rs`）：`(T)o` 拆箱
- [x] 2.3 `convert.rs` `value_to_str`（基元盒→标量字符串，修 `WriteLine(object)` 打印）+ `arg_i64` 透明拆箱
- [x] 2.4 `reflection.rs` SetValue 拆箱 / `object.rs` GetType（BoxedStruct 臂覆盖）
- [x] 2.5 `types.rs` GC visit（BoxedStruct 覆盖）/ equality（装箱整数 vs 裸整数透明拆箱臂）
- [x] 2.6 `repl.rs` 类名（type_desc.name）
- [x] 2.7 `exec_vcall.rs` + `jit/helpers/vcall.rs` 基元盒方法派发（this=裸标量，guard 与 struct 盒分流）
- [x] 2.8 `jit/helpers/object.rs` is/as 镜像 interp
- [x] 2.9 `arc_heap.rs` object_size_bytes（删 Boxed 臂，BoxedStruct 覆盖）

## 阶段 3: 删变体
- [x] 3.1 删 `Value::Boxed(Box<BoxedPrim>) = 13` + `BoxedPrim`（判别号 13 留空，14-18 不重编）
- [x] 3.2 `grep -rn "Value::Boxed\b\|BoxedPrim" src/runtime/src` 清零（仅注释残留）

## 阶段 4/5: 验证
- [x] 4.1 cargo `--lib`：`boxed_prim_i64` 宽度+符号 round-trip 单测（types_tests.rs）
- [x] 4.2 golden e2e：`types/boxed_primitive_is_as`、`types/box_unbox`（interp+jit 全绿）
- [x] 5.1 全量 `xtask test` GREEN（C#-free，all stages）
- [x] 5.2 self-host 不动点 5/5 gen1==gen2 逐字节（编译器不动）
- [x] 5.3 无格式 bump（zbc/zpkg minor 常量未动）
- [x] 5.4 文档同步：`docs/book/src/runtime/struct-value-semantics.md` 基元装箱统一节

## 备注
- **D1-B 落地零格式 bump**：wrapper phantom struct（size 0）的 struct_layout 未动；boxed-prim `struct_bytes`
  由 `alloc_boxed_prim` 运行期按标量宽度定尺，绕过 `inline_regions()`。
- **实施踩坑**：`value_to_str` 的 `Value::Boxed` 臂删除后未给 BoxedStruct 臂补基元标量字符串化 → `WriteLine(object)`
  装箱 int 打印 `Std.Int32{...}`（box_unbox 红）；补 `boxed_prim_i64().map(scalar-str)` 后修复。
