# 提交信息规范

> 触发条件：任何 `git commit`。

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

| scope | 范围 |
|-------|------|
| `compiler` | `src/compiler/`（z42c 自举编译器源码，用 z42 写） |
| `runtime` | `src/runtime/`（Rust VM：interp / jit / aot / gc） |
| `stdlib` | `src/libraries/`（.z42 标准库） |
| `toolchain` | `src/toolchain/` + xtask dispatch |
| `docs` | `docs/` |

跨子系统时取主要那个，或省略 scope。这张表同时用来判断「两个 in-flight PR 是否同子系统、要不要互相知会」
（见 [workflow.md 阶段 6](workflow.md)）。

## 一个 commit = 一个逻辑单元

- 一个 PR 对应一个逻辑单元，不积压、不混合多个独立功能/修复；squash 合并后 main 上**一个 PR 一个提交**（标题带 `(#N)`）
- 伴随文档、规则变更（`docs/agent/`、各工具入口文件）属同一逻辑单元，进同一个 PR
- squash 提交正文的写法见 [workflow.md 阶段 6](workflow.md) 第 4 步

## 页脚

**AI 生成的提交**，在信息末尾附**当次工具链下发的**模型署名行；**PR 描述**末尾附同一份署名。

- **格式由工具链决定**（各工具的署名形式不同），本文件不规定具体字符串，也不硬编码某个工具的示例。
- **工具链没有下发署名时，不加**——不要自己编一个（历史提交也不必回头补）。

## 示例

```
feat(compiler): packages.toml 解析模块 + include 名解析

3.2/3.4a 阶段：新增 toml 段解析与 include 名到 zpkg 路径的解析，
接通 4.1b 的 packages.toml 组装引擎。
```
```
docs: 确立文档体系总纲，新建 book 骨架
```
