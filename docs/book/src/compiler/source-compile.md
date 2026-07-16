# 源代码编译流程（z42c）

> **页型**: 机制页 ｜ **状态**: ✅ 已实现 ｜ **代码**: `src/compiler/z42c.syntax/` · `z42c.semantics/` · `z42c.ir/`
> **相关**: [架构总览](architecture.md) · [工程模型、依赖解析与工作区编译](project-model.md) · [编译产物：zpkg / zbc 格式](format.md) · [CLI 与诊断工具](tools.md) ｜ **对齐**: 2026-07-17

## 概述

单个源文件（或单个包内的源文件）从 `.z42` 编译到 `.zbc` / `.zpkg`，经过五个阶段：**词法 → 语法 → 类型检查 → IR 生成 → 写出**。前四步在内存中逐层抬升表示——从字符流到 Token、到语法树、到带类型的 Bound 树、到 IR，最后序列化成二进制。

```mermaid
graph LR
    S[source.z42] --> L[词法] --> T[Token 流]
    T --> P[语法] --> A[AST·CompilationUnit]
    A --> C[类型检查] --> B[Bound 树 + SemanticModel]
    B --> G[IR 生成] --> M[IrModule]
    M --> E[写出] --> O[.zbc / .zpkg]
```

## 机制

各阶段单向推进，前一阶段的产物是后一阶段的唯一输入。每个阶段都有对应的 `--dump-*` 命令可单独观察其产物（见 [CLI 与诊断工具](tools.md)）。

### 词法（Lexer）

字符流 → Token 流。手写扫描器逐字符读取，将空白、换行、注释作为 trivia 跳过，识别标识符与关键字、数字/字符串/字符/原始串/插值串字面量、以及按最长匹配切分的运算符与符号，末尾补 EOF。

跳过 trivia 使后续语法阶段只面对干净的 Token 序列，无需再关心排版与注释。观察：`--dump-tokens`。

### 语法（Parser）

Token 流 → AST（`CompilationUnit`）。表达式用 Pratt 优先级爬升解析，运算符的结合与优先级由绑定力表驱动，便于增删运算符；语句与声明用递归下降。类型统一经 `_parseType` 产出 `TypeExpr`。

解析器按关注点拆成若干子解析器（表达式、声明、成员、语句、类型），各司其职。AST 节点不可变，为后续并行分析与安全遍历提供基础。观察：`--dump-ast`。

### 类型检查（TypeCheck）

AST → Bound 树 + `SemanticModel`。分两步：先由 `SymbolCollector` 遍历整个编译单元建立符号表，再逐节点定型。先建表使同一单元内的前向引用与互相引用不受书写顺序约束。

定型过程解析每个表达式与语句的类型、校验可赋值性与定义性，并由 `OverloadResolver` 完成方法重载决议、`ConstraintChecker` 校验泛型约束。产物是 Bound 树（每个节点携带解析后的类型）与 `SemanticModel`（供代码生成消费）。观察：`--dump-bound`。

### IR 生成（IrGen）

Bound 树 + `SemanticModel` → `IrModule`。逐个类方法与顶层函数交给 `FunctionEmitter` 发射为寄存器式 IR 函数，汇总类描述与字符串池成 `IrModule`。函数以 `Class.Method`（类方法）或函数名（顶层函数）为键。

代码生成只依赖 `SemanticModel` 这一接口，与前端类型检查解耦。观察：`--dump-ir`。

### 写出（Emit）

`IrModule` → `.zbc` / `.zpkg`。由 `ZbcWriter` 将 IR 序列化为二进制：单文件产出 `.zbc`，打包产出 `.zpkg`。二进制布局与各 section 见 [编译产物：zpkg / zbc 格式](format.md)。

## 实现

| 阶段 | 关键文件 |
|------|---------|
| 词法 | `z42c.syntax/src/Lexer.z42`、`Token.z42`、`TokenKind.z42` |
| 语法 | `z42c.syntax/src/Parser.z42` + `ExprParser` / `DeclParser` / `MemberParser` / `StmtParser` / `TypeParser`；AST：`Ast.z42` / `Decl.z42` / `Stmt.z42` / `TypeExpr.z42` |
| 类型检查 | `z42c.semantics/src/TypeChecker.z42`、`SymbolCollector.z42`、`SymbolTable.z42`、`OverloadResolver.z42`、`ConstraintChecker.z42`；产物：`Bound.z42`、`SemanticModel.z42` |
| IR 生成 | `z42c.semantics/src/IrGen.z42`、`FunctionEmitter.z42`、`ExprEmitter.z42`、`EmitContext.z42`；IR 模型：`z42c.ir/src/IrModule.z42`、`IrInstr.z42`、`IrType.z42` |
| 写出 | `z42c.ir/src/BinaryFormat/ZbcWriter.z42`、`ZbcFormat.z42`、`ZbcInstr.z42` |

## 边界与限制

- **全量解析，无增量**：每次编译完整走一遍词法与语法。文件级增量探测在工作区构建层，见 [工程模型、依赖解析与工作区编译](project-model.md)。
- **单包视角**：本章只讲一个包内源码的编译。跨包符号导入（DependencyIndex、TSIG）由类型检查阶段读取，机制见 [工程模型、依赖解析与工作区编译](project-model.md)。

## Deferred

- 统一的 AST 脱糖阶段：目前少量 AST 改写分散在各处，尚未提取为独立 pass。索引见 `docs/roadmap.md` Deferred Backlog。
