# docs/agent/ — AI 协作规范

AI 与协作者干活的**行为约束与操作手册**，与具体 AI 工具无关：Claude Code、Codex、Cursor、Gemini 等
都从这里读同一份规范。系统知识（架构 / 机制 / 语言规则）不在这里，那是[三本书](rules/doc-system.md)的事。

> z42 是一门融合 C#、Rust、Python 优点的系统编程语言：编译器用 z42 自举（`src/compiler`，编译为 zpkg），
> 虚拟机用 Rust（Interpreter / JIT / AOT）。仓库布局见[根 README](../../README.md#repository-layout)，
> 构建 / 测试 / 打包命令见 [docs/internals/src/devinfra/](../internals/src/devinfra/)，
> 实现计划见 [docs/roadmap.md](../roadmap.md)（**当前焦点不在别处复制**——焦点变化快，复制必过时）。

## 从哪进

| 位置 | 管什么 | 怎么进 |
|------|--------|--------|
| [rules/](rules/README.md) | **行为约束**：协作流程、判断准则、提交与并行、文档写法、代码组织、自举与格式 bump | 按[「什么时候读」索引](rules/README.md)进；每次会话先读其中的 workflow / philosophy / doc-system |
| [playbooks/](playbooks/README.md) | 一类任务的操作清单（如「加一条 IR 指令」） | 任务命中时按需读 |

**本文件不复述任何规则。** 每条规则只在唯一归属文件里定义，索引只在 [`rules/README.md`](rules/README.md)；
这里只回答「我是谁、从哪个门进」——多一份摘要就多一处会漂的真相。

## 各 AI 工具的入口

规范只在本目录维护；各工具的入口文件只做引用，不放规则：

| 工具 | 入口 | 说明 |
|------|------|------|
| Claude Code | [`.claude/CLAUDE.md`](../../.claude/CLAUDE.md) | 引用本文件；`.claude/skills/` 是 [playbooks/](playbooks/README.md) 的薄包装；`.claude/settings.json` 是团队共享权限 |
| 其他工具 | 本文件 | 在该工具的指令文件里指向 `docs/agent/README.md` 即可 |
