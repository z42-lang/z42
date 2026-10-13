# z42 — Claude Code 入口

本仓库的 AI 协作规范与工具无关，统一在 [docs/agent/](../docs/agent/README.md) 维护。
**本文件只做引用，外加 Claude Code 专属用法**——要改规范，改 `docs/agent/`。

@../docs/agent/README.md
@../docs/agent/rules/workflow.md
@../docs/agent/rules/philosophy.md
@../docs/agent/rules/doc-system.md

## Claude Code 专属

- **memory**：用 Claude Code 自带的 auto-memory（本机 `~/.claude/projects/<project>/memory/`，**不入库**）。
  workflow 阶段 6 的续推口令：后续任务写成 `memory/<slug>.md`，口令标在该文件的 `description` 与 `MEMORY.md`
  置顶指针行（「🔑 口令『…』→ 读本文件续推」）；User 可 `/clear` 后凭口令续推。
- `.claude/skills/`：任务清单（如「新增 IR 指令」）的斜杠命令薄包装；清单本体在三本书的对应页。
- `.claude/settings.json`：团队共享的工具权限白名单。个人覆盖写 `.claude/settings.local.json`（已 gitignore）。
