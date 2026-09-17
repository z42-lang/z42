# z42-abi

## 职责

z42 Tier 1 native interop ABI 的 Rust 镜像。是 [`include/z42_abi.h`](../../include/z42_abi.h) 在 Rust 侧的权威等价定义；任何 Rust 代码（VM、Tier 2 高层 API、用户 native 库）都从这里引用 ABI 类型与函数签名。

`no_std`，无依赖；可在任意 Rust 上下文使用。

## 核心文件

| 文件 | 职责 |
|------|------|
| `src/lib.rs` | `#[repr(C)]` 镜像所有 ABI 类型；`extern "C"` 声明 `z42_*` 函数 |
| `tests/abi_layout_tests.rs` | 验证 `offset_of!` / `size_of` / 旗标常量未漂移 |

## 入口点

- 类型：`Z42TypeDescriptor_v1`、`Z42MethodDesc`、`Z42FieldDesc`、`Z42TraitImpl`、`Z42Value`、`Z42Args`、`Z42TypeRef`、`Z42Error`
- 常量：`Z42_ABI_VERSION` + `Z42_TYPE_FLAG_*` / `Z42_METHOD_FLAG_*` / `Z42_FIELD_FLAG_*`
- 函数：`z42_register_type` / `z42_resolve_type` / `z42_invoke` / `z42_invoke_method` / `z42_last_error`

## 状态

C1 接口骨架。所有 `extern "C"` 函数实现位于 `z42_vm` crate，C1 阶段返回 "not implemented" 错误（错误码 Z0905+，具体语义在 C2/C5 中钉死）。

## 依赖关系

无依赖；被 `z42-rs`、`z42_vm` 引用。

## ABI 演进规则

参见头文件注释 + `docs/reference/src/embedding/native-interop.md` §3.3：

- `abi_version` 永远在偏移 0
- 新字段只追加，不重排
- 所有访问通过 `z42_*` 函数，不直接读写 struct 字段
- major 版本升级 = 显式 break（语义版本规则）
