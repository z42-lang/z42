# 编译器

z42c —— **用 z42 自己写的编译器**，把 `.z42` 源码编成 `.zpkg`。

> 想知道**怎么用** `z42 build` / `z42.toml` 写什么 → [语言与库参考](https://z42-lang.github.io/z42/reference/)。
> 本部分只回答「怎么实现的、为什么这样、在哪改」。

## 代码在哪

| 包 | 位置 | 职责 |
|---|---|---|
| `z42c.core` | `src/libraries/z42c.core/` | `Span` / `Diagnostic` / `DiagnosticCodes` |
| `z42c.syntax` | `src/libraries/z42c.syntax/` | Lexer + Parser（Pratt 表达式）+ AST |
| `z42c.semantics` | `src/compiler/z42c.semantics/` | 符号收集 + 类型检查 + Codegen |
| `z42c.pipeline` | `src/compiler/z42c.pipeline/` | 编译管线编排、依赖扫描、工作区构建、增量缓存 |
| `z42c.driver` | `src/compiler/z42c.driver/` | CLI 入口（= `z42c` 可执行） |

> IR 模型与 zbc/zpkg 后端已下沉 stdlib 库 `z42.ir`；清单解析在 `z42.project`。
> 格式本身见[产物格式](../formats/README.md)。

## 从哪读起

1. [架构总览](architecture.md) —— 全景，先看这页
2. [源代码编译流程](source-compile.md) —— 主力页：Lexer → Parser → TypeCheck → IrGen → Emit
3. [工程模型、依赖解析与工作区编译](project-model.md) —— 一个「工程」怎么变成一次编译
4. [自举与种子](self-hosting.md) —— z42c 自己怎么被编出来；**改构建链前必读**

## 各页

| 页 | 讲什么 |
|---|---|
| [架构总览](architecture.md) | 组件关系与数据流 |
| [源代码编译流程](source-compile.md) | 五个阶段的机制与踩过的坑 |
| [工程模型](project-model.md) | 清单 → 源发现 → 依赖解析 → 工作区拓扑 |
| [自举与种子](self-hosting.md) | warm / cold 种子、两代自举、破环预建 |
| [Binder 层次](binder-hierarchy.md) | 多态 Binder 链（Phase 1 已落） |
| [访问权限强制](access-control.md) | 可见性在哪一步被检查 |
| [构造器继承与隐式 `base()`](ctor-inheritance.md) | 构造链的生成规则 |
| [错误码体系](error-codes.md) | 诊断结构、码段分配、**怎么新增一个码** |
| [脚本化 charter](scripting-charter.md) | 📋 设计已定 / 未实施 |

> 错误码的**全量码表**在参考手册的附录，不在这里——这里只讲「怎么分配、怎么加」。
