# 前言

这本书写给**改 z42 本身的人**（含 AI）：架构、机制流程、决策权衡，以及构建 / 测试 / 发布的操作配方。

判据是「**要动这块代码才需要读**」——只想用 z42 写程序的话，
[学习手册](https://z42-lang.github.io/z42/learn/)与[语言参考](https://z42-lang.github.io/z42/reference/)就够了。

## 系统总览

z42 的工具链是三个独立进程，互相之间只通过**文件格式**通信：

```
源码 .z42 ──▶  z42c   ──▶ .zpkg / .zbc ──▶  z42vm  ──▶ 运行
              编译器          产物格式         虚拟机
                 ▲                              ▲
                 └──────────  z42b  ────────────┘
                          构建编排 / 测试运行
                                 ▲
                              z42（launcher）
                           用户敲的那个命令，按动词转发
```

| 进程 | 语言 | 职责 | 在哪 |
|---|---|---|---|
| `z42c` | z42（**自举**） | 编译：源码 → zpkg | [编译器](compiler/README.md) |
| `z42vm` | Rust | 执行：解释器 / JIT / GC | [运行时](runtime/README.md) |
| `z42b` | z42 | 构建编排、测试运行、发布 | [工具链](toolchain/README.md) |
| `z42` | z42 | launcher，按动词转发给上面三个 | [工具链](toolchain/README.md) |

三者之间的**协议**（zbc 字节码、zpkg 包、IR）单独成部分：[产物格式](formats/README.md)——
改编译器和改 VM 的人都要查它，挂在任一侧都会让另一侧找不到。

## 各部分

| 部分 | 内容 |
|---|---|
| [编译器](compiler/README.md) | z42c：架构、编译流程、类型检查、codegen、工程模型、自举与种子 |
| [运行时](runtime/README.md) | z42vm：执行模型、解释器、JIT、GC、对象布局、加载上下文、native 扩展 |
| [产物格式](formats/README.md) | zbc / zpkg / IR —— 编译器与运行时之间的协议 |
| [标准库](stdlib/README.md) | 三层架构、包划分规则、API 准则、关键实现 |
| [工具链](toolchain/README.md) | launcher / z42b / workload / 平台发布 / REPL |
| [测试体系](testing/README.md) | 测试框架、TIDX 格式、runner 协议、跨平台测试 |
| [开发基础设施](devinfra/README.md) | 构建、测试门禁、CI、发布、调试、基准——**怎么跑** |

## 行文约定

- 页型与页头见 [book-writing.md](https://github.com/z42-lang/z42/blob/main/docs/agent/rules/book-writing.md)。
- **只写当前状态**，不写演进史（那归 git blame 与 `docs/spec/archive/`）。
- 尚未实施的设计页在页头标 **状态: 设计已定 / 未实施**。
