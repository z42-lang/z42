# 运行时 C SDK

在使用 z42 Runtime 的 C 或 C++ 宿主中，统一从 `z42.h` 引入接口。Runtime 包在 `native/include/` 提供公开头文件。

```c
#include <z42.h>
```

## 接口入口

| 文件 | 职责 |
|------|------|
| `z42.h` | 统一入口，汇总宿主、原生互操作、模块注册和组件组装声明 |
| `z42_host.h` | 初始化宿主、加载程序、解析入口、调用和退出，可独立包含 |
| `z42_abi.h` | 原生类型描述符、方法签名和值类型的 C ABI |
| `z42_native.h` | 原生模块、模块函数及提供方登记的 C ABI |

## 配置与组装

各平台共用 `Z42HostConfig`。使用头文件提供的 `Z42_HOST_ABI_VERSION`，将 `struct_size` 设为调用方的 `sizeof(Z42HostConfig)`。运行时依据 `Z42_HOST_CONFIG_MIN_SIZE` 校验必需前缀，并按调用方提供的尺寸读取可选尾部字段。`file_resolver` 按逻辑文件名提供程序依赖及启动配置，通过 `z42_file_write` 同步交付字节。

自行组装静态 Runtime 时，可通过 `z42.h` 登记执行组件和原生模块。组包时，`z42_components.h` 与 `z42_compression.h` 的声明随 `z42.h` 发布。各 workload 说明目标平台提供的执行模式、组件和链接方式。

## 关联文档

- [嵌入 C ABI](../../../docs/reference/src/embedding/c-abi.md)
- [原生互操作契约](../../../docs/reference/src/embedding/native-interop.md)
- [平台 workload](../../toolchain/workload/README.md) · [打包机制](../../../docs/internals/src/devinfra/packaging.md)
