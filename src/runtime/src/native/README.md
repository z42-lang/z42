# native/ — Tier 1 C ABI runtime

## 职责

实现 z42 native interop **Tier 1 C ABI** 的运行时部分，以及 stdlib native 扩展（`z42.compression` 等 cdylib）的加载。提供 `z42_register_type` / `z42_resolve_type` / `z42_invoke` / `z42_invoke_method` / `z42_last_error` 五个 `extern "C"` 入口，把 `Instruction::CallNative` IR opcode 接到 libffi 调度路径，让 native 库（C / Rust / 任意能产 C ABI 的语言）可以注册类型并被 z42 用户代码调用。

> 用户面向的 syntax（`extern class` / `import T from "lib"` 等）属 source generator（C5）；本目录只提供运行时接口，经 hand-crafted bytecode + 集成测试验证。

## 核心文件

| 文件 | 职责 |
|------|------|
| `mod.rs` | 模块入口；re-export 公开 API |
| `registry.rs` | `RegisteredType` + `MethodEntry`：解析 `Z42TypeDescriptor_v1`，预构建 libffi `Cif` |
| `marshal.rs` | z42 `Value` ↔ `Z42Value` 双向转换（blittable 子集）|
| `dispatch.rs` | `SigType` 枚举 + 签名解析 + libffi `Cif::new` / `Cif::call` 包装 |
| `loader.rs` | `dlopen`：`libloading::Library::new(path)` + 调用 `<basename>_register` 入口 |
| `error.rs` | thread-local `LAST_ERROR` 槽 + `z42_last_error()` 实现 |
| `ext.rs` | stdlib native 扩展加载器：启动时从 SDK native 搜索路径 dlopen `lib<basename>.{so,dylib,dll}`，把 `[Native(lib=…, entry=…)]` 引用的 `__<entry>` 注册进 `VmCore.ext_builtins`（与 Tier 1 `loader.rs` 的通用 libffi 路径互补） |
| `exports.rs` | `#[no_mangle]` `z42_*` extern 函数体 + thread-local `CURRENT_VM` + `VmGuard` RAII |
| `*_tests.rs` | 单元测试（registry / marshal / dispatch / ext）|

## 入口点

- `z42_abi::z42_register_type` / `z42_resolve_type` / `z42_invoke` / `z42_last_error`：本目录 `exports.rs` 提供 `#[no_mangle]` 实现
- `VmContext::register_native_type` / `resolve_native_type` / `load_native_library`：z42 内部调用入口
- `Instruction::CallNative` IR dispatch：在 `interp/exec_instr.rs` → `exec_native.rs` 调 `RegisteredType::method(symbol)` + `dispatch::call`

## 能力范围

- `z42_register_type`（abi 校验、descriptor 解析、cif 预构建）/ `z42_resolve_type` / `z42_last_error`（thread-local 槽）
- libffi 调度（`Cif::call<R>` 多返回类型分发）；marshal 双向（i8..i64 / u8..u64 / f32 / f64 / bool / null / pointer）
- `CallNative` interp dispatch；`PinPtr` / `UnpinPtr` + `Value::PinnedView`：经 `FieldGet view,"ptr"/"len"`（`exec_object::field_get`，从 per-context transient arena 解析）投成标量，再由 `marshal::value_to_z42` 投 `*const u8` / `usize`；`value_to_z42` 无 `ctx`，直接接 raw `PinnedView` 会报错（见 `docs/internals/src/runtime/object-abi.md` §2.2）

## 待办

- `z42_invoke` / `z42_invoke_method`：当前返回 "not implemented" 错误，reverse-call 随 source generator（C5）接入
- `Z42_VALUE_TAG_STR` / `OBJECT` / `TYPEREF`：tag 已冻结，marshal 路径待 C5 接入

## 依赖关系

- 上：`crate::interp::exec_instr` 调 `CallNative` 分支
- 下：
  - `z42_abi` crate（ABI 类型镜像）
  - `libffi = "3.2"`（cif 构造 + dispatch）
  - `libloading = "0.8"`（dlopen 库句柄）
- 平级：`crate::vm_context::VmContext` 持有 `native_types` / `native_libs`

## 如何测试验证

```bash
(cd src/runtime && cargo test --lib native)              # registry / marshal / dispatch / ext 单测
(cd src/runtime && cargo test --test native_interop_e2e)  # dlopen 端到端
```
