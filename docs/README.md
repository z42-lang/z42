# docs/

z42 项目文档总入口。

> **文档体系正在重构**（2026-07）。目标结构与四类文档职责、迁移待办见
> [`agent/rules/doc-system.md`](agent/rules/doc-system.md)。新知识一律进 **book**；
> 旧 `design/` 为待迁移旧仓库，迁完即删。

## 📖 知识库 book —— 系统"是什么"的唯一权威

**在线阅读（带侧栏+搜索的完整书）：<https://codesigner-ui.github.io/z42/>**
（每次 push 改动 `book/` 自动重新发布；源码即 markdown，也可直接在 GitHub 翻 [`book/src/`](book/src/)）

| 部分 | 概览页 | 涵盖 |
|------|--------|------|
| 前言 | [`book/src/README.md`](book/src/README.md) | 这本书是什么 + 知识上浮约定 |
| 第一部分 · 语言 | [`book/src/language/`](book/src/language/) | 语法 / 类型系统 / 内存模型 / 内置协议 / FFI |
| 第二部分 · 编译器 | [`book/src/compiler/`](book/src/compiler/) | 自举与种子 / pipeline / zpkg 产物 / 错误码 |
| 第三部分 · 运行时 | [`book/src/runtime/`](book/src/runtime/) | 执行模型 / IR·zbc / GC / 嵌入·跨平台 / native ABI |
| 第四部分 · 标准库 | [`book/src/stdlib/`](book/src/stdlib/) | 三层架构 / 包边界 / 核心包索引 |
| 第五部分 · 工具链 | [`book/src/toolchain/`](book/src/toolchain/) | 面向用户：launcher / workload 平台发行 / SDK 布局 |
| 第六部分 · 开发基础设施 | [`book/src/dev/`](book/src/dev/) | 面向仓库开发：xtask / 构建编排 / 测试门禁 / 打包引擎 |
| 附录 | [`book/src/appendix/`](book/src/appendix/) | 测试框架 / REPL 等独立主题 |

> 本地渲染：`cargo install mdbook` 后于 `book/` 跑 `mdbook serve --open`。
> 各概览页带**迁移状态表**，追踪旧 `design/` 各文档迁入进度（⬜/🟡/✅）。

## 顶层文件

| 文件 / 目录 | 受众 | 内容 |
|------------|------|------|
| [`book/`](book/) | 对外 | **知识库（mdBook）——系统"是什么"的唯一权威**；见上节 |
| [`features.md`](features.md) | 对外 | 语言特性 catalog（决策、当前 phase 归属） |
| [`roadmap.md`](roadmap.md) | 对外 | 唯一迭代计划：当前焦点 + 下一阶段 + SemVer 路线 + Feature→Version 映射 + Deferred Backlog |
| [`workflow/`](workflow/) | 内部 | 本地构建 / 测试 / CI / release / 调试工作流（按主题分子目录）|
| [`agent/`](agent/) | 内部 | 面向大模型的模型中立协作材料（`rules/` 开发规范…）|
| [`design/`](design/) | 混合 | 🟡 **旧**设计文档（迁移中 → book，迁完删）|
| [`spec/`](spec/) | 内部 | 变更工作区（`changes/` 进行中 + `archive/` 已归档）|
| [`error-codes/`](error-codes/) | 数据 | `Z.json`：Z#### runtime 错误码 catalog（Rust + C# 共享）|

## design/ 子目录

| 子目录 | 内容 |
|------|------|
| [`design/philosophy.md`](design/philosophy.md) | 设计哲学（顶层不动）|
| [`design/language/`](design/language/) | 语法 / 类型系统 / 内置协议 / FFI 表面（20 文件）|
| [`design/compiler/`](design/compiler/) | C# Bootstrap 编译器内部 + 工程文件 + 错误码体系（5 文件）|
| [`design/runtime/`](design/runtime/) | Rust VM 架构 + IR/zbc + 嵌入 + 跨平台（10 文件）|
| [`design/stdlib/`](design/stdlib/) | 三层架构 + 包边界 + 缺失包排期（3 文件）|
| [`design/testing/`](design/testing/) | z42.test 框架 + runner + 跨平台测试（3 文件）|

每个子目录有自己的 `README.md` 作为索引；新读者从那进入。

## 文档风格模板（2026-05-10 起）

详见 [`design/README.md`](design/README.md)：

- **A 长设计**（spec 风）：generics / closure / interop / static-abstract-interface 等
- **B 短规范**：parameter-modifiers / properties / arrays / foreach 等
- **C 参考手册**：仅 `language-overview.md`

## 延后特性管理

所有延后项一律就近写入对应 design doc 的 "Deferred / Future Work" 段；`roadmap.md` "Deferred Backlog Index" 横向索引。规则见 [`.claude/rules/philosophy.md`](../.claude/rules/philosophy.md#延后特性管理必须遵守) "延后特性管理"。

## 文档语种策略

z42 仓库文档采用**双语策略**，按受众分流：

- **对外文档**（面向语言用户 / 潜在贡献者 / 公开发布）：**英文**
  - 例：[`features.md`](features.md), [`design/philosophy.md`](design/philosophy.md), [`design/language/language-overview.md`](design/language/language-overview.md), [`design/language/interop.md`](design/language/interop.md), [`design/runtime/hot-reload.md`](design/runtime/hot-reload.md), [`design/runtime/execution-model.md`](design/runtime/execution-model.md), [`design/language/object-protocol.md`](design/language/object-protocol.md), [`README.md`](../README.md)（仓库根）

- **内部文档**（面向 z42 开发者 / 协作工作流 / 实现细节）：**中文**
  - 例：[`workflow/`](workflow/), [`roadmap.md`](roadmap.md), [`design/compiler/compiler-architecture.md`](design/compiler/compiler-architecture.md), [`design/runtime/vm-architecture.md`](design/runtime/vm-architecture.md), [`design/runtime/zbc.md`](design/runtime/zbc.md), [`.claude/CLAUDE.md`](../.claude/CLAUDE.md), [`.claude/rules/*.md`](../.claude/rules/)

写新文档时按此分流；混用注释（中文文件里的英文 code comment、英文文件里对中文术语的注音等）允许，但**主体语言**应一致。当一份对外英文文档需要配套实现细节时，把实现细节单独拆到一份内部中文文档而不是混在一起。
