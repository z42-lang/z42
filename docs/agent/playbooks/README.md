# docs/agent/playbooks/ — 任务操作清单

一类**重复出现、步骤多且易漏**的任务的逐步清单。与 [rules/](../rules/README.md)（约束「怎么做事」）不同，
这里写「这件具体的事要改哪些文件、按什么顺序」。与具体 AI 工具无关；Claude Code 的 `.claude/skills/`
只是这些文件的斜杠命令包装。

| 文件 | 何时读 |
|------|--------|
| [add-ir-op.md](add-ir-op.md) | 想新增 IR 指令 / opcode（**多数情况下不该加**，先读开头的两条更便宜的路） |

> 新增词法元素（关键字 / 符号 / 数字格式）的改动点见 [compiler-z42c.md「新增词法元素」](../rules/compiler-z42c.md)；
> 编译器调试面（`--dump-tokens` / `--dump-ast` 等）见 [dev-setup.md](../../internals/src/devinfra/dev-setup.md)。
> 不为这两件事另立清单——重复必漂移。
