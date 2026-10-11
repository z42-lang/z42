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
> **⚠️ 局部变量不做块级 shadow**（写任何 z42 代码都适用，见 [common-pitfalls.md §3](common-pitfalls.md)）：
> 嵌套块里的局部变量名不得与外层已声明的同名——编译不报错，静默复用/覆盖外层那个。

---

## 子包结构

z42c 由 **7** 个子包组成（下表），均在 `src/compiler/`——改动前先读对应子包的 `README.md`。
IR 模型 + zbc/zpkg 格式 + 依赖索引在编译器域库 **`z42.package`**（namespace `Z42.IR` / `Z42.Package`；为 REPL 共享）——改 IR/格式/zpkg 后端去 `src/compiler/z42.package`。

| 子包 | 职责 | 关键文件 |
|------|------|---------|
| `z42c.core`| 基础设施：`Span` / `Diagnostic` / `DiagnosticBag` / `DiagnosticCodes` / `LanguageFeatures` | `Span.z42`、`Diagnostic*.z42`、`LanguageFeatures.z42` |
| `z42c.syntax`| **语法层**：Lexer + Parser + AST | `TokenKind.z42`、`Lexer.z42`、`Parser.z42`、`Ast.z42`、`Stmt.z42`、`Decl.z42`、`TypeExpr.z42` |
| `z42c.semantics` | 语义分析（符号收集 + 绑定 + TypeCheck + 校验），不含代码生成；**不得引用** `z42c.emission` | `SymbolCollector.z42`、`TypeChecker.z42`、`BoundExpr.z42`、`FrontEnd.z42` |
| `z42c.emission` | Codegen（Bound→IR，用 `z42.package` 的模型）+ 单文件 / 包编译编排；只对无错误的程序生成 | `IrGen.z42`、`ExprEmitter.z42`、`IrDump.z42`、`CuCompile.z42` |
| `z42c.optimization` | IR → IR 优化管线（只依赖 `z42.package`）+ `Opt` 开关 | `IrOptPipeline.z42`、`IrOptInfo.z42`、`OptSet.z42`、`IrInline.z42` |
| `z42c.pipeline` | 编译管线编排 + 依赖扫描 + workspace 构建 + `CacheStore`（增量缓存） | `BuildSession.z42`、`DepScan.z42`、`WorkspaceBuild.z42`、`CacheStore.z42` |
| `z42c.driver` | CLI 入口（exe） | `Main.z42` |

> IR/zpkg 后端（`IrModule`/`IrInstr`/`ZbcWriter`/`ZpkgWriter`/`ZpkgBuilder`/`TsigReconcile`/
> `DependencyIndex` 等）位于 `src/compiler/z42.package/`——见其 README。清单模型 `.z42.toml` 解析在
> `src/compiler/z42.project`。

---

## Lexer（词法器）

**位置**：`z42c.syntax/src/Lexer.z42`。**风格**：手写扫描器（逐字符 + 前瞻），无外部组合子库（为自举保留最简实现）。

- 主循环 `_lexOne()` 按首字符分派到 `_lexIdent` / `_lexNumber` / `_lexString` / `_lexRawString` / `_lexInterpolated` / `_lexChar` / `_lexSymbol`；trivia（空白 / `//` / `/* */`）单独跳过。
- 关键字识别：`_initKeywords()` 注册到并行数组 `_kwNames` / `_kwKinds`（注册序供 DumpTool / vscode-syntax 生成），`_kwLookup()` 走首字符分桶链 `_kwHead` / `_kwNext`；标识符 lex 后查表，命中即关键字。
- 符号：`_lexSymbol()` 做**最长匹配**（`==` 胜 `=`，`>>>` 胜 `>>` 胜 `>`）。

### Token 类型

**位置**：`z42c.syntax/src/TokenKind.z42`——`static class` + `int` 常量（无 enum）。值仅需互异；顺序沿用既有排列。`Token`（`Token.z42`）= `Kind` + `Text` + `Span`。

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

优先级数值与跨文件依赖的说明**只写在** `ParseTable.z42` 的 `ParseTable.LeftBp()` 及其头注释里
（数值越大越紧；左结合，右操作数用 `bp + 1` 递归；赋值 / 三目 `?:` / `is` `as` / `switch` `with` 同在
`LeftBp()` 表里，led 角色见 `ParseTable.Led()`；后缀为无守卫的最紧一档，见 `PostfixKind`）。
**改优先级**：编辑 `LeftBp()` 对应分支并同步它的头注释——本文件不复列数值，两份必漂。

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

## 新增 IR 指令（opcode）

> ⚠️ **先读这一节：多数情况下你不该加 opcode。** 新增 IR opcode 要改 **~20 个文件**，并**必然 bump
> zbc + zpkg 格式**（strict-pin：minor 不等直接 bail），连带 zbc 6 个 + zpkg 4 个 fixture regen
> （后者无一键 regen）、golden hex 单测重截、触发 ci-bootstrap 两代自举。

**先评估这两条更便宜的路**：

