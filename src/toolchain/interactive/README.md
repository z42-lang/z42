# toolchain/interactive — z42 交互式 REPL（`z42i`）

## 职责

z42 的交互式 read-eval-print loop：读取源码片段 → 调编译器 API 即时编译 →
VM 求值 → 打印结果，维持跨输入的会话状态（已声明的变量 / 类型 / import）。

与 z42d 不同，`z42i` **不是 muxer**——它本身就是一个交互入口，无子命令。
launcher 命令分发：`z42 repl` → `z42i`（裸 `z42` 无参是否进 REPL 待定）。

```
src/toolchain/interactive/core/*.z42  →  z42.interactive.zpkg  →  apphost z42i
```

## 核心文件（`core/`，scaffold）

| 文件 | 职责 |
|------|------|
| `core/interactive_main.z42` | REPL 主循环：`Repl.ReadLine` 读入（tty 下整块多行编辑：回车判写完没 / 非 tty 逐行累积 + `Completeness.IsIncomplete`）→ `.` 元指令 / `Script.Eval` 求值；`-c` 单次求值（整块多行 + 完整性机制见 [book](../../../docs/book/src/toolchain/repl-input-completeness.md)）|
| `core/z42.interactive.z42.toml` | 包清单（exe / pack / apphost）|

## 依赖关系

- **前置**：`extract-compile-pipeline-api`——REPL 需要把「编译一段源 → 拿到可执行 zpkg/IR」
  下沉为可复用的进程内 API（`CompileResult` / `PackageCompiler`），而非 fork z42c 子进程。
- 依赖 `runtime/`（在同一 VM 实例中增量执行片段、保留会话状态）、`compiler/`（增量编译）。
- 被 launcher 命令分发调用。

## 状态

🟡 **骨架占位，已打包**。入口 + apphost bin/payload 均已就位，`z42.interactive.z42.toml`
已登记进 [`scripts/packages.toml`](../../../scripts/packages.toml)（`[component.interactive]`，
2026-07-01 User 裁决），随 SDK 包一起发行——但入口仍只打印 "planned" 后 `Environment.Exit(1)`，
真正的 REPL 仍待 `extract-compile-pipeline-api` 落地。

REPL 是 0.3.x capstone，设计见 [`docs/design/toolchain/repl.md`](../../../docs/design/toolchain/repl.md)；
推进时点见 `docs/roadmap.md`。
