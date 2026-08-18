# native/ — Tier 1 C ABI runtime

## 职责

实现 z42 native interop **Tier 1 C ABI** 的运行时部分（spec C2 `impl-tier1-c-abi`）。提供 `z42_register_type` / `z42_resolve_type` / `z42_invoke` / `z42_invoke_method` / `z42_last_error` 五个 `extern "C"` 入口，把 `Instruction::CallNative` IR opcode 接到 libffi 调度路径，让 native 库（C / Rust / 任意能产 C ABI 的语言）可以注册类型并被 z42 用户代码调用。

> 用户面向的 syntax（`[Native]` / `extern class` / `import T from "lib"`）是 spec C5 的工作；本目录只解决"接口通电"，C2 测试通过 hand-crafted bytecode + 集成测试验证。

## 核心文件

| 文件 | 职责 |
|------|------|
| `mod.rs` | 模块入口；re-export 公开 API |
| `registry.rs` | `RegisteredType` + `MethodEntry`：解析 `Z42TypeDescriptor_v1`，预构建 libffi `Cif` |
| `marshal.rs` | z42 `Value` ↔ `Z42Value` 双向转换（C2 blittable 子集）|
| `dispatch.rs` | `SigType` 枚举 + 签名解析 + libffi `Cif::new` / `Cif::call` 包装 |
| `loader.rs` | `dlopen`：`libloading::Library::new(path)` + 调用 `<basename>_register` 入口 |
| `error.rs` | thread-local `LAST_ERROR` 槽 + `z42_last_error()` 实现 |
| `exports.rs` | `#[no_mangle]` `z42_*` extern 函数体 + thread-local `CURRENT_VM` + `VmGuard` RAII |
| `*_tests.rs` | 单元测试（registry / marshal / dispatch）|

## 入口点

- `z42_abi::z42_register_type` / `z42_resolve_type` / `z42_invoke` / `z42_last_error`：本目录 `exports.rs` 提供 `#[no_mangle]` 实现
- `VmContext::register_native_type` / `resolve_native_type` / `load_native_library`：z42 内部调用入口
- `Instruction::CallNative` IR dispatch：在 `interp/exec_instr.rs` 调 `RegisteredType::method(symbol)` + `dispatch::call`

## 状态

C2 接口 + 单元测试落地。

- ✅ `z42_register_type` 完整实现（abi 校验、descriptor 解析、cif 预构建）
- ✅ `z42_resolve_type` 完整实现
- ✅ `z42_last_error` thread-local 槽
- ✅ libffi 调度（`Cif::call<R>` 多返回类型分发）
- ✅ marshal 双向（i8..i64 / u8..u64 / f32 / f64 / bool / null / pointer）
- ✅ `CallNative` interp dispatch（取代 C1 trap）
- ✅ `PinPtr` / `UnpinPtr` interp dispatch + `Value::PinnedView` (spec C4 2026-04-29)
- ✅ `PinnedView` 经 `FieldGet view,"ptr"/"len"`（`exec_object::field_get`，从 per-context transient
  arena 解析）投成标量，再由 `marshal::value_to_z42` 投 `*const u8` / `usize`
  （make-value-copy：`PinnedView` 现是 transient-arena 句柄；`value_to_z42` 无 `ctx`，其直接接 raw view
  的旧防御路径退化为错误——source-gen 始终先 `FieldGet` 再传标量，见 `docs/design/runtime/object-abi.md` §2.2）
- 🟡 `z42_invoke` / `z42_invoke_method`：spec C5 接入（reverse-call 是 source generator 一起的工作）
- 🟡 `Z42_VALUE_TAG_STR` / `OBJECT` / `TYPEREF`：tag 已冻结，marshal 路径在 C5 接入

## 依赖关系

- 上：`crate::interp::exec_instr` 调 `CallNative` 分支
- 下：
  - `z42_abi` crate（ABI 类型镜像）
  - `libffi = "3.2"`（cif 构造 + dispatch）
  - `libloading = "0.8"`（dlopen 库句柄）
- 平级：`crate::vm_context::VmContext` 持有 `native_types` / `native_libs`
