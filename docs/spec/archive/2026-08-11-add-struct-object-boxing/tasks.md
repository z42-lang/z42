# Tasks: struct→object 健全装箱 + 身份（PR2a）

> 状态：🟢 已完成 | 创建：2026-08-11 | 完成：2026-08-11
> 「struct 值类型完备化」工作流 PR2a。PR2b（合成 Equals/GetHashCode/ToString）后续。

## 进度概览
- [x] 阶段 1: 运行时 boxed-struct 表示 + GC
- [x] 阶段 2: 装箱 / 拆箱 / 身份（VM + 编译器）
- [x] 阶段 3: 测试 + 验证 + 文档

## 阶段 1: 运行时表示（`src/runtime`）
- [x] 1.1 `types.rs`：`Value::BoxedStruct(Box<BoxedStruct>)` 变体 + `BoxedStruct{type_name:Arc<str>, bytes:Box<[u8]>, refs:Box<[Value]>}`；补 Debug
- [x] 1.2 GC trace/scan 加 `BoxedStruct` 分支（遍历 `refs` 递归 trace）
- [x] 1.3 `PartialEq` 加 `BoxedStruct` provisional 值相等分支（type_name ∧ bytes ∧ refs；design D5，2b 复议）

## 阶段 2: 装箱/拆箱/身份
- [x] 2.1 `corelib/convert.rs` `builtin_box_struct(ctx,args)`：读 StructRef arena slot → 拷 bytes+clone refs → `BoxedStruct`（幂等）；`corelib/mod.rs` 注册 `__box_struct`
- [x] 2.2 编译器 `Bound.z42` `BoundBox` 加装箱 kind（prim/struct）；`TypeChecker.BoxIfNeeded` 扩（blob struct 擦除 object/iface → BoundBox struct kind）
- [x] 2.3 `ExprEmitter._emitBox` struct kind → `ConstStr(structFQ)` + `Builtin(dst,"__box_struct",[handle,cls])`
- [x] 2.4 拆箱：`exec_object.rs::as_cast` + `exec_value.rs` convert 加 `BoxedStruct`→blob struct 分支（alloc 当前帧 arena slot + 拷 bytes/refs → StructRef）
- [x] 2.5 身份：`exec_object.rs::is_instance` + `corelib/object.rs::builtin_obj_get_type` 加 `BoxedStruct` 分支（is object/P、GetType=type_name、as P 拆箱/as object 保持/else Null）

## 阶段 3: 测试 + 验证 + 文档
- [x] 3.1 Rust 单测：box→arena truncate 后不 stale + 装箱快照独立性（`exec_struct_tests.rs`）
- [x] 3.2 golden `src/tests/types/struct_boxing.z42`：跨帧存活 GetType/is/as + 拆箱值独立 + string 叶子保真（断言自检）
- [x] 3.3 `cargo build --release`（VM）+ **`cargo test --lib`**（VM 改动必跑，[[xtask-test-excludes-cargo-test]]）
- [x] 3.4 完整 `xtask test` GREEN（不传 `Z42_HOME`）+ self-host 5/5
- [x] 3.5 spec scenarios 逐条覆盖确认
- [x] 3.6 `docs/book/src/runtime/struct-value-semantics.md`：加「struct→object 装箱」小节 + Deferred 更新
- [x] 3.7 归档 + PR

## 备注
- **无格式 bump**（`__box_struct` 复用 Builtin 0x51）——warm 环境（z42-svs4，seed z42sdk2 0.36）继续可用。
- Out of scope（→2b）：boxed struct 的 Equals/GetHashCode/ToString（合成值方法）；`==` on boxed 最终语义（D5）。
