# vm — 虚拟机

## 职责

组合每 VM 与每线程状态，提供加载、运行时链接和执行协议。`Vm` 按执行模式调用已注册引擎；`VmContext` 是固定地址的每线程视图，线程共享同一 `Arc<VmCore>`。具体引擎、builtin 与扩展由 crate 门面装配。

## 功能索引

| 入口 | 职责 |
|---|---|
| `mod.rs` | `Vm` 执行入口，导出 `VmContext` 与 `VmCore` |
| `context/` | 共享 Core 与线程视图、worker 加入、帧/查找/静态字段访问与生命周期 |
| `diagnostics/` | 计数、运行时事件、观察者与共享诊断状态 |
| `threads/` | 线程注册、park、safepoint 与暂停握手 |
| `runtime/` | 运行时身份、类型、静态存储与链接缓存 |
| `loader/` | 包定位、加载上下文、惰性符号加载与初始化队列 |
| `exec/` | 引擎共享调用、指令语义、异常、栈与栈保护 |
| `config.rs` / `ext.rs` | 构造输入与创建前登记、冻结的扩展状态 |

## 基础用法

Rust 消费方从 `z42::vm::{Vm, VmContext}` 获取入口类型。构造携带 module 的上下文，准备依赖搜索路径后交给 `Vm::run`；宿主 C ABI 继续由 `host` 提供。偏移常量、注册表指针等上下文细节位于 `vm::context`。

组件协议、工厂装载和实例管理由相邻的 [`components`](../components/README.md) 模块统一实现；host 发起装配，实例创建完成后一次发布到 VmCore.components。worker 共享该注册表。核心不持有 host 选项；接口租约保留所属 Core，实例销毁完成前库和服务上下文保持存活。

## 如何测试验证

仓库根运行 `./xtask test` 与 `./xtask test runtime`。定向检查：

```bash
(cd src/runtime && cargo test --lib vm::)
(cd src/runtime && cargo check --workspace --no-default-features)
```

## 关联文档

- [VM 总体架构](../../../../docs/internals/src/runtime/vm-architecture.md)
- [组件化运行时](../../../../docs/internals/src/runtime/componentized-runtime.md)

## 核心文件

`mod.rs` 汇总 VM 入口与各子系统；`context/types.rs` 保留 Core/Context 的状态布局，`exec/` 与 `diagnostics/` 分别持有执行和观察实现。
