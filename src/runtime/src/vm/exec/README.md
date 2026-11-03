# exec/

## 职责

保存引擎共享的调用状态、栈读取和指令执行语义。标量运算从 object 的 Value 取值，遵循统一的算术、比较与转换规则。

## 功能索引

| 入口 | 职责 |
|---|---|
| `ambient.rs` | 执行期当前堆 TLS、HeapGuard 嵌套恢复和堆 epoch |
| `call/` | 统一 VmFrame / FrameStack、寄存器池和三种帧作用域 arena |
| `stack.rs` | 栈快照、栈迹格式与根遍历；沿用所属线程 / 暂停所属线程的读取约束 |
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
