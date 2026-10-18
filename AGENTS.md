# z42 — AI 协作入口（AGENTS.md）

本仓库的 AI 协作规范与具体工具无关，统一在 [docs/agent/](docs/agent/README.md) 维护。
**本文件只做引用，外加一份提交流程速查**——要改规范，改 `docs/agent/`。

**每次会话先读**（完整清单与「什么时候读什么」见 [docs/agent/README.md](docs/agent/README.md)）：

- [docs/agent/README.md](docs/agent/README.md) — 规范索引与开工导航
- [docs/agent/rules/workflow.md](docs/agent/rules/workflow.md) — 流程主线（阶段 0–6、GREEN 门禁、并行与合并顺序）
- [docs/agent/rules/philosophy.md](docs/agent/rules/philosophy.md) — 判断准则（最终方案优先、根因修复、不留兼容）
- [docs/agent/rules/doc-system.md](docs/agent/rules/doc-system.md) — 文档规则（三问、三道门、防漂移）

## 提交流程速查

完整规则见 [workflow.md](docs/agent/rules/workflow.md)；本节只列触发条件 + 一行结论：

1. **提交 / 合并前先确认基于最新 main**：`git fetch` 后 rebase `origin/main`，不带着落后的 base 合并
   （workflow.md 阶段 6「合并前置」）。
2. **rebase 冲突及时解决**：机械冲突当场解掉继续；暴露的是方案分歧时停下找 User 裁决，不硬合。
3. **没问题自动合并**：并入最新 main 后完整 `xtask test` 全绿，即 squash 合并并清理分支 / worktree，
   无需等 User 确认（批量授权的边界仍守）。
4. **实现功能 / 修复 bug 必须带测试用例**：无测试 = 未完成（要求表见 workflow.md「测试要求」）。
5. **性能变更必须带性能用例**：被优化的路径要有 bench 覆盖并附 A/B 结论，防回归
   （判红规则见 [性能基准与回归门禁](docs/internals/src/devinfra/benchmarking.md)）。

## 跨会话记忆（memory）

不入库，放哪由所用工具的自带机制决定（本地家目录、不进仓库）。续推口令怎么写、写透到什么程度，
见 workflow.md 阶段 6 第 6 步。
