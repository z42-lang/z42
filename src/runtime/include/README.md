# include/ — 运行时 C 头文件

## 职责

z42 runtime 对外的全部 C ABI 声明集中在本目录，供 host 应用 / native 库通过 `-I` 引用；不再在其他目录散落副本。Rust 侧镜像：Tier 1 类型 ABI 见 [`crates/z42-abi`](../crates/z42-abi)，宿主 C 适配见 [`src/host/c`](../src/host/c)，组件登记见 [`src/components/ffi.rs`](../src/components/ffi.rs)。

## 文件

| 文件 | 职责 |
|------|------|
| `z42_host.h` | 统一发布伞头：聚合下列全部声明，native SDK 的单一 include 入口 |
| `z42_host_api.h` | Tier 1 嵌入 / 宿主 ABI（`Z42HostConfig` + `z42_host_*` 函数），经伞头发布 |
| `z42_abi.h` | Tier 1 native interop ABI（`Z42TypeDescriptor_v1` + `z42_*` 函数），单独安装 |
| `z42_native.h` | native 模块注册 ABI（`Z42NativeModule` + 登记/解析入口），单独安装 |
| `z42_compression.h` | 压缩组件的 C 符号解析器声明，经伞头发布 |
| `z42_components.h` | 最终链接组合的组件登记契约（`z42_component_register_static`），经伞头发布 |

## 发布规则

`z42_abi.h` / `z42_native.h` 作为独立头随 SDK 平铺安装；展开进伞头时 `z42_abi.h` 是唯一兄弟引用（设备宿主模板只随伞头暂存 `z42_abi.h`），`z42_native.h` / `z42_host_api.h` / `z42_compression.h` / `z42_components.h` 不单独装进伞头消费方，由安装器（`scripts/src/package/install.z42` 的 `_runtimeHeaderText`）内联，输出自包含的 `native/include/z42_host.h`。源码树与 SDK 共享同一伞头；workload 的 include 与 modulemap 不随实现目录变化。

ABI 演进：`abi_version` 永远在偏移 0；新字段只追加（宿主侧另有 `struct_size` 门控）；major 版本升级 = 显式 break（详见 [native interop](../../../docs/reference/src/embedding/native-interop.md) §3.3 与 [C ABI](../../../docs/reference/src/embedding/c-abi.md)）。

## 关联文档

- 设计 / 机制：[嵌入宿主](../../../docs/internals/src/runtime/embedding.md) · [native ABI](../../../docs/internals/src/runtime/native-abi.md) · [打包机制](../../../docs/internals/src/devinfra/packaging.md)
