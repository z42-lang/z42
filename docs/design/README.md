# docs/design/

z42 长期设计文档。变更归档时（`docs/spec/changes/<name>/` → `docs/spec/archive/`），相关 design doc 必须同步更新（参见 [`.claude/rules/workflow.md`](../../.claude/rules/workflow.md) 阶段 9）。

## 目录布局

| 子目录 | 受众 | 内容 |
|------|------|------|
| [`language/`](language/) | 语言用户 + 编译器开发者 | 语法、内置语义、类型系统、FFI 表面 |
| [`compiler/`](compiler/) | 编译器开发者 | z42c 自举编译器内部架构、编译产物策略、工程文件、错误码体系 |
| [`runtime/`](runtime/) | VM 开发者 | Rust VM 架构、IR / zbc 格式、执行模式、JIT / hot-reload / GC、嵌入与跨平台 |
| [`stdlib/`](stdlib/) | stdlib 设计者 | 三层架构、包边界、缺失包排期 |
| [`testing/`](testing/) | 测试基础设施 | z42.test 框架、测试运行器、跨平台测试 |
| [`philosophy.md`](philosophy.md) | 全部 | 设计准则与目标受众（顶层不动）|

## 文档风格模板（2026-05-10 起）

文档分三种风格，新写或重写时参照：

### 模板 A — 长设计（spec 风）

适用：核心特性，含设计权衡（generics / closure / delegates-events / interop / static-abstract-interface 等）。

```markdown
# <Feature>

## Status
- Phase: L1 / L2 / L3
- 锁定日期: YYYY-MM-DD

## Why
<动机>

## Design Decisions
### Decision 1: <标题>
<选项 + 决定 + 理由>

## Syntax / Mechanism
<语法或机制描述>

## Compiler Pipeline Mapping
<Lexer / Parser / TypeChecker / IrGen / VM 各阶段如何承载>

## Runtime / IR
<IR 指令 / VM 行为映射，如有>

## Examples
<代码片段>

## Deferred / Future Work
<对应 roadmap.md "Deferred Backlog Index" 索引项>
```

### 模板 B — 短规范

适用：单一语法 / 实现规则（parameter-modifiers / properties / arrays / foreach / string-builtins / compound-assign / exceptions / object-protocol / namespace-using / access-control / customization / iteration / syntax-config 等）。

```markdown
# <Feature>

## Status
- Phase: L1 / L2 / L3
- 锁定日期: YYYY-MM-DD

## Syntax
<语法定义 / desugar 规则>

## Semantics
<语义>

## Pipeline Mapping
| 阶段 | 内容 |
|------|------|
| Lexer | ... |
| Parser | ... |
| TypeChecker | ... |
| IrGen | ... |
| VM | ... |

## Limits / Phase
<当前 phase 不支持的子功能>

## Deferred / Future Work
<延后项>
```

### 模板 C — 参考手册

仅 [`language/language-overview.md`](language/language-overview.md) 一份；按主题章节组织 + 大量代码示例，详细机制链接到模板 A/B 文档。

## 跨引用规则

- 同子目录内：相对路径（`./generics.md`）
- 跨子目录：相对路径（`../compiler/error-codes.md`）
- 顶层引用：相对路径（`../philosophy.md`）

## 延后特性管理

延后项一律写在对应文档的 "Deferred / Future Work" 段，并在 [`docs/roadmap.md`](../roadmap.md) "Deferred Backlog Index" 加索引行。详见 [philosophy.md "延后特性管理"](../../.claude/rules/philosophy.md#延后特性管理必须遵守)。
