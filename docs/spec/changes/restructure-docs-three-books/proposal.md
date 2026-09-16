# Proposal: 文档三书重构

## Why

`doc-system.md` 2026-07-01 定的终态是「book 作为知识库唯一 SoT，`docs/design/` 迁完即删」。两个半月后的实测：

| 项 | 计划 | 实际 |
|---|---|---|
| design → book 迁移 | 迁完即删 | **81 篇迁了 2 篇（2.5%）** |
| design 停止写入（D2） | 不再往里写 | `design/runtime/zbc.md` **2026-09-16 仍在改**（book 对应页停在 07-19）——两份并行漂移 |
| 规范集中 | 全迁 `docs/agent/rules/` | ✅ 已由 `consolidate-agent-rules` 收口（16 篇归一） |

**根因不是执行力，是结构**：book 被定义为「用户 + 维护者 + 大模型」**双受众**，于是实现细节在其中没有一等位置——把 `compiler-architecture.md` 的 1431 行 TSIG / Pratt / BoundVisitor 迁进一本「也给用户看的书」本身就别扭，所以迁不动。同源病症还有：语种策略无法收敛、`workflow/packaging.md` 与 `book/src/dev/packaging.md` 文件名撞车、CI job 表三处并存。

**按受众切成三本书**，每本的深度、语种、是否发布都自洽，实现细节终于有自己的家。

## What Changes

- **建 `docs/reference/`**（语言与库参考，面向使用者查）与 **`docs/internals/`**（实现内幕，面向维护者），连同 `docs/learn/` 共三本 mdBook，同站发布。
- **`docs/book/` 拆散**并入 reference 与 internals；**`docs/design/`（99 篇）与 `docs/workflow/`（25 篇）在终态不存在**，内容全部并入。
- **重写 `docs/agent/rules/doc-system.md` 为唯一总纲**：三书判据（含「明确不写什么」）、三问触发判据、三道门、单向链接铁律。
- **删除两份重复的触发矩阵**（`workflow.md` 阶段 9、`code-organization.md`），改为链接总纲。
- 游离文件处置：`features.md` → internals；`library_review.md` / `todo-list.md` → 删；`docs/README.md` 重写。

详细的角色定义、内容规划、迭代规范、分批计划见 [design.md](design.md)。

## Scope（允许改动的文件）

本次改动约 200 篇文档，**Scope 按批次以目录 + 清单方式给出**（逐文件路径见 `migration-manifest.md`，该清单是 tasks 的执行依据，两者双向对齐）。

| 批 | 允许改动 | 变更类型 |
|---|---|---|
| **0** | `docs/agent/rules/doc-system.md` | MODIFY（重写） |
| | `docs/reference/{book.toml,src/SUMMARY.md,src/README.md,src/*/README.md}` | NEW |
| | `docs/internals/{book.toml,src/SUMMARY.md,src/README.md,src/*/README.md}` | NEW |
| | `.github/workflows/deploy-book.yml` | MODIFY（扩到三书） |
| **1** | `docs/book/src/compiler/**`(12) → `docs/internals/src/compiler/**`；`docs/design/compiler/**`(9) 合并或删除 | RENAME / MODIFY / DELETE |
| **2** | `docs/book/src/runtime/**`(21) + `docs/design/runtime/**`(25) → `docs/internals/src/{runtime,formats}/**` | RENAME / MODIFY / DELETE |
| **3** | `docs/book/src/language/**`(22) + `docs/design/language/**`(28) → `docs/reference/src/language/**` | RENAME / MODIFY / DELETE |
| **4** | `docs/book/src/stdlib/**`(4) + `docs/design/stdlib/**`(22) → `docs/reference/src/stdlib/**` | RENAME / MODIFY / DELETE |
| **5** | `docs/book/src/{toolchain,dev}/**`(12) + `docs/design/{toolchain,testing}/**`(13) + `docs/workflow/**`(25) → `docs/reference/src/toolchain/**` 与 `docs/internals/src/{toolchain,dev}/**` | RENAME / MODIFY / DELETE |
| **6** | `docs/design/`、`docs/workflow/`（删空壳）；`docs/README.md`、`docs/features.md`、`docs/library_review.md`、`docs/todo-list.md`；`../../../agent/rules/workflow.md`、`../../../agent/rules/code-organization.md`（删矩阵拷贝）；全仓链接重指（`src/**/README.md`、`.claude/`、`scripts/README.md`、根 `README.md`） | MODIFY / DELETE |
| **7** | `scripts/test/xtask_test_docs.z42`、`scripts/test/xtask_test.z42`、`docs/internals/src/dev/test-gate.md`、`.github/workflows/ci.yml` | NEW / MODIFY |

**只读引用**：`docs/learn/**`（除链接重指外不动）、`docs/spec/archive/**`、`docs/roadmap.md`（仅批 6 改 Deferred 索引链接）。

## Out of Scope

- **`docs/spec/changes/` 的 118 个未归档 change**（19 个已标 🟢 却仍在 `changes/`）：真实的卫生问题，但属 `docs/spec/` 而非三本书，单开 change 清理。
- ~~`.claude/rules/` 与 `docs/agent/rules/` 的收口~~ → **已由 change `consolidate-agent-rules` 完成**（User 2026-09-16 裁决「迁回 docs」）：12 篇全部并入 `docs/agent/rules/`，`.claude/CLAUDE.md` 退化为瘦入口。
  ⚠️ 例外：批 0 重写 `doc-system.md`、批 6 删 `workflow.md` / `code-organization.md` 里的矩阵拷贝，这两处**必须**在本次做（否则总纲与拷贝冲突）。
- **internals 内容的质量重写**：本次只做搬迁 + 合并 + 删重复，不借机重写机制页。
- **英文版**：reference / learn 的英文版不在本次。
- **教程外迁 z42-docs 仓**（roadmap `infra-extract-user-docs`）：本次拆分让它更容易，但不在本次做。

## Open Questions

**已裁决 12 条**，见 [design.md §五之二](design.md)。剩余一条：

- [ ] **Q1 已发布 URL 断裂**：`/z42/`（现 book）消失，分流到 `/z42/reference/` 与 `/z42/internals/`。pre-1.0 直接接受（与「不为旧版本提供兼容」一致），还是在站点根放一页分流索引？**建议：接受断裂，站点根放一页三书分流索引**（成本近零，且站点根本就该有一个入口页）。
