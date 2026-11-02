# 提交信息规范

> 触发条件：commit 与 PR 标题。PR 生命周期、合并与清理统一见 [workflow.md](workflow.md)。

## 格式

```text
type(scope): 描述

[可选正文：原因与权衡]
```

标题 ≤ 72 字符，描述用中文、动词开头、不加句号。正文解释原因与关键决策，不复述 diff。
一个提交表达一个逻辑单元；伴随测试和文档同轮交付，PR 的拆分与 squash 要求以 workflow 为准。

## type（改动性质）

| type | 用于 |
|---|---|
| `feat` | 新功能、语法、IR 或 API |
| `fix` | Bug 修复 |
| `refactor` | 不改外部行为的重构 |
| `perf` | 性能优化 |
| `test` | 只改测试 |
| `docs` | 只改文档，包括 rules / README |
| `chore` | 构建、CI 与杂项 |

这里的 type 描述提交性质，不决定审批模式；后者按 workflow 的实际契约变化分类。

## scope（子系统）

| scope | 范围 |
|---|---|
| `compiler` | 自举编译器 |
| `runtime` | Rust VM、interp / JIT / AOT / GC |
| `stdlib` | 标准库 |
| `toolchain` | 工具链与 xtask |
| `docs` | 文档 |

跨子系统取主要者，或省略 scope；不另用这张表定义 PR 依赖和并行关系。

## 页脚

AI 提交和 PR 描述附当前工具链明确下发的署名，格式服从该工具链。
没有下发署名时不加，不自行编造，也不回填历史提交。
