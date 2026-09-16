# include/ — Public C headers

## 职责

z42 runtime 对外暴露的 C ABI 公开头文件。供 host 应用 / native 库通过 `-I` include 路径引用。

## 核心文件

| 文件 | 职责 |
|------|------|
| `z42_abi.h` | Tier 1 native interop ABI（`Z42TypeDescriptor_v1` + `z42_*` 函数声明）。Rust 镜像见 [`crates/z42-abi/`](../crates/z42-abi/) |
| `z42_host.h` | Tier 1 embedding / hosting ABI（`Z42HostConfig` + `z42_host_*` 函数声明）。`#include "z42_abi.h"` 复用 `Z42Value` / `Z42Args` / `Z42Error`。Spec 见 [`docs/internals/src/runtime/embedding.md`](../../../docs/internals/src/runtime/embedding.md) |

## 入口点

`#include "z42_abi.h"`（native interop / 注册类型）：

- 类型：`Z42TypeDescriptor_v1`、`Z42MethodDesc`、`Z42FieldDesc`、`Z42TraitImpl`
- 常量：`Z42_ABI_VERSION` + `Z42_TYPE_FLAG_*` / `Z42_METHOD_FLAG_*` / `Z42_FIELD_FLAG_*`
- 函数：`z42_register_type` / `z42_resolve_type` / `z42_invoke` / `z42_invoke_method` / `z42_last_error`

`#include "z42_host.h"`（embedding / 启动 VM）：

- 类型：`Z42HostConfig`、`Z42HostStatus`、`Z42ExecMode`、`Z42WriteSink`、`Z42ZpkgResolverFn`
- 句柄：`Z42HostRef`、`Z42ModuleRef`、`Z42EntryRef`
- 常量：`Z42_HOST_ABI_VERSION`
- 函数：`z42_host_initialize` / `z42_host_load_zbc` / `z42_host_resolve_entry` / `z42_host_invoke` / `z42_host_set_stdout_sink` / `z42_host_set_stderr_sink` / `z42_host_last_error` / `z42_host_shutdown`

## ABI 演进

`abi_version` 字段永远在偏移 0；新字段只追加，major 版本升级 = 显式 break。两份头文件遵循同一规则（`z42_abi.h` 详见 `docs/design/language/interop.md` §3.3；`z42_host.h` 详见 `docs/internals/src/runtime/embedding.md` §4.5）。
