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

`.rs` / `.z42` 源文件有软限（建议拆分）与硬限（必须拆分）两档，**数值以 `xtask check lines` 为准**
（见 [GREEN gate](../../internals/src/devinfra/test-gate.md)），本文件不抄数值。测试文件不计入限制，
但单个测试文件明显过长时也应按功能拆分。

### 函数 / 方法行数

| 限制类型 | 行数 |
|----------|------|
| 软限制（超出需要重新审视） | 40 行 |
| 硬限制（必须提取子函数） | 60 行 |

> 计算方式：函数体内（开括号行与闭括号行之间）的非空、非注释行数；单行体计 0。
> 门禁：`xtask check fnlines`（与 `check lines` 同形的棘轮，基线 `scripts/test/fn-limit-baseline.txt`，
> 见 [GREEN gate](../../internals/src/devinfra/test-gate.md)）——新越界 / 继续膨胀 = 红，软限只提示。

### 类型规模（struct / class / impl block）

**单类型 200 行硬限已废止（2026-10-21）**：它自立规起从未有 lint，而真实代码早已按
「partial 按关注点分片」长成大类型（MemberResolver 8 个 partial ≈2.5K 行、SymbolTable
4 碎片、ExprEmitter 发射簇……）——规则与现实的裂缝只会培养对整个规范的轻视。机械可执行
的约束只有两条：**文件 ≤886 行**（`check lines` 棘轮）与**函数体 ≤60 行**（`check fnlines`
棘轮）。类型本身的规模由下面的**结构约定**管，不做行数判定：

- **大类型必须 `partial` 按关注点分片**，每片一个文件、一片一个名字（`.Subst` / `.Candidates` /
  `.Query` / `.Funcref`……）；同族方法（同名重载组）整组落同一碎片，新重载追加在组尾
  （primary 裸键纪律，见 String.z42 头注的先例）。
- **先抽协作对象、再加 partial**：分片的正当顺序是「职责确实属于同一类型」；仅仅为了摊行数
  把无关方法塞进新 partial 是反模式——那类情况该提取辅助类型（如 `PatCtx` / `ModBodies`）。
- Rust `impl` 块同理：按 trait / 职责拆多个 impl 块或子模块，无行数判定。

### 执行方式

- **机械门禁**：`xtask check lines`（文件）与 `xtask check fnlines`（函数体）都是 GREEN gate 的 stage，按硬限做
  棘轮（怎么判红、基线在哪见 [GREEN gate](../../internals/src/devinfra/test-gate.md)）。拆分后降到上限以下的
  文件 / 函数要从基线里剔除；**禁止把新越界或增长写进基线**——那等于关掉门禁。
- AI 每次新增代码时，**主动检查**所在文件是否超出软限制
- 超出软限制时，**建议**拆分方案但不强制打断当前任务
- 超出硬限制时，**必须**在本次迭代内拆分，不得推迟
- 拆分是独立的 `refactor` PR，不与功能变更混在一起

