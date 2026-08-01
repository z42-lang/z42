# .claude/rules/ — 开发规范总入口

AI 与协作者干活的行为规范（文档四类中的**③ 开发规范**，见
[doc-system.md](../../docs/agent/rules/doc-system.md)）。分四组：

> 迁移中：本组规范正逐步搬往 `docs/agent/rules/`（模型中立）。`doc-system.md` 已迁，
> 见 [`docs/agent/rules/doc-system.md`](../../docs/agent/rules/doc-system.md) 第五节。

## 流程主线（怎么推进一次变更）

| 文件 | 管什么 |
|------|--------|
| [workflow.md](workflow.md) | 协作流程主线：阶段 0–9、Scope、GREEN 门禁、归档 |
| [philosophy.md](philosophy.md) | 实现哲学：最终方案优先 / 根因修复 / 不做兼容 / 设计完整性 / 延后管理 |
| [parallel-development.md](parallel-development.md) | 多 change 并行：PR 隔离模型（分支/worktree + 先来后到合并 + 合并前并入 main·重跑 GREEN + 合并后删分支） |

## 产出规范（代码/提交/文档长什么样）

| 文件 | 管什么 |
|------|--------|
| [code-organization.md](code-organization.md) | 目录 README 模板（5 段）、文件/函数/类型行数限制 |
| [commit-log.md](commit-log.md) | 提交信息格式 `type(scope): 描述` |
| [doc-system.md](../../docs/agent/rules/doc-system.md) 📍docs/agent/rules/ | 文档体系总纲：四类文档职责、SoT、知识上浮、迁移期约定 |
| [version-bumping.md](version-bumping.md) | zbc / zpkg 格式 version bump checklist |

## 语言专属（各技术栈的坑与约定）

| 文件 | 管什么 |
|------|--------|
| [compiler-z42c.md](compiler-z42c.md) | z42c（编译器，用 z42 写）代码约定 |
| [runtime-rust.md](runtime-rust.md) | Rust VM 代码约定 |
| [common-pitfalls.md](common-pitfalls.md) | 跨语言共同陷阱（加载顺序非确定性等） |
| [bootstrap-seed.md](bootstrap-seed.md) | 自举种子鸡蛋问题、分阶段引入新语法纪律 |

## 其它

| 文件 | 管什么 |
|------|--------|
| [spec.md](spec.md) | 语言规范文件（design/language、examples）编写细则 |

> 顶层工作手册在 [`.claude/CLAUDE.md`](../CLAUDE.md)，每次对话自动加载；本目录是其展开细则。
