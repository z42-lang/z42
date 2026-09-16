# 第四部分 · 标准库（Standard Library）

z42 标准库的三层架构、包边界与各核心包设计。逐包的**用法**细节归各包 `src/libraries/<pkg>/README.md`；本部分讲**设计与边界**。

## 章节规划

| 章节 | 涵盖 |
|------|------|
| 三层架构与包边界 | core / 中间层 / 上层包的分层，依赖方向，API 准则 |
| 核心包索引 | 各包一句话职责 + 指向其目录 README 与设计文档 |

## 迁移状态（旧 `docs/design/stdlib/` → 本部分）

> ⬜ 待迁 · 🟡 迁移中 · ✅ 已迁并校对。

| 旧文档 | 目标章节 | 状态 |
|--------|---------|------|
| organization.md / overview.md / api-guidelines.md | 三层架构与包边界 | ⬜ |
| roadmap.md | （并入 `docs/roadmap.md` 或本部分排期段） | ⬜ |
| json.md / toml.md / yaml.md / uri.md / regex.md / crypto.md / compression.md / encoding.md / net.md / io-stream.md / io-binary.md / numerics.md / random.md / time.md / diagnostics.md / cli.md | 核心包索引（各包一节，链接到包内 README） | ⬜ |
| README-template.md | （→ 归入 `../../../agent/rules/code-organization.md` 的 README 模板） | ⬜ |
