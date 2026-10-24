# z42c.syntax

## 职责
语法层（Lexer 词法 + Parser 语法 → AST）。命名空间 `Z42.Syntax`。手写 Lexer + Pratt 表达式 + 递归下降语句/声明（class 继承 + virtual `Dump()` 出 s-expression，受限写法）。依赖 `z42c.core`（Span/Diagnostic）。**不做**语义分析（在 `z42c.semantics`）。

本包是 **host-platform-independent 可移植前端**，供 z42c 编译器与 scripting / playground / runtime 共享；不属于 `Std.*` 标准库 API 面，只是恰好与 stdlib 同处 build + ship。冷启动破环预建见 [self-hosting.md](../../../docs/internals/src/compiler/self-hosting.md) 轴 ④。

## 功能索引
| 功能 | 入口 |
|------|------|
| 词法化 | `Z42.Syntax.Lexer`：`Tokenize()` → `TokenCount()` / `TokenAt(i)` |
| 解析表达式 / 语句 / 编译单元 | `new Parser(src, file)`：`ParseExpression()` → `Expr` / `ParseStatement()` → `Stmt` / `ParseCompilationUnit()` → `CompilationUnit`（均 `.Dump()` 出 s-expression）|
| 前端 dump | `DumpTool.DumpTokens` / `DumpAst` / `DumpDoc`（driver 的 `--dump-tokens` / `--dump-ast` / `--dump-doc` 调用）|
| doc 注释 | Lexer 收集 `DocBlock`（`DocBlockCount()` / `DocBlockAt(i)`）；解析后读 `Decl.Doc` / `EnumMember.Doc` / `CompilationUnit.Doc`，孤立块 = `CompilationUnit.DocBlocks` 里 `Claimed` 为假的 |
| 输入不完整判定 | Parser 的 `IncompleteAtEof`（REPL 续读依据）|
| 诊断局部抑制 | `SuppressionSet`（`#suppress` 字节区间 + `[Suppress]` 声明活跃栈；analyzer 与语义层共用）|

## 如何测试验证
```bash
./xtask test stdlib z42c.syntax    # tests/{lexer,decl,doc,parser,stmt,stmt_order,using_stmt,error_recovery,feature_gates,parse_table,incomplete_at_eof,dump}.z42
```
`xtask test compiler` 只认 `tests/<unit>/*.z42.toml` 目录单元，不覆盖本包的扁平 `tests/*.z42`。

## 关联文档
- 设计 / 机制：[self-hosting.md](../../../docs/internals/src/compiler/self-hosting.md)、[architecture.md](../../../docs/internals/src/compiler/architecture.md)
- 可定制语法：[syntax-customization.md](../../../docs/internals/src/compiler/syntax-customization.md)

## 待办
- 字符串转义的数字 / Unicode（`\uXXXX`）解码未实现（当前支持 C 系单字符转义全集）

## 核心文件
### 词法
| 文件 | 职责 |
|------|------|
| `src/TokenKind.z42` | token 类型常量（int）|
| `src/Token.z42` | 词法 token（Kind / Text / Span）|
| `src/Lexer.z42` | 手写词法器：trivia 跳过 + 标识符/关键字 + 数字（十进制/hex/bin + `_` 分隔 + 小数/指数 + 后缀）+ 字符串/字符/raw `"""`/插值 `$"` + 全符号最长匹配 + EOF |
| `src/DocBlock.z42` | doc 注释块（`///` 外部 / `//!` 内部）：词法器收进旁挂表，解析器在声明入口按 token 下标认领 |
| `src/LexerEscapes.z42` | 字符串转义解码与校验（C 系单字符全集 `\a\b\f\n\r\t\v\0\\\"\'`；未知转义报 E0102）；`Lexer.DecodeString` 转发到此 |

