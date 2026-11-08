# z42-component-api — 组件协议

## 职责

定义同构建运行时组件使用的 C ABI：描述符、版本和构建身份、不透明句柄、接口规格及核心服务表。库为 no_std，不依赖 VM、GC 或任何执行后端。

## 功能索引

| 入口 | 职责 |
|---|---|
| `ComponentDescriptor` / `ComponentInit` | 不可变工厂、依赖、接口声明与每 VM 创建/销毁 |
| `ApiHeader` / `InterfaceSpec` | 版本、大小和对齐校验 |
| `ComponentInterface` / `InterfaceRef` | 类型化表的编译期契约与 ABI 借用视图 |
| `RuntimeApi` / `VmInfoApi` | 核心服务和已声明依赖的查询、稳定 VM 身份 |
| `build.rs` | 按 workspace Rust 源码、清单/锁文件、工具链、目标与目标特性生成构建身份 |

## 基础用法

组件的 init 返回静态描述符；create_vm 接收借用的 RuntimeApi 与 VmHandle，返回组件拥有的状态。组件通过不透明句柄查询核心服务和已声明依赖，不静态包含运行时实现。

接口表必须采用 C 布局，首字段为 ApiHeader。每个表通过 unsafe ComponentInterface 实现声明 ID 和版本，再用 InterfaceSpec::of 发布大小和对齐。指针、回调、状态并发和存活期由组件保证，不能跨边界传递 Rust 所有权或 unwind。创建失败时，非空输出也必须可被 destroy_vm 清理；成功的无状态组件可以使用空句柄。

嵌入宿主从 z42::host 使用协议类型和 ComponentOptions；组件实现直接依赖本库。加载和注册实现不属于本库。

## 如何测试验证

在 src/runtime/ 运行 `cargo test -p z42-component-api`。真实静态/动态协议测试随 `cargo test --lib host::component_tests` 执行。

## 关联文档

- [组件化运行时](../../../../docs/internals/src/runtime/componentized-runtime.md)
- [嵌入机制](../../../../docs/internals/src/runtime/embedding.md)

## 核心文件

src/lib.rs 定义协议，src/api_tests.rs 验证公共 ABI 形状。构建身份要求同源、同工具链及目标；跨版本组件不兼容。
