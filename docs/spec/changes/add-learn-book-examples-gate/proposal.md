# Proposal: 学习手册（docs/learn）+ examples 清零重建 + 教程示例门禁

> 状态：🟡 进行中（2026-09-16 User 批准开工）| 创建：2026-09-15
> 子系统：`docs`（新 book + 写作规范）+ `toolchain`（xtask 门禁 / CI / 部署）
> 姊妹变更：[`add-beginner-cli-onramp`](../add-beginner-cli-onramp/proposal.md)（前三章依赖的 CLI 功能）

## Why

z42 已初步可用，但**没有任何面向用户的入门材料**：

- `docs/book/` 是设计知识库（页型只允许概览/机制/参考，禁止混合体），不适合写教程。
- `examples/` 是历史堆积：22 个单文件 showcase 里 21 个与 `src/tests` 重复覆盖、4 个编不过；
  `workspace-*` / `global_using/` 从未被任何门禁编译过（README 声称被校验，不属实）；
  `embedding/{hello,multi_line}.z42` 实为 6 条平台 CI 的测试夹具，与示例混在一起。
- 现有 examples 门禁（#615）只**编译**顶层单文件，不运行、不比对输出，更不校验书里展示的命令与输出。
- mdBook 的 `{{#include}}` **出错不失败**（实测 0.4.40）：文件缺失只打 ERROR、原样保留指令文本，
  锚点缺失**连日志都没有**、渲染成空代码块，退出码均为 0 —— 书与代码脱节会静默上线。

User 裁决（2026-09-15）：
1. 手册为**独立 book `docs/learn/`**，与知识库同站发布在 `/z42/learn/`。
2. **中文先写**，后续补英文。
3. examples **只放教程配套**，不与测试用例混；现有测试性质的内容搬去合适的测试目录。
4. examples 门禁**重新设计、一次彻底搞定**。
5. playground 暂不做，后续再加跳转链接（本变更只保证链接所需信息可得）。

## What Changes

1. **新 book `docs/learn/`**：book.toml、SUMMARY（完整目录规划，未写章节不进 SUMMARY）、首章「安装 / Hello World」骨架、写作规范。
2. **examples 清零**：夹具搬到平台测试目录、有独特覆盖的转为测试、其余删除；清理全仓对旧 examples 的引用。
3. **教程示例门禁（重设计）**：examples 按「工程 + 会话脚本（transcript）」组织；门禁用**当前源码构建的 SDK 里真实的 `z42` 命令**，
   在隔离沙箱里逐条执行脚本中的命令并比对输出；同时双向校验书 ↔ examples 的引用关系，禁止书里出现未经验证的代码块。
4. **发布**：部署工作流同时构建两本书；`examples/**`、`docs/learn/**` 改动也触发部署；PR 上构建两本书并把 mdBook 的 ERROR 判红。
5. **规则同步**：doc-system.md（新增教程文档类别 + §8 语言规则）、新增 learn-writing.md、test-gate.md、`xtask test changed` 映射、引用 examples 的 rules / skills / README。

## Scope（允许改动的文件）

| 文件 / 目录 | 变更 | 说明 |
|------|------|------|
| `docs/learn/**` | NEW | book.toml、SUMMARY、前言、第 1 章安装、第 2 章 Hello World、OUTLINE、`theme/`（z42 高亮别名） |
| `examples/**` | DELETE + NEW | 清空旧内容；新 README + 首章示例 |
| `src/toolchain/workload/fixtures/` | NEW（搬入） | `hello.z42`、`multi_line.z42`（+ 对应 toml） |
| `scripts/test/xtask_test_platform.z42` | MODIFY | 夹具路径 |
| `src/toolchain/workload/{wasm,ios,android,desktop}/**` | MODIFY | 注释 / .gitignore 中的夹具路径 |
| `src/tests/classes/target_typed_new.z42` | NEW | 从旧 examples 转来的独特覆盖（见 design §5） |
| `scripts/test/xtask_test_example.z42` | REWRITE | 新门禁；删 `_topLevelExamplesGate` |
| `scripts/test/examples-known-broken.txt` | DELETE | |
| `scripts/test/xtask_test_examples.z42`、`scripts/test/xtask_examples_{transcript,run,book}.z42` | NEW | 入口 / transcript 解析与匹配 / 沙箱执行 / 书引用校验 |
| `scripts/test/xtask_test_targets.z42`、`scripts/xtask_cli.z42`、`scripts/cli/xtask_cli_test.z42` | MODIFY | 项目级 `[[example]]` 目标并入 `targets` stage；CLI 入口 |
| `scripts/test/xtask_test.z42` | MODIFY | examples stage 前置（SDK）与 `--no-build` 行为 |
| `scripts/test/xtask_test_changed.z42` | MODIFY | `examples/**`、`docs/learn/**` 映射 |
| `.github/workflows/deploy-book.yml`、`.github/workflows/ci.yml` | MODIFY | 双书构建 / 触发路径 / PR 构建检查 / examples stage 所需 SDK |
| `docs/agent/rules/doc-system.md`、`docs/agent/rules/learn-writing.md`（NEW） | MODIFY / NEW | |
| `docs/book/src/dev/test-gate.md`、`docs/book/src/dev/xtask.md` | MODIFY | stage 表 / changed 映射 / 命令 |
| `.claude/CLAUDE.md`、`.claude/rules/{spec,workflow}.md`、`.claude/skills/{add-token,parse-test}/SKILL.md` | MODIFY | 引用旧 examples 的地方 |
| `README.md`、`src/tests/README.md`、`docs/workflow/**`、`docs/design/**` 中引用旧 examples 的行 | MODIFY | 仅改引用 |

## Out of Scope

- 教程正文第 2 章以后的内容（按部分分批 PR，每批 = 章节 + 示例）。
- playground 跳转按钮（链接协议见 design D8，网站就绪后单独加）。
- 英文版手册。
- CLI 功能补齐（见姊妹变更 `add-beginner-cli-onramp`）。
- 嵌入（C / Rust）章节的外部工具链支持：transcript 格式预留 `requires`，随该章节落地时实现。

## Open Questions

- **Q1（实施期测量后定，无需 User 裁决）**：examples stage 在 CI test-host 腿上获得「带 launcher 的 SDK」的代价。
  判定规则见 design D6。
