# z42 — Claude Code 入口

本仓库的 AI 协作规范与工具无关，统一在 [docs/agent/](../docs/agent/README.md) 维护。
**本文件只做引用，不放规则**——要改规范，改 `docs/agent/`。

@../docs/agent/README.md
@../docs/agent/rules/doc-system.md

## Claude Code 专属

- `.claude/skills/`：任务清单（如「新增 IR 指令」）的斜杠命令薄包装，清单本体在 [docs/agent/rules/](../docs/agent/rules/) 对应文件里（见 [docs/agent/README.md](../docs/agent/README.md) 的索引）。
- `.claude/settings.json`：团队共享的工具权限白名单。个人覆盖写 `.claude/settings.local.json`（已 gitignore）。
- 跨会话记忆用 Claude Code 自带的 auto-memory（本机 `~/.claude/`），**不入库**。
