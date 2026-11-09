# context — VM 共享状态与线程视图

## 职责

定义 `VmCore`、`VmContext`、上下文偏移与注册指针。Core 组合各子系统的共享状态；每条执行线程持有固定地址的上下文，并按注册表锁和暂停协议暴露其根与栈。

## 功能索引

| 文件 | 职责 |
|---|---|
| `types.rs` | Core/Context 状态、repr 与偏移常量 |
| `construct.rs` | worker 共享、线程注册、Drop 与诊断计数转移 |
| `frames.rs` | 帧读写、根扫描、异常槽和栈快照访问 |
| `lookup.rs` / `symres.rs` | 上下文查找与缺符号判定 |
| `statics.rs` | 静态字段和初始化访问 |
| `extensions.rs` | 创建前登记与冻结的每 VM 扩展状态；默认资源装配由 host 提供 |
| `native.rs` | 反射重入的待传播异常值 |

## 基础用法

入口类型从 `z42::host::{VmCore, VmContext}` 获取。主上下文构造装配由 crate 门面实现；worker 用 `new_with_core` 共享现有 Core。跨线程读取帧前须遵守所属线程或暂停线程约束。

## 如何测试验证

```bash
./xtask test runtime
(cd src/runtime && cargo test --lib context::)
```

## 关联文档

- [VM 总体架构](../../../../docs/internals/src/runtime/vm-architecture.md)
- [组件化运行时](../../../../docs/internals/src/runtime/componentized-runtime.md)

## 核心文件

`mod.rs` 定义注册表和指针，`types.rs` 保存状态布局；`vm_context_tests.rs` 和 `symres_tests.rs` 保留上下文与符号判定覆盖。
