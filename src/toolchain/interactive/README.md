# toolchain/interactive — z42 交互式 REPL（`z42i`）

## 职责

z42 的交互式 read-eval-print loop：读取源码片段 → 调编译器 API 即时编译 →
VM 求值 → 打印结果，维持跨输入的会话状态（已声明的变量 / 类型 / import）。

与 z42d 不同，`z42i` **不是 muxer**——它本身就是一个交互入口，无子命令。
launcher 命令分发：`z42 repl` → `z42i`。

```
src/toolchain/interactive/core/*.z42  →  z42.interactive.zpkg  →  apphost z42i
```

## 功能索引

| 功能 | 入口 / 文件 |
|------|-----------|
| 交互循环 / `-c "<expr>"` 单次求值 / `.` 元指令 | `core/interactive_main.z42` 的 `Main` |
| 求值内核（编译 → 加载 → 反射调用，跨轮变量保留） | [`z42.scripting`](../../compiler/z42.scripting/)（`Script.Eval`） |
| 终端行编辑 | [`repl/`](repl/)（`Std.Repl`） |

## 基础用法

```bash
z42 repl                  # 交互会话
z42 repl -c "1+2"         # 单次求值后退出
```

## 如何测试验证

```bash
xtask build toolchain     # 构建 z42.scripting → z42.repl → z42.interactive 并 publish z42i
xtask test dist           # 打包后 smoke 含 `z42 repl -c "1+2"`
```

## 关联文档

- 设计与机制：[REPL](../../../docs/internals/src/toolchain/repl.md)

## 核心文件（`core/`）

| 文件 | 职责 |
|------|------|
| `core/interactive_main.z42` | REPL 主循环：`Repl.ReadLine` 读入（tty 下整块多行编辑：回车判写完没 / 非 tty 逐行累积 + `Completeness.IsIncomplete`）→ `.` 元指令 / `Script.Eval` 求值；`-c` 单次求值（整块多行 + 完整性机制见 [book](../../../docs/internals/src/toolchain/repl.md)）|
| `core/z42.interactive.z42.toml` | 包清单（exe / pack / apphost）|

## 依赖关系

- 依赖 `z42.scripting`（进程内编译 + 求值）与 `repl/`（`Std.Repl` 终端层），不 fork z42c 子进程。
- 在同一 VM 实例中增量执行片段、保留会话状态。
- 被 launcher 命令分发调用。
