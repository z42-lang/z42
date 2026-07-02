# docs/agent/ — 面向大模型的协作材料

本目录汇集**给大模型（AI）协作者用的、模型中立的**材料——不绑定任何特定工具。
各家工具（Claude Code / Codex / Cursor …）只需一个瘦入口（`CLAUDE.md` / `AGENTS.md`）
指进来即可（入口留待结构理清后配置）。

## 子目录

| 子目录 | 内容 |
|--------|------|
| [`rules/`](rules/) | 开发规范：[文档体系](rules/doc-system.md)、[book 写作规范](rules/book-writing.md)、协作流程、提交格式… |

> 后续如有其它分类（如提示词模板、检查清单等），在 `agent/` 下新建子目录，不塞进 `rules/`。

## 从哪读起

先读 [`rules/doc-system.md`](rules/doc-system.md)——全仓文档的顶层地图、四类文档职责、
以及本次重构的决策与迁移待办。

> 迁移期说明：多数开发规范暂仍在 `.claude/rules/`，将逐步搬来此处。见
> [`rules/doc-system.md` 第六节](rules/doc-system.md)。
