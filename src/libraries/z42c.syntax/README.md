# z42c.syntax

## 职责
语法层（Lexer 词法 + Parser 语法 → AST）。命名空间 `Z42.Syntax`。手写 Lexer + Pratt 表达式 + 递归下降语句/声明（class 继承 + virtual `Dump()` 出 s-expression，受限写法）。依赖 `z42c.core`（Span/Diagnostic）。

> **位置（converge-z42-syntax-lib，route A 地基）**：本包是 **host-platform-independent 可移植前端**，已从 `src/compiler/` 挪进 `src/libraries/`，成 z42c 编译器**与** scripting/playground/runtime 共享的可移植库。**包名/命名空间不变**（仍 `z42c.syntax` / `Z42.Syntax`）——非 Std/z42.* 标准库 API 面，只是恰好与 stdlib 同处 build+ship。冷启动破环预建见 [self-hosting.md](../../../docs/design/compiler/self-hosting.md) 轴 ④。

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/TokenKind.z42` | token 类型常量（int；镜像 C# enum TokenKind）——含 `readonly`(150) / `const`(151) 字段修饰关键字 + `implicit`(59) / `explicit`(60) 转换运算符修饰关键字 |
| `src/Token.z42` | 词法 token（Kind / Text / Span）|
| `src/Lexer.z42` | 手写词法器：trivia 跳过 + 标识符/关键字 + 数字（十进制/hex/bin + `_` 分隔 + 小数/指数 + 后缀）+ 字符串/字符/raw `"""`/插值 `$"` + 全符号最长匹配 + EOF；`DecodeString` 转义解码（C 系单字符全集 `\a\b\f\n\r\t\v\0\\\"\'`）+ 未知转义报 E0102 |
| `src/TypeExpr.z42` | 类型表达式 AST（TypeExpr + virtual Dump；NamedType/ArrayType/NullableType；Dump 复刻规范 type-text）+ TypeParamList（形参 `<T>`）+ WhereClause/WhereConstraint（泛型约束）|
| `src/Ast.z42` | 表达式 AST（Expr + virtual Dump；字面量/标识符/一元/二元含位运算·??·is·as/成员/调用/索引/赋值·复合/三目/new；is·as·new 的类型为 TypeExpr；`IsPatternExpr` = is 结构化模式）|
| `src/Stmt.z42` | 语句 AST（Stmt + virtual Dump；expr/var-decl/return/if/while/block/break/continue/throw/foreach/for/do-while/switch/try-catch-finally；SwitchCase/SwitchArm 持 Pattern + 守卫 Guard）|
| `src/Pattern.z42` | 模式 AST（Pattern + virtual Dump；Wildcard/Constant/Name/Positional/Property）——switch / is 结构化共用（模式匹配核心 A1）|
| `src/PatternParser.z42` | 模式子解析器 `_parsePattern`：名字形状分流（字面量/`_`/点分名→常量·`(`位置·`{`属性·ident 类型+绑定·单裸名）；is 结构化前瞻 `_isPatternLead` |
| `src/Decl.z42` | 声明 AST（CompilationUnit[含 `SuppressRegions` 局部抑制区间]/Using/Class·Struct·Interface[Kind 区分]/Enum+EnumMember/Delegate/Field/Method[IsFree=顶层 func]/Property/Param/ParamList/TypeList/Attr+AttributedDecl + `SuppressRegion`{RuleId,Start,End}（PR3c，`#suppress` 收集，AST-only）+ Dump；类型用法位均为 TypeExpr）|
| `src/Parser.z42` | Pratt 表达式（含后缀/赋值/三目/is·as/new）+ 递归下降语句（含 for/switch/try）+ 顶层声明（class·struct·interface/enum/delegate/顶层 func/field/method/ctor/property + 类型位置参数 `Foo(int X)`（`[Record]`→public 字段 / 无→private 主构造器）+ 泛型形参 `<T>`/where + 前置 attribute `[X]` + `partial` 修饰符（类型 + 方法，方法可无 body）+ 用户转换运算符 `implicit/explicit operator T(S)`（MemberParser → op_Implicit/op_Explicit）+ `(UserType)operand` cast 消歧（ExprParser._castOperandStart）+ **`#suppress <Id> ["reason"]` / `#restore <Id>` 局部抑制指令**（PR3c，语句/顶层声明列表边界拦截 `Hash` → 收集成 `CompilationUnit.SuppressRegions`；非指令 `#` 落回错误路径））|
| `src/DumpTool.z42` | 前端 dump 纯函数（`DumpTokens`/`DumpAst`：源码→token 流/AST s-expr）；供 z42c driver `--dump-*` 调用 + [Test] 验证 |
| `src/SyntaxSkeleton.z42` | **过渡占位**：semantics/pipeline 仍引用；各自移植时移除（driver 已切真实依赖）|

## 入口点
`Z42.Syntax.Parser`（`new Parser(src,file)`）：`ParseExpression()` → `Expr` / `ParseStatement()` → `Stmt` / `ParseCompilationUnit()` → `CompilationUnit`（均 `.Dump()` 出 s-expression）；`Z42.Syntax.Lexer`：`Tokenize()` → `TokenCount()`/`TokenAt(i)`。
测试：`tests/{lexer.z42 27, decl.z42 33, parser.z42 23, stmt.z42 21, incomplete_at_eof.z42 8,
dump.z42 2}`（共 114 个 `[Test]`），经 **`xtask test stdlib z42c.syntax`**（本库住
`src/libraries/`，走 stdlib [Test] 门禁；`xtask test compiler` 只扫
`src/compiler/<member>/tests/`，扫不到这里——2026-09-06 前这些单元因此从未运行，
见 change `tidy-test-layout`）。
**z42c driver 已接前端**：`z42c --dump-tokens|--dump-ast <file.z42>`（调用 `DumpTool`）——自举编译器前端作为真实 CLI 可跑（0.3.4 lex/parse 解锁）。
**incr 6d 全部完成**：类型全部结构化（TypeExpr + TypeParamList + WhereClause；is/as/new/var-decl/foreach/声明位/catch/where 全切；`_parseTypeText`/`_consumeAngles` 已移除）。
待移植（incr 6e+）：byte-identical 对账（最硬，强依赖 AST 形态）；lambda；Visitor（并入后端 semantics）；转义 `\0`/`\uXXXX` 解码。

## 依赖关系
→ z42c.core。stdlib 自动可用。
