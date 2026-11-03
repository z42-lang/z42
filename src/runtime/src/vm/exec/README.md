# exec/

## 职责

保存引擎共享的调用状态、栈读取和指令执行语义。标量运算从 object 的 Value 取值，遵循统一的算术、比较与转换规则。

## 功能索引

| 入口 | 职责 |
|---|---|
| `ambient.rs` | 执行期当前堆 TLS、HeapGuard 嵌套恢复和堆 epoch |
| `engine.rs` | Engine / TierApi 注册、共享调用边界和 OSR 交接；门面按 feature 装配，后端持有代码及激活；corelib 的反射、attribute 和线程回调也经此入口 |
| `ops/call.rs` / `ops/stringify.rs` | 冷 Call/FnRef 绑定及 ToString 协议执行，调用经已注册 Engine |
| `call/` | 统一 VmFrame / FrameStack、寄存器池和三种帧作用域 arena |
| `call/outcome.rs` / `call/forward.rs` / `call/type_param_env.rs` | 共享执行结果、Native 桩折叠和激活类型实参 |
| `ops/structs.rs` | struct 分配、复制、拆箱和符号化字段检查，两个后端仅保留帧适配 |
| `stack.rs` | 栈快照、栈迹格式与根遍历；沿用所属线程 / 暂停所属线程的读取约束 |
| `ops/vcall.rs` / `ops/obj_new.rs` / `ops/cast.rs` | 共享派发、构造与类型判定的加载/初始化/异常编排，纯事实查询来自 runtime/dispatch |
| `ops/scalar.rs` | 值级算术、比较、整数除余、数值转换与错误判据 |

## 基础用法

解释器和 JIT helper 从 `z42::vm::exec::ops::scalar` 调用共享函数；JIT 内联发码遵循相同规则，由双模式 golden 验证。异常对象的构造和传播由调用方处理。

## 如何测试验证

仓库根运行 `./xtask test runtime` 与 `./xtask test e2e`。定向验证：

```bash
(cd src/runtime && cargo test --workspace vm::exec::ops::scalar:: -- --test-threads=1)
```

## 关联文档

共享语义和后端一致性见 [解释器与 JIT 语义](../../../../../docs/internals/src/runtime/interp-jit-semantics.md)。

## 核心文件

`mod.rs` 汇总 call、stack 与 ops；`ops/scalar.rs` 与同目录的 `scalar_tests.rs` 保存实现和原有边界测试。
