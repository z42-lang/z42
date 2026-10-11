# docs/agent/rules/ — 开发规范

AI 与协作者干活的**行为约束**。这里不写任何系统知识——那是三本书的事（[doc-system.md](doc-system.md)）。

**本文件是规则的唯一索引**：[`docs/agent/README.md`](../README.md) 只做入口、不复述，
各 rules 文件是每条规则的唯一归属地。

**每次会话先读**：[workflow.md](workflow.md)（流程主线）· [philosophy.md](philosophy.md)（判断准则）·
[doc-system.md](doc-system.md)（文档总纲）。其余按「什么时候读」进：

## 推进一次变更时

| 文件 | 管什么 |
|------|--------|
| [parallel-development.md](parallel-development.md) | PR 隔离模型：一 change 一 worktree 一分支、先来后到合并、合并前并入 main + 重跑 GREEN |
| [commit-log.md](commit-log.md) | 提交信息格式 `type(scope): 描述`、页脚 |

## 写文档时

| 文件 | 管什么 |
|------|--------|
| [doc-system.md](doc-system.md) | **总纲**：三本书各自的角色与「明确不写什么」、三问、三道门、单向链接 |
| [readme-writing.md](readme-writing.md) | 根 README 与目录 README 的写法（**六段模板的唯一 SoT**） |
| [book-writing.md](book-writing.md) | `reference` / `internals` 的页型、页头、图表、检索约定 |
| [learn-writing.md](learn-writing.md) | `learn` 与 `examples/` 的写法：代码只来自 examples、会话脚本格式、门禁规则码 |

## 写代码时

| 文件 | 管什么 |
|------|--------|
| [code-organization.md](code-organization.md) | 哪层目录要 README + 文件/函数/类型的行数限制（`xtask check lines` 棘轮） |
| [common-pitfalls.md](common-pitfalls.md) | 跨语言共同陷阱：加载顺序非确定性、id 作用域、局部变量无块级 shadow |
| [compiler-z42c.md](compiler-z42c.md) | z42c（编译器，用 z42 写）：子包结构、Lexer / Parser / AST 约定 |
| [runtime-rust.md](runtime-rust.md) | Rust VM 代码约定 |
| [dependencies.md](dependencies.md) | 加新依赖前先查：已采用 / 推荐的 Rust 库 + 子系统参考实现 |

## 碰自举链 / 改格式时

| 文件 | 管什么 |
|------|--------|
| [bootstrap-seed.md](bootstrap-seed.md) | 种子鸡蛋问题；**新语法/格式 support 先行、晚一个 nightly 再 use** |
| [version-bumping.md](version-bumping.md) | zbc / zpkg 格式 version bump 的同步 checklist + 编译器语义指纹 |

## 任务操作清单

一类重复出现、步骤多且易漏的任务的逐步清单（「这件事要改哪些文件、按什么顺序」）见
[../playbooks/](../playbooks/README.md)——与上面的「怎么做事」互补。

---

> 各 AI 工具的入口文件（如 Claude Code 的 [`.claude/CLAUDE.md`](../../../.claude/CLAUDE.md)）只做引用，
> 不放规则；实质规范都在 `docs/agent/`。
