---
paths:
  - "src/compiler/**/*.z42"
---

# z42c 自举编译器开发规范

> z42 的编译器 z42c **用 z42 语言自身写**（`src/compiler/z42c.*`，编译为 zpkg）。
> 本文件沉淀 Lexer / Parser / AST / 诊断 的约定，便于改动时定位。Rust VM 侧见
> [runtime-rust.md](runtime-rust.md)；`.zbc` / `.zpkg` 二进制格式版本 bump 见
> [version-bumping.md](version-bumping.md)。
>
> **受限写法**：z42c 源码避开 z42 尚未完整支持的特性——无 `enum`（用 `static class` +
> `int` 常量替代，见 `TokenKind` / `DiagnosticCodes`）、泛型字段倾向用 typed array。
> 写编译器代码时沿用这套既有写法，不要引入新依赖。
>
> **⚠️ 局部变量非块作用域 shadow（2026-07-09 踩坑）**：z42 的局部变量**不做块级 shadow**
> ——在内层 `while`/`if` 块里 `string name = ""` 会**复用/覆盖**外层同名 `name`，不是新建
> 影子变量。案例：`ZpkgReader.Open` 内层 STRS 解码循环用了 `name` 逐串拼接，污染了外层
> META 包名 `name`（→ `z.Name` 变成池里最后一个串）→ `_isPrelude(z.Name)` 恒 false → prelude
> 包（z42.core）永不激活 → 所有 no-`using`/bare 名（`Assert` 等）解析回 null → 136 个无 using
> 的 golden 全红。**自举不动点抓不到**（z42c 源码全用显式 using，不走 prelude 路径）。
> 铁律：**函数内嵌套块里的局部变量，名字不得与外层已声明的局部同名**（编译不报错、静默复用）。

---

## 子包结构

`src/compiler/` 下 **5** 个独立 workspace 子包（按依赖序）。改动前先读对应子包的 `README.md`。
IR 模型 + zbc/zpkg 格式 + 依赖索引已下沉 stdlib 库 **`z42.ir`**（namespace `Z42.IR` / `Z42.Project`
不变；converge-z42c-ir-metadata-onto-stdlib，为 REPL 共享）——改 IR/格式/zpkg 后端去 `src/libraries/z42.ir`。

| 子包 | 职责 | 关键文件 |
|------|------|---------|
| `z42c.core` | 基础设施：`Span` / `Diagnostic` / `DiagnosticBag` / `DiagnosticCodes` / `LanguageFeatures` | `Span.z42`、`Diagnostic*.z42`、`LanguageFeatures.z42` |
| `z42c.syntax` | **语法层**：Lexer + Parser + AST | `TokenKind.z42`、`Lexer.z42`、`Parser.z42`、`Ast.z42`、`Stmt.z42`、`Decl.z42`、`TypeExpr.z42` |
| `z42c.semantics` | 类型检查（符号收集 + TypeCheck）+ Codegen（Bound→IR，用 `z42.ir` 的模型） | `SymbolCollector.z42`、`TypeChecker.z42`、`Bound.z42`、`ExprEmitter.z42`、`IrGen.z42` |
| `z42c.pipeline` | 编译管线编排 + 依赖扫描 + workspace 构建 + `CacheStore`（增量缓存） | `PipelineSkeleton.z42`、`DepScan.z42`、`WorkspaceBuild.z42`、`CacheStore.z42` |
| `z42c.driver` | CLI 入口（exe） | `Main.z42` |

> IR/zpkg 后端（`IrModule`/`IrInstr`/`ZbcWriter`/`ZpkgWriter`/`ZpkgBuilder`/`TsigReconcile`/
> `DependencyIndex` 等）现在 `src/libraries/z42.ir/`——见其 README。清单模型 `.z42.toml` 解析在
> `src/libraries/z42.project`（converge-z42c-onto-z42-project）。

---

## Lexer（词法器）

**位置**：`z42c.syntax/src/Lexer.z42`。**风格**：手写扫描器（逐字符 + 前瞻），无外部组合子库（为自举保留最简实现）。

- 主循环 `_lexOne()` 按首字符分派到 `_lexIdent` / `_lexNumber` / `_lexString` / `_lexRawString` / `_lexInterpolated` / `_lexChar` / `_lexSymbol`；trivia（空白 / `//` / `/* */`）单独跳过。
- 关键字识别：`_initKeywords()` 注册到并行数组 `_kwNames` / `_kwKinds`（注册序供 DumpTool / vscode-syntax 生成），`_kwLookup()` 走首字符分桶链 `_kwHead` / `_kwNext`（perf-compiler-lookup-tables）；标识符 lex 后查表，命中即关键字。
- 符号：`_lexSymbol()` 做**最长匹配**（`==` 胜 `=`，`>>>` 胜 `>>` 胜 `>`）。

### Token 类型

**位置**：`z42c.syntax/src/TokenKind.z42`——`static class` + `int` 常量（无 enum）。值仅需互异；顺序沿用历史便于对照。`Token`（`Token.z42`）= `Kind` + `Text` + `Span`。

### 新增词法元素 → 改哪里