### AST
| 文件 | 职责 |
|------|------|
| `src/TypeExpr.z42` | 类型表达式 AST（NamedType/ArrayType/NullableType）+ TypeParamList（形参 `<T>`）+ WhereClause/WhereConstraint（泛型约束）|
| `src/Ast.z42` | 表达式 AST（字面量/标识符/一元/二元/成员/调用/索引/赋值/三目/new/lambda 等；`IsPatternExpr` = is 结构化模式）|
| `src/AstVisitor.z42` | 语法树 visitor 接口 `IExprVisitor<R, D>` / `IStmtVisitor<R, D>` / `IPatternVisitor<R, D>` / `ITypeExprVisitor<R, D>`：每节点一个 `Visit<节点类名>`，节点经抽象 `Accept<R, D>(v, d)` 双分派；穷举遍历实现接口即由编译器保证完备（新节点漏接 = 编不过）|
| `src/Stmt.z42` | 语句 AST（expr/var-decl/return/if/while/block/break/continue/throw/foreach/for/do-while/switch/try-catch-finally；SwitchCase/SwitchArm 持 Pattern + 守卫）|
| `src/Pattern.z42` | 模式 AST（Wildcard/Constant/Name/Positional/Property）——switch / is 结构化共用 |
| `src/Decl.z42` | 声明 AST（CompilationUnit 含 `SuppressRegions` 局部抑制区间 / Using / Class·Struct·Interface / Enum / Delegate / Field / Method（`IsFree` = 顶层 func）/ Property / Param / Attr；类型用法位均为 TypeExpr）|
| `src/Modifiers.z42` | 声明修饰符位标志 `Mod.*` + `Mod.Parse` / `Mod.Has`：各声明构造时把 `Mods` 串解析进 `ModFlags`（属性另有 `GetModFlags` / `SetModFlags`），下游一律按标志判断 |
| `src/TupleArity.z42` | 元组元数上下界（2..8）与越界消息的唯一定义：类型位（`TypeParser`）与元组字面量（`ConstructTyper`）共用，同码 E0402 同措辞 |

### 解析器
| 文件 | 职责 |
|------|------|
| `src/Parser.z42` | 解析器主体：游标原语 + 共享状态；顶层 `ParseCompilationUnit` / `ParseStatement` / `ParseExpression`；`#suppress <Id> ["reason"]` / `#restore <Id>` 局部抑制指令在语句 / 顶层声明列表边界拦截，收集成 `CompilationUnit.SuppressRegions` |
| `src/ExprParser.z42` | Pratt 表达式（后缀/赋值/三目/is·as/new）+ `(UserType)operand` cast 消歧 |
| `src/ExprParserInterp.z42` | 插值串 `$"…"` 解析（`partial`，与 ExprParser 同一个类）|
| `src/ParseTable.z42` | 表达式规则表：绑定力 / led 角色 / 特性门的唯一真相源 |
| `src/StmtParser.z42` | 递归下降语句 |
| `src/DeclParser.z42` / `src/MemberParser.z42` | 顶层声明（class·struct·interface/enum/delegate/顶层 func/field）与类成员（method/ctor/property/类型位置参数 `Foo(int X)`/泛型形参与 where/前置 attribute/`partial`/用户转换运算符 `implicit`·`explicit operator`/事件合成）|
| `src/TypeParser.z42` | 类型位置解析 |
| `src/PatternParser.z42` | 模式子解析器 `_parsePattern`：名字形状分流；is 结构化前瞻 `_isPatternLead` |
| `src/MethodOfParser.z42` | `methodof(...)` 括号内的签名语法解析 |

### 其它
| 文件 | 职责 |
|------|------|
| `src/DumpTool.z42` | 前端 dump 纯函数（`DumpTokens`/`DumpAst`/`DumpDoc`：源码 → token 流 / AST s-expr / 声明的 doc 挂靠）|
| `src/SuppressionSet.z42` | 诊断局部抑制集：`AddRegion`（`#suppress` 区间，来自 `CompilationUnit.SuppressRegions`）/ `PushActive`·`PopActive`（`[Suppress]` 声明活跃栈）/ `IsSuppressed`；`Directive` = 指令 attribute 名。`z42c.plugin`（analyzer 诊断）与 `z42c.semantics`（`[Deprecated]` 告警）共用 |

## 依赖关系
`z42c.core`。stdlib 自动可用。
