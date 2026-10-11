# docs/agent/ — AI 协作规范

AI 与协作者干活的**行为约束与操作手册**，与具体 AI 工具无关：Claude Code、Codex、Cursor、Gemini 等
都从这里读同一份规范。系统知识（架构 / 机制 / 语言规则）不在这里，那是[三本书](rules/doc-system.md)的事。

> z42 是一门融合 C#、Rust、Python 优点的系统编程语言：编译器用 z42 自举（`src/compiler`，编译为 zpkg），
> 虚拟机用 Rust（Interpreter / JIT / AOT）。仓库布局见[根 README](../../README.md#repository-layout)，
> 构建 / 测试 / 打包命令见 [docs/internals/src/devinfra/](../internals/src/devinfra/)，
> 实现计划见 [docs/roadmap.md](../roadmap.md)（**当前焦点不在别处复制**——焦点变化快，复制必过时）。

## 怎么用

**本文件是全部规范的唯一索引**：每条规则只在唯一归属文件里定义，其余位置只留「触发条件 + 一行结论 + 链接」。
任务型操作清单（「这件事要改哪些文件、按什么顺序」）不另立目录，它跟对应规范放在一起。

**每次会话先读**：[workflow.md](rules/workflow.md)（流程主线）· [philosophy.md](rules/philosophy.md)（判断准则）·
[doc-system.md](rules/doc-system.md)（文档总纲）。其余按「什么时候读」进：

## 推进一次变更时

| 文件 | 管什么 |
|------|--------|
| [workflow.md](rules/workflow.md) | 流程主线（以 PR 为单位）：隔离铁律与 worktree 供种、阶段 0–6、变更分类、Spec-First、Scope、批量授权、GREEN 门禁、并行与合并顺序、越界防护、测试要求 |
| [commit-log.md](rules/commit-log.md) | 提交信息格式 `type(scope): 描述`、scope 子系统表、页脚 |
| [philosophy.md](rules/philosophy.md) | 做选择时的判准：最终方案优先 / 根因修复 / 不做兼容 / 设计完整性 / 规范冲突检测 / 事实校正 / 延后管理 |

## 写文档时

| 文件 | 管什么 |
|------|--------|
| [doc-system.md](rules/doc-system.md) | **总纲**：三本书各自的角色与「明确不写什么」、三问、三道门、单向链接、知识上浮、语种 |
| [book-writing.md](rules/book-writing.md) | 三本书的页型与写法：`reference` / `internals` 的页头、机制页骨架、图表、检索，以及 `learn` 的章节写法与 `examples/` 的会话脚本、门禁码 |
| [readme-writing.md](rules/readme-writing.md) | 根 README 与目录 README 的写法（**六段模板的唯一 SoT**） |

## 写代码时

| 文件 | 管什么 |
|------|--------|
| [code-organization.md](rules/code-organization.md) | 哪层目录要 README + 文件/函数/类型的行数限制（`xtask check lines` 棘轮） |
| [common-pitfalls.md](rules/common-pitfalls.md) | 跨语言共同陷阱：加载顺序非确定性、id 作用域、局部变量无块级 shadow |
| [compiler-z42c.md](rules/compiler-z42c.md) | z42c 写代码的约定：受限写法、Parser 错误处理、自举纪律（机制与**「新增词法 / 语法 / IR 指令」清单**见书里 [extending-language.md](../internals/src/compiler/extending-language.md)） |
| [runtime-rust.md](rules/runtime-rust.md) | Rust VM 代码约定 + 推荐依赖与参考实现 |

> 编译器调试面（`--dump-tokens` / `--dump-ast` 等）见
> [dev-setup.md](../internals/src/devinfra/dev-setup.md)。

## 碰自举链 / 改格式时

| 文件 | 管什么 |
|------|--------|
| [bootstrap-seed.md](rules/bootstrap-seed.md) | 种子鸡蛋问题；**新语法 / 格式 support 先行、晚一个 nightly 再 use** |
| [version-bumping.md](rules/version-bumping.md) | zbc / zpkg 格式 version bump 的同步 checklist + 编译器语义指纹 |

## 各 AI 工具的入口

规范只在本目录维护；各工具的入口文件只做引用，不放规则：

| 工具 | 入口 | 说明 |
|------|------|------|
| Claude Code | [`.claude/CLAUDE.md`](../../.claude/CLAUDE.md) | 引用本文件；`.claude/skills/` 是任务清单（如「新增 IR 指令」）的斜杠命令薄包装，清单本体在对应规则文件里；`.claude/settings.json` 是团队共享权限 |
| 其他工具 | 本文件 | 在该工具的指令文件里指向 `docs/agent/README.md` 即可 |
