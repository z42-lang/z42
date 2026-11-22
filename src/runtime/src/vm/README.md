# vm — VM 组合与装配

## 职责

组合共享 Core 与固定地址的每线程视图，管理线程加入/退出及状态寿命，接收宿主装配的服务与组件输入。默认服务由上层组合；执行、加载、对象、诊断等领域机制由各自模块持有。

## 功能索引

| 文件 | 职责 |
|---|---|
| `mod.rs` | VmCore/VmContext、布局常量、线程注册与销毁、最小状态借用入口 |
| `wiring.rs` | Core 构造后连接 GC、根扫描、回收与观察回调 |
| `assembly.rs` | 已加载程序的统一准备顺序、内部组件输入和失败发布边界 |
| `defaults.rs` | 捕获默认配置、准备 Core 与上下文输入 |
| `extensions.rs` | 创建前登记并冻结的扩展状态及逆序释放 |

## 基础用法

内部消费者从 `crate::vm::{VmCore, VmContext, VmExtensions}` 导入。装配子模块保持 crate 内部可见；普通宿主使用 `host::LoadedProgram`，独立组件通过 `components::bridge` 获取同构建协议所需的最小类型和布局事实。

## 如何测试验证

在运行时目录使用 `cargo z42-test-dyn -p z42 --lib --no-default-features --features desktop-dynamic vm:: -- --test-threads=1`；仓库根运行完整 `./xtask test`。稳定消费路径另由库 doctest 验证。

## 关联文档

- [VM 总体架构](../../../../docs/internals/src/runtime/vm-architecture.md)
- [运行时模块边界](../../../../docs/internals/src/runtime/module-boundaries.md)

## 核心文件

`mod.rs` 维护组合与生命周期入口；`tests.rs`、`defaults_tests.rs`、`extensions_tests.rs` 覆盖 VM 隔离、worker 共享和扩展销毁契约。
