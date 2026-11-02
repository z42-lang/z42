# 代码组织规范

## 目录 README.md

**每个功能目录必须有一个 `README.md`，描述该目录的职责、核心模块和入口点。**

适用范围：从 `src/` 算第 1 层，到第 3 层为止。

```
src/                          ← 第 1 层，不需要（已有根 README.md）
src/compiler/                 ← 第 2 层 ✓ 需要 README
src/compiler/z42c.semantics/  ← 第 3 层 ✓ 需要 README
src/compiler/z42c.semantics/src/  ← 第 4 层 ✗ 不自动加 README
```

第 3 层的 README 应在"核心文件"或"子目录"表中覆盖其子目录的职责，不依赖子目录的 README。

### README.md 模板

**六段模板（职责 / 功能索引 / 基础用法 / 如何测试验证 / 关联文档 / 核心文件）的唯一 SoT 是
[readme-writing.md](readme-writing.md)**——本文件只管「哪一层目录需要 README」，不复制模板。

何时必须同步 README 见 [doc-system.md 三问](doc-system.md)第 3 问；读代码前先读 README 的规则见
[开工导航](../README.md#开工先找-readme-和书)。

---

## 代码规模限制

### 文件行数

.z42 源文件有软限（建议拆分）与硬限（必须拆分）两档，**数值以编译器内建 `style.fileLines` /
`style.fileLinesHard` 规则为准**（各包清单 `[lints]` 启用，见
[编译期扩展](../../reference/src/toolchain/compile-time-extensions.md)），本文件不抄数值。
测试文件不计入限制，但单个测试文件明显过长时也应按功能拆分。
`.rs` 侧无行数门禁（如需要走 clippy）。

### 函数 / 方法行数

软限与硬限以编译器 `style.fnLines` / `style.fnLinesHard` 为准，包清单 `[lints]` 决定诊断级别。
函数体按非空、非注释行计算，局部函数与 lambda 不计入外层；实现与配置见
[编译期扩展](../../reference/src/toolchain/compile-time-extensions.md)。

### 类型规模（struct / class / impl block）

类型本身不按行数判定，按职责组织：

- 先抽协作对象；职责确属同一大类型时，再用 `partial` 按关注点分片，每片一个文件。
- 同名重载组放在同一片，新重载追加在组尾；不能为摊行数把无关方法塞进新 partial。
- Rust `impl` 按 trait / 职责分块或拆子模块，不设行数门禁。

### 执行方式

新增代码主动检查规模：超过软限时评估拆分，超过硬限时本轮解决。
历史 `[Suppress]` / `#suppress` 豁免保留理由，拆分后删除；禁止给新越界加豁免。
为实现本轮目标而必需的拆分与调用点调整可同 PR；独立的大范围整理另开 `refactor` PR，
不把不相干的重构混进功能变更。
