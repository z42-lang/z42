# docs/agent/ — AI 协作规范

AI 与协作者干活的**行为约束与操作手册**，与具体 AI 工具无关：Claude Code、Codex、Cursor、Gemini 等
都从这里读同一份规范。系统知识（架构 / 机制 / 语言规则）不在这里，那是[三本书](rules/doc-system.md)的事。

> z42 是一门融合 C#、Rust、Python 优点的系统编程语言：编译器用 z42 自举（`src/compiler`，编译为 zpkg），
> 虚拟机用 Rust（Interpreter / JIT / AOT）。仓库布局见[根 README](../../README.md#repository-layout)，
> 构建 / 测试 / 打包命令见 [docs/internals/src/devinfra/](../internals/src/devinfra/)，
> 实现计划见 [docs/roadmap.md](../roadmap.md)（**当前焦点不在别处复制**——焦点变化快，复制必过时）。

## 内容

| 位置 | 管什么 |
|------|--------|
| [rules/](rules/README.md) | 行为约束：协作流程、判断准则、提交与并行、文档写法、代码组织、自举与格式 bump。**按「什么时候读」索引** |
| [playbooks/](playbooks/README.md) | 一类任务的操作清单（如「加一条 IR 指令」），按需读 |

## 每次会话先读

1. [rules/workflow.md](rules/workflow.md) —— 协作流程主线（以 Pull Request 为单位）：变更分类、Spec-First、阶段 0–6、GREEN 门禁、合并
2. [rules/philosophy.md](rules/philosophy.md) —— 做选择的判准：最终方案优先 / 根因修复 / 设计完整性 / 规范冲突检测 / 事实校正
3. [rules/doc-system.md](rules/doc-system.md) —— 文档总纲：三本书的角色、三问、三道门

## 核心要点

- **新会话开场**：读取当前阶段（roadmap + `gh pr list` 进行中的 PR）和你所用工具的跨会话记忆（若有），
  主动说明状态与下一步。
- **一次迭代 = 一个 PR，PR 描述是方案与记录**：需规范先行（lang / ir / vm 类变更）：draft PR 写方案 → User 确认 → IMPL → GREEN → 合并；
  轻量变更（fix / refactor / test）：直接 IMPL → GREEN → 合并。拿不准按严格模式走。
- **全绿（GREEN）**：定义与例外见 [workflow.md 阶段 5](rules/workflow.md)，以那里为准；未全绿不得 commit / push。
- **提交**：格式 `type(scope): 描述`，每个逻辑单元单独提交（[commit-log.md](rules/commit-log.md)），无需 User 二次确认。
- **落地走 PR 优先**：一 PR 一 worktree 一分支，很小的改动可直推 main，其余开 PR；合并前并入 main
  最新改动 + 重跑 GREEN，合并后删分支 / worktree（[parallel-development.md](rules/parallel-development.md)）。
- **碰自举链 / 改格式**：新语法 / 格式「support 先行、晚一个 nightly 再 use」（[bootstrap-seed.md](rules/bootstrap-seed.md)）；
  zbc / zpkg version bump 走 [version-bumping.md](rules/version-bumping.md) 的 checklist。
- **文档同步**：任何改变外部可见行为、机制、规则或约定的迭代，合并前必须有对应文档落地，**无文档 = 未完成**。
  判据是 [doc-system.md 的三问](rules/doc-system.md)；复杂实现逻辑必须落 `docs/internals/` 对应机制页。
- **代码风格**：z42c 用 z42 写（[compiler-z42c.md](rules/compiler-z42c.md)）；
  Rust VM 见 [runtime-rust.md](rules/runtime-rust.md)；目录 README 与行数限制见
  [code-organization.md](rules/code-organization.md)；加新依赖前先查 [dependencies.md](rules/dependencies.md)。

## 各 AI 工具的入口

规范只在本目录维护；各工具的入口文件只做引用，不放规则：

| 工具 | 入口 | 说明 |
|------|------|------|
| Claude Code | [`.claude/CLAUDE.md`](../../.claude/CLAUDE.md) | 引用本文件；`.claude/skills/` 是 [playbooks/](playbooks/README.md) 的薄包装；`.claude/settings.json` 是团队共享权限 |
| 其他工具 | 本文件 | 在该工具的指令文件里指向 `docs/agent/README.md` 即可 |
