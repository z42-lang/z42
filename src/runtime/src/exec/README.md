# exec/

## 职责

保存引擎共享的调用状态、栈读取和指令执行语义。标量运算从 object 的 Value 取值，遵循统一的算术、比较与转换规则。无上下文的值格式化和盒装值哈希直接消费 object/value 原语；ToString 协议执行和异常编排仍由执行层承接。

## 功能索引

| 入口 | 职责 |
|---|---|
| `exception.rs` | 异常对象构造、传播和栈迹捕获；解释器与 JIT 保持各自的传播通道 |
| `symbol_resolution.rs` | 解析穷尽后的缺符号/歧义/实参数错误裁决与托管异常，不归上下文状态模块 |
| `ops/object/static_defaults.rs` | 静态字段声明类型的零值与值类型物化，需要加载和 GC 的执行语义 |
| `stack_guard.rs` | 原生栈余量、致命标记与报告；两个后端共用 |
| `init.rs` | 类型与包初始化屏障、执行、parked 等待及异常构造 |
| `thread_state.rs` | `ThreadExecState` 的帧、池、arena、异常槽与 JIT 中立头；每线程独立 |
| `state.rs` | `ExecState` 组合每 VM 引擎注册表、builtin 目录与栈分配策略；对象协议由 Core 单独持有 |
| `native.rs` | native 调用与 owned pin 提供方契约；门面在创建 VM 前登记，worker 共享 |
| `active_vm.rs` | native-interop 回调的活动 VM TLS 与激活 guard；底帧与宿主重入共用，退出时恢复前一上下文 |
| `engine.rs` | Engine / TierApi 注册、共享调用边界和 OSR 交接；门面按 feature 装配，后端持有代码及激活；corelib 的反射、attribute 和线程回调也经此入口 |
| `ops/call.rs` / `ops/stringify.rs` | 冷 Call/FnRef 绑定及 ToString 协议执行，调用经已注册 Engine |
| `call/entry.rs` | 程序入口参数校验、GC 根保护、引擎派发与类型化调用失败 |
| `call/engine_guards.rs` | 栈底激活安装 VM / heap TLS，最后一帧退出恢复；支持嵌套宿主重入 |
| `call/`（经 exec 根再导出） | 统一 VmFrame / FrameStack、寄存器池和三种帧作用域 arena；消费路径为 `exec::{frames, frame_stack, reg_pool, ...}`，`call` 本身 crate 内可见 |
| `call/builtins.rs` | 按 ID / 名称派发 builtin，按 `(provider, export)` 派发模块函数；共享计数、异常与 void 语义，门面提供 callable 表 |
| `call/builtin_abi/` | builtin 参数、返回值、阻塞调用协议及错误映射；corelib 与 interop 只提供实现 |
| `call/outcome.rs` / `call/forward.rs` / `call/type_param_env.rs` | 共享执行结果、Native 桩折叠和激活类型实参 |
| `ops/structs.rs` | struct 分配、复制、拆箱和符号化字段检查，两个后端仅保留帧适配 |
| `stack.rs` | 栈快照、栈迹格式与根遍历；沿用所属线程 / 暂停所属线程的读取约束 |
| `ops/vcall.rs` / `ops/obj_new.rs` / `ops/cast.rs` | 共享派发、构造与类型判定的加载/初始化/异常编排，纯事实查询来自 runtime/dispatch |
| `ops/references.rs` | ref 位置的读取与写回；解释器和 builtin ABI 共用，数组/字段委托 objops 屏障 |
| `ops/scalar.rs` | 值级算术、比较、整数除余、数值转换与错误判据 |

## 基础用法

跨领域与独立引擎通过 `exec::{EngineRegistry, Engine, TierBackend, Initialization}` 及 `exec::invoke` 消费契约。`engine.rs`、`init.rs` 为私有实现，移动它们不改变消费路径。

解释器和 JIT helper 从 `z42::exec::ops::scalar` 调用共享函数；JIT 内联发码遵循相同规则，由双模式 golden 验证。异常对象的构造和传播由调用方处理。
对象能力契约位于 `object/protocol.rs`；执行层借用 Core 已绑定的服务，保留托管调用、异常和文本物化。
堆字段、数组及 struct 写入和插入屏障由 object 根接口提供；exec 负责 IL 检查、缓存、类型解析和帧适配。struct 装箱传入已解析的 TypeDesc，快照与存储地址查询直接消费 object。

## 如何测试验证

仓库根运行 `./xtask test runtime` 与 `./xtask test e2e`。定向验证：

```bash
(cd src/runtime && cargo test --workspace exec::ops::scalar:: -- --test-threads=1)
```

## 关联文档

共享语义和后端一致性见 [解释器与 JIT 语义](../../../../docs/internals/src/runtime/interp-jit-semantics.md)。

## 核心文件

`mod.rs` 汇总 call、stack 与 ops；`ops/scalar.rs` 与同目录的 `scalar_tests.rs` 保存实现和原有边界测试。

栈分配开关由门面构造时提供的每 VM 策略决定。Rust 辅助函数 `call::stack_alloc::stack_alloc_enabled` 与 `stats_enabled` 显式接收 `&VmContext`；worker 复用 Core 的策略，执行层不解析配置字符串。