| 新增什么 | 改的文件 | 加什么 |
|----------|---------|--------|
| 新关键字 | `TokenKind.z42` + `Lexer.z42` | ① 新 `int` 常量；② `_initKeywords()` 加一行 `this._kw("name", TokenKind.Name);` |
| 新符号 / 运算符 | `TokenKind.z42` + `Lexer.z42` | ① 新 `int` 常量；② `_lexSymbol()` 的最长匹配分支加判别 |
| 新数字格式（如八进制） | `Lexer.z42` | `_lexNumber()` 在 hex/bin 判别后加分支 |
| 新数字后缀 | `Lexer.z42` | 扩 `_lexNumber()` 后缀消费逻辑 |
| 新字符串前缀 | `Lexer.z42` | `_lexOne()` 字符串分派前加前缀检测（参考 `_lexInterpolated` `$"..."`） |

---

## Parser（语法器）

**位置**：`z42c.syntax/src/Parser.z42`。**风格**：表达式用 **Pratt 优先级爬升**，语句 / 声明用递归下降，类型统一走 `_parseType()` 产 `TypeExpr`。

三个公开入口：`ParseExpression()` → `Expr`、`ParseStatement()` → `Stmt`、`ParseCompilationUnit()` → `CompilationUnit`（顶层）。

### AST 节点

class 继承层次（**非** record；每个节点带 `Span Span` 用于错误报告，带 `virtual Dump()` 产 s-expr 供 golden 对账）：

| 基类 | 文件 | 子类举例 |
|------|------|---------|
| `Expr` | `Ast.z42` | `IntLitExpr` / `IdentExpr` / `UnaryExpr` / `BinaryExpr` / `MemberExpr` / `CallExpr` / `IndexExpr` / `AssignExpr` / `TernaryExpr` / `ObjNewExpr` / `IsExpr` / `AsExpr` / `CastExpr` / `TypeofExpr` / `LambdaExpr` … |
| `Stmt` | `Stmt.z42` | `ExprStmt` / `BlockStmt` / `IfStmt` / `WhileStmt` / `ForStmt` / `ForeachStmt` / `SwitchStmt` / `TryStmt` / `ReturnStmt` / `VarDeclStmt` … |
| `Decl` | `Decl.z42` | `CompilationUnit` / `UsingDecl` / `ClassDecl`（`Kind` 区分 class/struct/interface）/ `FieldDecl` / `MethodDecl` / `ConstructorDecl` / `EnumDecl` / `Param` / `AttributedDecl` … |
| `TypeExpr` | `TypeExpr.z42` | `NamedType`（`Name` + 泛型 `Args`）/ `ArrayType` / `NullableType` / `FuncTypeExpr`；`TypeParamList` / `WhereClause` |

### 运算符优先级（Pratt binding power）

二元运算符优先级在 `Parser.z42` 的 `_infixBp()`（数值越大越紧；左结合，右操作数用 `bp + 1` 递归）：

```
30  ||            40  &&
44  |             46  ^            48  &      (bitwise)
50  == !=         60  < <= > >=    65  << >>  (bitwise)
70  + -           80  * / %
```

赋值 / 三目 `?:` / `??` / 后缀（`.` `?.` `()` `[]` `++` `--`）/ `is` `as` 不在 `_infixBp` 表里，由 `_parseExpr` 的前缀 / 后缀分支与赋值解析单独处理。**改优先级**：编辑 `_infixBp()` 对应分支。

### 新增语法 → 改哪里

| 新增什么 | 改的文件 | 加什么 |
|----------|---------|--------|
| 新表达式节点 | `Ast.z42` + `Parser.z42` | ① `sealed class XxxExpr : Expr`（带 `Span` + `Dump()`）；② `_parseExpr` 前缀/后缀分支分派 |
| 新语句 | `Stmt.z42` + `Parser.z42` | ① `sealed class XxxStmt : Stmt`；② `ParseStatement()` 加分派 |
| 新声明 | `Decl.z42` + `Parser.z42` | ① `sealed class XxxDecl : Decl`；② 顶层 / 成员解析加分派 |
| 新类型形式 | `TypeExpr.z42` + `Parser.z42` | ① `sealed class XxxType : TypeExpr`；② `_parseType()` 加分支 |

### 错误处理

**不抛异常**：Parser 持 `DiagnosticBag`，遇错调 `_diags.Error(code, msg, span)` 累积后继续。错误码集中在 `z42c.core/src/DiagnosticCodes.z42`（`E02xx` = Parser）。消息面向语言使用者：`expected '(' but got '{token}'`，不暴露内部枚举名。

---

## 分阶段引入语法 / 格式（自举纪律）

z42c 自身用 z42 写、由**上一个已发布 nightly 的 z42c** 编译。因此**新语法 / 新 zbc·zpkg 格式必须 support 先行、晚一个 nightly 再 use**——否则跨版本自举断链。完整纪律 + `xtask test bootstrap` 边界检查见 [bootstrap-seed.md](bootstrap-seed.md)。

特性开关在 `z42c.core/src/LanguageFeatures.z42`（`Set(name, on)` / `IsEnabled(name)`）；需要按 phase 门禁某语法时在 Parser 查 `IsEnabled` 并报 feature-disabled 诊断。

---

## 资源加载顺序

加载 zpkg / module / 注册 builtin 的循环（`read_dir` / 容器迭代 + first-wins 写入）必须先按稳定键排序，禁止依赖文件系统 / hash 的"碰巧顺序"。该约束跨语言适用，统一沉淀在 [common-pitfalls.md §1](common-pitfalls.md#1-资源加载顺序必须显式排序2026-05-17-强化)。