| 路线 | 成本 | 何时适用 |
|---|---|---|
| **复用 `BuiltinInstr` + 常量字符串参数** | ~3 文件，**零格式 bump** | 新语义能表达成「一次 builtin 调用 + 常量串区分」。`BuiltinId` 不进 wire（是进程内下标），表尾追加即可。先例：`__box_prim`（`TypeOpEmitter._emitBox`）、`__sym_available`（`_emitSymAvailable`） |
| **attr-ref 哨兵 + 编译期消解** | ~5 文件，**零格式 bump** | 新特性能在编译期完全消解成既有构造。骑既有 SIGS/param attr-ref blob 通道，未标注符号不追加 → 老 golden 逐字节不变。先例：`$Deprecated` / `$Default` / `$Caller:*` |

同类需求优先选哨兵而非新 flag 位 / opcode，以避开格式-bump 两代自举回归。**拿不准就先问，别默认加 opcode。**

### 真要加 opcode：完整清单

**编译器侧**（`z42.package` + `z42c.optimization` + `z42c.emission`）：

1. `src/compiler/z42.package/src/IrInstr*.z42` — 新 `sealed class XxxInstr : IrInstr`
2. `src/compiler/z42.package/src/BinaryFormat/ZbcFormat.z42`（`static class Op`）— 新 opcode 常量
3. `.../ZbcInstr.z42` — 编码分支（带字符串则同时补 `InternInstrStrings`）
4. `.../ZbcReaderInstr.z42` — 解码分支 + 寄存器上界 / 重映射
5. `src/compiler/z42c.optimization/src/IrOptInfo.z42` — **4 处**：Dst / args 计数 / args 替换 / Dst 改写
6. `src/compiler/z42c.optimization/src/IrEscapeAnalysis.z42` — args 逃逸标记
7. 发射点（`src/compiler/z42c.emission/src/Emission/` 各 `*Emitter.z42`）

**运行时侧**（Rust）：

8. `src/runtime/src/metadata/bytecode/instruction.rs` — `Instruction` variant + `written_reg()`
9. `src/runtime/src/metadata/zbc_reader/opcodes.rs` — `OP_XXX` 常量
10. `src/runtime/src/metadata/zbc_reader/instr_decode.rs` — 解码
11. `src/runtime/src/metadata/resolver.rs` — 若需 per-site token 解析
12. `src/runtime/src/interp/exec_*.rs` — 执行语义。**`exec_instr` 的 match 无 `_` 兜底，必须显式处理**
13. `src/runtime/src/jit/translate/*.rs` — **要么翻译，要么**登记进
    `src/runtime/src/jit/translate/unsupported.rs` 的 `unsupported_reason`（单一真相表）

**格式 bump（强制链）**：新增 opcode **必然** bump zbc + zpkg。9 步同步清单（writer / reader 常量、两个格式页
changelog、两组 fixture regen、golden hex 重截）以 [version-bumping.md](version-bumping.md) 为唯一 SoT——
**本清单不复列**；bump 后在本地怎么重建工具链、重生 fixture 见
[格式 bump 期的本地验证](../../internals/src/devinfra/format-bump.md)。

**文档**：IR 参考页（`docs/internals/src/formats/ir.md`）、`src/compiler/z42.package/README.md`、
`src/runtime/src/interp/README.md`。

### 自举纪律

若 **z42c 自身的源**要使用新 opcode：按 [bootstrap-seed.md](bootstrap-seed.md) 的「support 先行、晚一个
nightly 再 use」拆两个 PR 跨两个 nightly。只加编解码 / 执行能力而不发射 → 无字节变化 → 可单 PR
（`ZbcFormat.z42` 里 0xC0–0xC3 的注释是这个做法的先例）。

### 验证

```bash
cd src/runtime && cargo build --release --bin z42vm
./xtask build compiler
./xtask test                            # 全 stage
./xtask test compiler bootstrap         # 改了语法 / 格式能力必跑
```

---

## 分阶段引入语法 / 格式（自举纪律）

z42c 自身用 z42 写、由**上一个已发布 nightly 的 z42c** 编译。因此**新语法 / 新 zbc·zpkg 格式必须 support 先行、晚一个 nightly 再 use**——否则跨版本自举断链。完整纪律 + `xtask test compiler bootstrap` 边界检查见 [bootstrap-seed.md](bootstrap-seed.md)。

特性开关在 `z42c.core/src/LanguageFeatures.z42`（`Set(name, on)` / `IsEnabled(name)`）；需要按 phase 门禁某语法时在 Parser 查 `IsEnabled` 并报 feature-disabled 诊断。

---

## 资源加载顺序

加载 zpkg / module / 注册 builtin 的循环（`read_dir` / 容器迭代 + first-wins 写入）必须先按稳定键排序，禁止依赖文件系统 / hash 的"碰巧顺序"。该约束跨语言适用，统一沉淀在 [common-pitfalls.md §1](common-pitfalls.md#1-资源加载顺序必须显式排序)。
