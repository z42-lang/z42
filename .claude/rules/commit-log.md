# 提交信息规范

> 触发条件：任何 `git commit`。此前散落在 [workflow.md](workflow.md) / [CLAUDE.md](../CLAUDE.md)
> 的提交约定集中到此。

---

## 格式

```
type(scope): 描述

[可选正文：为什么这么改，而非改了什么]
```

- **一行 summary ≤ 72 字符**，动词开头，不加句号
- 描述用**中文**（与内部文档语种一致）
- 正文可选；解释"为什么/权衡"，不复述 diff

## type（改动性质）

| type | 用于 |
|------|------|
| `feat` | 新功能 / 新语法 / 新 IR 指令 / 新 API |
| `fix` | Bug 修复 |
| `refactor` | 纯重构，不改外部行为 |
| `perf` | 性能优化 |
| `test` | 只动测试 |
| `docs` | 只动文档（含 book / rules / README） |
| `chore` | 构建脚本 / CI / 杂项 |

> 与 [workflow.md 变更分类](workflow.md)（lang/ir/vm/fix/refactor/test/docs）的关系：
> 那是**流程**分类（决定走完整流程还是最小化）；这里是**提交** type（描述这次 commit 的性质）。
> lang/ir/vm 类变更的提交通常是 `feat`。

## scope（子系统）

沿用[子系统划分](parallel-development.md)：`compiler` / `runtime` / `stdlib` / `toolchain` / `docs`。
跨子系统时取主要那个，或省略 scope。

## 一个 commit = 一个逻辑单元

- 每个 `docs/spec/changes/` 变更对应**一个** commit，不积压、不混合多个独立功能/修复
- 拆分（refactor）与功能变更**分开提交**（见 [code-organization.md](code-organization.md)）
- `.claude/`（规则/记忆）与 `docs/spec/`（提案/归档）**必须纳入**对应提交，不得遗漏
- **归档（`changes/`→`archive/` + tasks.md 改 🟢 + 文档同步）是该变更「同一逻辑单元」的一部分，必须随
  代码进同一个 PR，禁止 PR 合并后再单独 push 一个 `docs: 归档` 到 main**——那会把一个逻辑单元拆成两次
  落地、给 main 平白多一条提交。详见 [workflow.md 阶段 9 铁律](workflow.md)。同理，**任何伴随代码变更的
  文档（book 机制页、README、workflow 手册等）都在同一 PR 内一起改一起提交**，不留「代码先合、文档后补」的尾巴。

## 页脚

AI 生成的提交在信息末尾附：

```
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
```

## 示例

```
feat(compiler): packages.toml 解析模块 + include 名解析

3.2/3.4a 阶段：新增 toml 段解析与 include 名到 zpkg 路径的解析，
接通 4.1b 的 packages.toml 组装引擎。
```
```
docs: 确立文档体系总纲，新建 book 骨架
```
