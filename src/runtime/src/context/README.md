# context — VM 共享状态与线程视图

## 职责

定义 `VmCore`、`VmContext`、上下文偏移与注册指针。Core 组合各子系统的共享状态；每条执行线程持有固定地址的上下文，并按注册表锁和暂停协议暴露其根与栈。

## 功能索引

| 文件 | 职责 |
|---|---|
| `types.rs` | Core/Context 状态、repr 与偏移常量 |
| `core_builder.rs` | 从显式输入构造 Core，连接 GC 服务并通过统一线程入口创建主线程上下文 |
| `thread_lifecycle.rs` | 主线程/worker 的共同注册机制、Drop 与诊断计数转移 |
| `access.rs` / `observation.rs` | 状态借用、计数聚合与事件观察 |
| `frames.rs` | 帧读写、根扫描、异常槽和栈快照访问 |
| `lookup.rs` | 类型/函数查找、待加载类型队列与加载编排；待按 loader/metadata 所有权拆分 |
| `statics.rs` | 静态字段和初始化访问 |
| `extensions.rs` | 创建前登记与冻结的每 VM 扩展状态；默认资源装配由 host 提供 |

## 基础用法

入口类型从 `z42::host::{VmCore, VmContext}` 获取。主上下文构造装配由 crate 门面实现；worker 用 `new_with_core` 共享现有 Core。跨线程读取帧前须遵守所属线程或暂停线程约束。
对象协议的不可变表由 Core 持有，worker 借用同一服务，不放入执行状态。Core 的访问入口仅 crate 内可见；同构建后端通过 `components::bridge` 读取所需服务。
缺符号异常归 `exec/exception/symbol_resolution.rs`，静态默认值物化归 `exec/ops/object/static_defaults.rs`。
JIT 和反射/原生回调的异常交接使用 `ctx.exec()` 的线程状态，不再通过 VmContext 转发。

## 如何测试验证

```bash
./xtask test runtime
(cd src/runtime && cargo test --lib context::)
```

## 关联文档

- [VM 总体架构](../../../../docs/internals/src/runtime/vm-architecture.md)
- [组件化运行时](../../../../docs/internals/src/runtime/componentized-runtime.md)

## 核心文件

`mod.rs` 定义注册表和指针，`types.rs` 保存状态布局；`vm_context_tests.rs` 保留上下文覆盖，符号判定测试随执行语义归 exec/exception。
