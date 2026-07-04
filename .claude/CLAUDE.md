# z42 — Claude 工作手册

## 项目简介

z42 是一门融合 C#、Rust、Python 优点的系统编程语言。
- 编译器：z42 自举（`src/compiler`，自编译为 zpkg）；C# bootstrap 编译器已移除（2026-06-26）
- 虚拟机：Rust，支持 Interpreter / JIT / AOT 混合执行
- 详细设计见 `docs/book/`（知识库 SoT；旧 `docs/design/` 迁移中，见 doc-system.md）；库推荐见 `.claude/libraries.md`

## 代码库结构

```
src/compiler/       # z42 自举编译器（z42c.core/ir/syntax/project/semantics/pipeline/driver，编译为 zpkg）
src/runtime/    # Rust VM（interp / jit / aot）
src/libraries/  # 标准库 .z42 源码（编译后产出 .zpkg）
src/toolchain/  # 配套工具链（launcher / test-runner / workload；debugger·builder 占位）
docs/book/      # 知识库（mdBook：语言/编译器/运行时/stdlib/工具链；旧 docs/design/ 迁移中）
examples/       # .z42 示例源文件
```

## 构建与测试

所有构建、编译、测试、打包命令见 [docs/workflow/](../docs/workflow/)（按主题分子目录：building / testing / ci / release / debugging）。

## 实现计划

见 `docs/roadmap.md`。当前焦点：**0.3.x 自举线**——GC v1 地基 → A（stdlib 重组+perf）‖ B（**编译器全自举**：7 子系统用 z42 重写到 byte-identical）‖ C（反射 MVP）→ REPL capstone（2026-06-07 重排；规划见 `docs/spec/changes/plan-0.3.x-three-streams/proposal.md`）。

## 协作工作流（必须遵守）

完整流程见 [`workflow.md`](rules/workflow.md)（流程主线 / Scope / commit）+ [`philosophy.md`](rules/philosophy.md)（实现哲学 / 设计完整性 / 延后管理）+ [`version-bumping.md`](rules/version-bumping.md)（zbc / zpkg version bump checklist）+ [`parallel-development.md`](rules/parallel-development.md)（多 change 并行：子系统互斥锁 + `docs/spec/changes/ACTIVE.md` 账本）+ [`bootstrap-seed.md`](rules/bootstrap-seed.md)（自举种子鸡蛋问题：删构建期种子/兜底前必须先为所有 cold-start 入口供种，删+供种是同一原子变更；**新语法/格式分阶段引入纪律——support 先行、晚一个 nightly 再 use，让上一版 z42c 永远能编当前源码 → 彻底删 C# 种子的前提**）。核心要点：

- **每次新对话**：Claude 自动读取 `.claude/projects/<project>/memory/MEMORY.md` 和当前阶段，主动说明状态和下一步
- **需规范先行**（lang / ir / vm 类变更）：DRAFT → User 确认 → IMPL → GREEN → COMMIT
- **轻量变更**（fix / refactor / test）：直接 IMPL → GREEN → COMMIT
- **全绿（GREEN）标准**：定义见 [workflow.md 阶段 8](rules/workflow.md)；任何测试失败（含 pre-existing）都不得 commit / push
- **提交格式**：`type(scope): 描述`，每个逻辑单元单独提交
- **自动提交**：每次迭代完成后 Claude 自动 commit + push，`.claude/` 和 `docs/spec/` 必须纳入，无需 User 二次确认

## 文档同步（必须遵守）

**核心规则：任何改变了外部可见行为、机制、规则或约定的迭代，归档前必须有对应文档落地。无文档 = 未完成。**

具体的"改动类型 → 需更新文档"映射见 [workflow.md 阶段 9](rules/workflow.md) 的**统一维护触发矩阵**（唯一 SoT）；归档前按同节 **doc-check 清单**逐项核对。

> **实现原理文档规则（2026-04-25）**：涉及编译器或 VM 的**内部机制 / 架构策略**的变更（不只是对外行为），必须把"实现原理"（数据结构、算法、加载策略、决策权衡）同步到 `docs/book/` 编译器 / 运行时部分的对应机制页（旧 `docs/design/` 不再更新，见 doc-system.md 决策 D2），使新接手者不必阅读大量源码即可理解"为什么这样设计"。

## 代码风格

**z42c（编译器）**：用 z42 写（`src/compiler/z42c.*`）；新代码一律 z42、不退回 C#；具体风格参照 [`src/compiler/README.md`](../src/compiler/README.md) 与 [`docs/design/compiler/compiler-architecture.md`](../docs/design/compiler/compiler-architecture.md)

**Rust（VM）**：`anyhow::Result` + `thiserror`；非测试代码不用 `unwrap()`；公开类型加 `#[derive(Debug)]`

## 代码组织（必须遵守）

完整规则见 `.claude/rules/code-organization.md`（目录 README、文件/函数/类型行数限制、Rust 测试拆分等）。

## 规范冲突检测（必须遵守）

**Claude 在实施任何变更时，若发现规范文档之间存在冲突、不一致或冗余，必须立即：**

1. **停下来指出冲突**，描述：哪两个文档、哪条规则、冲突点是什么
2. **提出建议的解决方案**（以哪个为准，或如何合并）
3. **等待 User 裁决**后再继续实施
4. **裁决结果同步到所有相关文档**，确保唯一真相来源

> 规范冲突优先级高于当前任务推进，适用于所有规范文档（`CLAUDE.md`、`workflow.md` / `philosophy.md` / `version-bumping.md`、`code-organization.md`、`docs/design/` 等）。

## 事实校正责任（必须遵守）

**Claude 对 User 的判断和意见负有校正责任。** Claude 通常掌握更多实现细节（源码、约束、历史决策、二进制格式、CI 拓扑），因此当 User 的判断、假设或指令**与事实相违背**时，Claude **必须主动提出、摆出依据、与 User 讨论调整**，而不是默默照做导致走偏弄错。

- **不盲从**：User 说的若与代码现状 / 约束 / 已知事实冲突，先停下指出「事实是 X，与你说的 Y 不符」+ 具体依据（file:line / 机制 / 历史决策），再一起调整方案。
- **不甩锅**：不得以「是 User 让我这么做的」为由实施一个 Claude 已知有问题的方案——发现问题而不提 = Claude 的失职。
- **提出 ≠ 抗命**：摆清事实后，最终方向仍由 User 裁决；但裁决必须建立在「事实已摆清」的基础上，而非信息不对称下的误判。
- **与 [规范冲突检测](#规范冲突检测必须遵守) / [设计完整性原则](rules/philosophy.md) 一脉相承**：三者都是「发现不对就停下来摆事实、给依据、再继续」。

## 注意事项

- 编译器已自举为 z42（`src/compiler/z42c.*`）；编译器代码只用 z42 写，不退回 C#
- M4（解释器）全绿前，不填充 JIT/AOT 实现
- L2/L3 特性（Result、Trait、ADT、泛型、Lambda、async 等）不在 L1 阶段引入到规范或代码中

@docs/design/language/language-overview.md
@docs/design/runtime/ir.md
