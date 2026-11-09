# threads/

## 职责

协调 VM mutator 的轮询、暂停、注册与恢复，并实现 GC 的根和暂停服务契约。
GC 决定回收策略；本模块保证扫描期间其他线程不会改写根。

## 功能索引

| 入口 | 职责 |
|---|---|
| `state.rs` | `ThreadLocalState` 拥有每线程 parked TLAB 与节流计数； `ThreadState`：每 VM 注册表、暂停握手、collector 仲裁与 fatal/自动回收请求标记 |
| `safepoint.rs` | 轮询、collector 仲裁、暂停握手、native park 与 RAII 恢复 |
| `roots.rs` | 共享静态值、驻留串与全部线程的异常、帧和 arena 根枚举 |
| `gc.rs` | 将当前 VmContext 的暂停协议适配为 GcToVm |

## 基础用法

mutator 调用 `threads::safepoint::check_safepoint`，需要原生阻塞区间的调用方使用同模块的
`NativeParkGuard`。GC 通过 `gc::to_vm::GcToVm` 获取暂停令牌；丢弃令牌恢复线程。
持久根提供方仅持有 Weak<VmCore>，根遍历须在暂停内进行。

## 如何测试验证

仓库根运行 `./xtask test runtime`。定向运行：

```bash
(cd src/runtime && cargo test --lib threads:: -- --test-threads=1)
```

GC 注册与 collector 仲裁的 loom 模型位于 `src/runtime/tests/gc_registration_race_loom.rs`。

## 关联文档

暂停协议见 [GC 子系统](../../../../docs/internals/src/runtime/gc.md)，
原生阻塞区间见 [原生代码与 GC](../../../../docs/internals/src/runtime/native-gc-contract.md)。

## 核心文件

`mod.rs` 汇总线程协议；`safepoint_tests.rs` 保存原有暂停、仲裁和 park 测试。

节流策略由门面解析后在 Core 创建时传入，每 VM 持有不可变值。Rust `safepoint::throttle_n(ctx)` 显式查询所属 VM，主线程和 worker 初值以及 interp/JIT 慢路径重置使用同一策略。暂停和自动回收请求仍按原协议强制轮询，不受普通倒计数延迟。
