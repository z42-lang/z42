# z42c 编译器代码评审报告

> 评审日期:2026-07-05
> 评审范围:`src/compiler/` 全部 7 个子包(z42c.core / syntax / ir / project / semantics / pipeline / driver,共约 3.5 万行)
> 评审角度:代码结构、实现框架合理性、数据驱动、易扩展维护
> 评审基准:`.claude/rules/code-organization.md`(文件硬限 500 行 / 函数硬限 60 行 / 类型硬限 200 行)、`common-pitfalls.md`(确定性排序)、`compiler-z42c.md`(受限写法约定)
> 推进状态:见文末「推进顺序」;每项落地时按 workflow 开 refactor change

---

## 总评

**分层架构本身是健康的**:core → syntax/ir/project → semantics/pipeline → driver 依赖方向无违规;诊断码集中在 `DiagnosticCodes`;zpkg 有 strict-pin 版本校验;`common-pitfalls §1` 的确定性排序在 DepScan(prelude-first + Ordinal 插入排序)和 WorkspaceBuild 都落实到位。

问题不在骨架,集中在三类:

1. **巨型文件/函数大面积违反自家 code-organization 硬限**(11 个文件超 500 行,多个函数超 60 行 2~4 倍)
2. **分派逻辑全靠 if-is 链、缺表驱动**,新增 AST 节点 / IR 指令是散弹式修改(5~6 处平行改点)
3. **同一份知识在多处手工维护**(zpkg 格式布局、StrMap、arity-mangle 规则),漂移风险真实存在

---

## 一、规模硬限违规

`code-organization.md` 规定"超硬限必须在本迭代内拆分",现状违规清单:

### 超 500 行文件(11 个)

| 文件 | 行数 | 超限 | 优先级 |
|------|------|------|--------|
| ~~`z42c.semantics/src/TypeChecker.z42`~~ | ✅ **1937→257**（拆 6 子绑定器 Overload/Member/Stmt/Expr/TypeFactsTc/DeclBinder，全 <500，change `extract-member-resolver` 2026-07-12）| 已解决 |
| ~~`z42c.syntax/src/Parser.z42`~~ | ✅ **1743→265**（拆 5 子解析器 Type/Expr/Stmt/Decl/Member，全 <500，change `split-parser-class` 2026-07-12）| 已解决 |
| `z42c.semantics/src/IrGen.z42` | 1153 | +653 | 🔴 P1 |
| `z42c.semantics/src/ExportedTypeExtractor.z42` | 887 | +387 | 🟠 P2 |
| `z42c.semantics/src/ExprEmitter.z42` | 874 | +374 | 🟠 P2 |
| `z42c.semantics/src/SymbolCollector.z42` | 668 | +168 | 🟠 P2 |
| `z42c.ir/src/BinaryFormat/ZbcWriter.z42` | 667 | +167 | 🟠 P2 |
| `z42c.semantics/src/Bound.z42` | 654 | +154 | 🟡(节点定义文件,可按类别拆) |
| `z42c.semantics/src/FunctionEmitter.z42` | 633 | +133 | 🟠 P2 |
| `z42c.project/src/ZpkgWriter.z42` | 552 | +52 | 🟡 |
| `z42c.ir/src/IrInstr.z42` | 515 | +15 | 🟡 |

### 超 60 行函数(代表性)

| 函数 | 行数 | 位置 |
|------|------|------|
| ~~`TypeChecker._bindExpr`~~ | ✅ **278 → 43**(纯分派;抽 15 个逐节点 binder,change `split-typechecker-fns` 2026-07-12) | TypeChecker.z42 |
| ~~`TypeChecker._bindStmt`~~ | ✅ **218 → 38**(纯分派;抽 11 个逐语句 binder,同 change) | TypeChecker.z42 |
| `Parser._parsePrefix` | **140** | Parser.z42:197 起 |
| `Parser._parseExpr` | **111** | Parser.z42:52 起 |
| ~~`TypeChecker._bindMemberCall`~~ | ✅ **98 → 48**(抽 `_bindInstanceMemberCall`,change `split-typechecker-member-fns` 2026-07-12) | TypeChecker.z42 |
| ~~`TypeChecker._bindMember`~~ | ✅ **80 → 23**(抽 `_bindInstanceMember`+`_bindClassMemberAccess`,同 change) | TypeChecker.z42 |
| `Parser._isVarDeclStart` | 91(7 种 var-decl 形式的 lookahead 混合) | Parser.z42:570 起 |
| `SymbolCollector._fillClass` | 82 | SymbolCollector.z42:497 起 |
| `FunctionEmitter.EmitFunction` | 82 | FunctionEmitter.z42 |
| `TypeChecker._bindLambda` | 77 | TypeChecker.z42:1381 起 |
| `ExprEmitter._emitCall` | 73 | ExprEmitter.z42 |
| `TypeChecker._synthCtors` | 69 | TypeChecker.z42:52 起 |
| `FunctionEmitter._emitTry` | 67 | FunctionEmitter.z42 |
| `TypeChecker._resolveParamsOverload` | 64 | TypeChecker.z42:1743 起 |

### God Class 拆分方向

**TypeChecker(五种职责聚合)**:表达式类型检查、语句绑定、成员解析+虚派发、重载决议、转换规则。
拆分:`ExprTyper` / `StmtBinder` / `MemberResolver` / `ConversionChecker`,TypeChecker 降为协调器(Facade)。

**Parser(五层解析混合)**:表达式(Pratt)、语句递归下降、成员、声明顶层、泛型约束。
拆分:`ExpressionParser` / `StatementParser` / `DeclarationParser` / `TypeParser` + 主类保留三个公开入口(`ParseExpression` / `ParseStatement` / `ParseCompilationUnit`),下游无感知。

**巨型分派函数**:`_bindExpr` 按表达式大类提取(`_bindLiterals` / `_bindOperations` / `_bindCalls` / `_bindConditionals` 等);`_bindStmt` 拆 `_bindLoopStmt` / `_bindControlFlowStmt` / `_bindDeclarationStmt`;`_parsePrefix` 拆 `_parseUnaryPrefix` / `_parseCastPrefix` / `_parsePrimaryPrefix`;`_isVarDeclStart` 按 7 种形式分离子判别函数。

---

## 二、可扩展性:散弹式修改面

### 2.1 新增一条 IR 指令,今天要动 5~6 处

实测修改面(以假想新 opcode 为例,约 30 行散在 4 个文件 + VM 侧):

| 改点 | 文件 | 内容 |
|------|------|------|
| 1 | `z42c.ir/src/IrInstr.z42` | 新 sealed class + `Dump()`(~15 行) |
| 2 | `z42c.ir/src/BinaryFormat/ZbcFormat.z42` | Op 常量(~1 行) |
| 3 | `z42c.ir/src/BinaryFormat/ZbcInstr.z42` | `WriteInstr()` if-is 分支(19–172 行处) + `InternStrings()` 字符串预扫分支(381–410 行处) |
| 4 | `z42c.ir/src/BinaryFormat/ZbcWriter.z42` | `_regtInstr()` REGT 类型收集分支(501–552 行处) |
| 5 | `z42c.semantics/src/ExprEmitter.z42` | codegen 发射逻辑 |
| 6 | VM 侧 `zbc_reader.rs` | 解码(跨仓库同步) |

任何一处漏改 = 运行期 panic 或字节漂移,而字节漂移在自举对账里排查成本极高。

**建议**:建 opcode 元数据表(`OpcodeRegistry`:name / code / 操作数形状 / 是否含字符串操作数),让 `WriteInstr` / `InternStrings` / `_regtInstr` 三个平行 if-is 链统一改为查表迭代操作数。class-per-instruction 本身保留(类型安全),要消灭的是三个平行 switch。**改点从 5 处降到 2 处,代码量约 -70%**。

### 2.2 新增一个表达式节点,要动 5 处

`Ast.z42` 新类 + `Parser` 分派 + `TypeChecker._bindExpr` 分支 + `Bound.z42`(现有 ~65 个 Bound 节点类)+ `ExprEmitter.Emit()`(28 分支 if-is 链),各处还有各自的 `Dump()`。漏掉 codegen 端 → Unknown 指令或 panic。

z42 无泛型/enum 的约束下 visitor 不好做,但至少可以:

- **给节点加 kind-tag 常量表**(仿 `TokenKind` 风格加 `ExprKind`),把"漏改某个消费端"从运行期错误变成可 grep 检查的模式
- syntax README 里"Visitor 延后"的决策建议在 0.3.x 重新评估一次(设计完整性讨论点)

### 2.3 IrInstr 构造直连,重构面大

语义层 74+ 处直接 `new XxxInstr(...)` 构造。若 IrInstr 签名变(字段增删、参数顺序),需同步所有 Emit* 调用点。可选改进:EmitContext 提供 factory 方法(中介),IrInstr 重构只改 factory。优先级低,与 2.1 的 registry 一起做时顺带评估。

### 2.4 派发键稳定化（重载 mangle key 的 bootstrap 敏感性，2026-07-12 记）

**现状**：调用派发走 mangle 名键——`SymbolCollector.regName`(598–611) 给方法算 `RegKey` → `BoundCall.MethodName = ms.RegKey` → emit `CallInstr(dst, "Std.IO.Path.Join$2$string$string", …)` → VM 按字符串名精确查找。（`SIGS` 序列化是**反射**元数据，非派发通道；派发从设计上就走 mangle 名，两者正交。）

**问题**：`RegKey` 规则**兄弟集相关**——唯一→裸 `Name`；多 arity→`Name$arity`；同 arity 重载→`Name$arity$types`。故**给一个「唯一方法」加一个重载，会把它从裸键 re-mangle 成 `Name$arity`**（实证：`Path.Join` 加 params 重载后，`z42.io` 导出键 `Join` → `Join$2`+`Join$1`）。已编译的调用方（288 处，含 9 个 z42c 源文件 + 预编译驱动）指向旧裸键 → 打新库 undefined。这是 bootstrap 敏感变更（`add-params-varargs` 2026-07-01 defer stdlib params 迁移的正因）。

**最终方案 A（稳定键，deferred → roadmap）**：`regName` 改**一律全签名 mangle**（键 = 自身签名纯函数、与兄弟无关；协议豁免名 `ToString`/`Equals`/`GetHashCode`/`GetType`/`get_Item`/`set_Item` 保持裸名——VM 按字面量硬查）→ 键永久稳定，未来加重载零处理。代价：全局改键（巨大字节 diff）+ 自指两代自举过渡（链接约定级、比格式 bump 微妙）+ 硬编码名审计（VM builtin / 反射 by-name / well-known-names / DepIndex / 跨包解析）；且 mangle 本身仍有 Canon 归一碰撞边角（见 TypeChecker:154）。**编译器内、VM 派发不动**。

**2026-07-12 实测关键发现（校正）**：原以为 params 两阶段能「用完整重建 + CI 两代自举吸收 re-mangle」——**错**。实测 `build stdlib` 本地即崩：seed driver（旧、baked 裸 `Join` 直调）跑在新键 z42.io（`Join$2`）上 → `undefined Std.IO.Path.Join`。且 ci-bootstrap 的**两代自举只由格式版本差（zbc/zpkg minor）触发**，纯 key 变更**不 bump 格式 → 触发不了两代 → 不自愈**。「换名兜底」（单 params 重载复用裸键）也不行——seed 的直调 ABI 与 params 打包 ABI 不匹配。

**结论**：给「z42c 消费的 stdlib 方法」加重载是**硬 bootstrap 破坏**，无轻量路径。三条真选项：① **方案 A**（稳定键，本身也要随格式 bump + 两代自举落地）；② **随一次格式 bump 搭两代自举**把 params 一起推（本地不可验、靠 CI）；③ **换名兜底**（变长版另起名如 `Path.JoinAll` / 复用 0-caller 的 `Combine(params)`，`Join` 2-arg 不动 → 零 re-mangle、立即可落）。**2026-07-12 裁决：低频 → 暂缓 params-for-Join，方案 A 记 roadmap「设计期延后」**（它是让这类演进可行的根本解，痛值得时随格式 bump 走 spec-first）。

---

## 三、数据驱动缺口:if 链该表驱动的地方

| 位置 | 现状 | 建议 |
|------|------|------|
| `Parser._infixBp()`(338–350)与 `_isAssignOp()`(352–358) | 语义相关、物理分离的两条 if 链;加新复合赋值运算符要同时改两处 | 合并为单一 `OpInfo` 表(kind / bp / isAssign) |
| `Parser._isModifier`(877–885,11 个 if)/ `_isTypeKeyword`(725–734,7 个)/ `_isVisibilityModifier`(1283–1290) | if 链 | 并行数组查表(项目已有 `_kwNames`/`_kwKinds` 先例,风格统一) |
| `_operatorMethodNameTc` / ~~`_bindBinary` 操作数·结果~~ ✅ / `_isAssignable` | 🟡 **合法操作数/结果已表驱动**（`BinaryTypeTable`+`OperandKind`/`ResultKind`/`BinaryRule.Lookup` 已存在，review 时未成熟）。剩：`_operatorMethodNameTc`（运算符→overload 方法名，7 行小 if）低 ROI；`_isAssignable`（转换规则）as-is 更清晰且表格化风险最高 → 不做 | — |
| 基元类型别名/分类 | 🟡 **已大体集中**：`Z42Type.Canon`（别名归一）+ `TypeFacts`（`_asPrimName`/`IsNumeric`/`IsIntegral`/`_structPrimName`/`_isNumericName` 等）已是事实上的 PrimitiveTypeRegistry。剩 `TypeChecker._isNumericPrim`/`_primWrapper`/`_isPrimKeyword` 与 TypeFacts **语义有别**（`_isNumericPrim` 视 char 为数值、`_isNumericName` 不；`_primWrapper` 是 `_structPrimName` 逆向）→ 合并需行为对账、非机械 dedup，风险>收益 → 暂不做 | — |
| `Main.z42` 命令分派(21–81) | if 链;exit code 无契约(0/1/2 语义靠各函数自觉,`_build` 与 `_buildWorkspace` 各自约定) | 命令表 + `EXIT_OK / EXIT_BUILD_ERROR / EXIT_USAGE_ERROR / EXIT_INTERNAL_ERROR` 常量,文档化契约 |
| `Lexer._kwLookup`(417–424) | 关键字 O(n) 线性查 | 低优先;可改二分(z42 无哈希容器) |

---

## 四、重复知识:单一 SoT 被违反的地方(漂移风险最高)

### 4.1 StrMap.z42 ≈ StrMapIr.z42(相似度 >99%)

`z42c.semantics/src/StrMap.z42`(99 行)与 `z42c.ir/src/StrMapIr.z42`(105 行)字面同构,唯一差异是 StrMapIr 多 `TryAdd`。根因:z42 类字段无泛型 + ir 是叶子包不能依赖 semantics。修 bug 需两处同步。

**建议**:落到公共底层包——若 ir 已依赖 core 则放 `z42c.core`,否则新建 `z42c.common`;过渡期至少在两文件头加"同构、改动必须双写"警示注释。

### 4.2 zpkg 格式布局知识 Writer/Reader 各写一遍

- META 段字段顺序:Writer(187–192)与 Reader(90–103)各自硬编码;NSPC 同理(Writer 195–200 vs Reader 128–136)
- EXPT kind 枚举(func=0/type=1/const=2):Writer 有映射,Reader 无越界校验
- TSIG 编解码:`ZpkgWriter._internTsig`(298–365)与 `ZpkgReader._readImpl`(342–370)是两端互不可见的私有方法,方法字节布局(name/ret/vis/flags/minArg/pc/paramsFrom/params)的一致性只靠注释

对比 `ZbcFormat.z42` 已经做对了(Op/Tag 常量集中),zpkg 侧缺对等物。

**建议**:`ZpkgFormat.z42` 集中段布局常量 + `TsigCodec.z42` 让读写共用同一编解码函数。这正是 version-bumping checklist 想防的漂移——从工具层面消除比 checklist 更根本。

### 4.3 ExportedTypeExtractor 与 SymbolCollector 重复实现 mangle 规则

> 2026-07-12 部分修：Object-4 导出方法（ToString/Equals/GetHashCode/GetType）从 ExportedTypeExtractor + TsigReconcile 两处抽到 `z42c.ir/ObjectMethods.Four()` 单源。剩余（AST 侧 vs IR-binary 侧的完整 class-extraction 逻辑）跨包不同输入，不可共享——保持独立。

ExportedTypeExtractor(887 行)的泛型 arity 预扫(59–75)几乎复制 SymbolCollector._passClassStubs 的检测逻辑;类骨架扫描、字段/方法收集、接口处理均有平行实现。两处规则不同步时,**TSIG 导出键与运行期虚派发键会不一致**——这类 bug 只在跨包调用时爆,排查困难。

**建议**:提取共用 ClassMetadata/mangle 规则工具,或让 Extractor 直接消费 SymbolTable,不再自建 classMap。

### 4.4 小项

- 事件合成逻辑散在 Parser 4 个方法(`_isMulticastEventType` / `_synthEventAccessor` / `_isSinglecastEventType` / `_synthSinglecastAccessor`,953–1035)→ 集中 EventSynthesizer
- zbc section 顺序(NSPC→STRS→TYPE→SIGS→IMPT→EXPT→FUNC→REGT)只存在于 `ZbcWriter` 的调用顺序里 → section 描述表集中(tag / builder / optional)

---

## 五、其他值得记账的点

| 项 | 位置 | 说明 |
|----|------|------|
| Skeleton 死代码 | 6~7 个 `*Skeleton.z42`(Core/Syntax/Ir/Project/Semantics/Pipeline) | B0 期占位残留,grep 无引用,仍在编译加载链;定清理时机 |
| ManifestLoader 异常裸奔 | `ManifestLoader.z42:15` | `TomlValue.Parse` 抛异常时无捕获,格式错误直达顶层而非走 DiagnosticBag |
| Driver 绕过统一诊断渲染 | `Main.z42:155–157` | 直接 `ConsoleError.WriteLine` 遍历诊断;建议 `DiagnosticBag.FormatAll()` 收口(DiagnosticRenderer 本就是 README 待移植项) |
| Parser 无统一错误恢复 | Parser.z42 全文 30 处 `_diags.Error` | 有的 advance 有的不 advance;建议 `_skipToSync(terminators)` 同步集策略 |
| ~~测试缺口~~ | ✅ OverloadResolver 8 单测（change `add-overload-resolver-tests` 2026-07-12）；WorkspaceBuild 环检测已由 `topo_tests.test_topo_cycle_throws` 覆盖 | 已补 |
| 诊断消息文案分散 | TypeChecker 等多处字符串字面量 | 码已集中(DiagnosticCodes)、文案未集中;低优先 |
| Bound.Dump() s-expr 格式无规范 | Bound.z42 | 新节点易格式不一致;在文件头注释或 book 页规范格式 |

---

## 六、做得好的地方(保持)

- 依赖方向:core 无依赖 → syntax/ir/project → semantics/pipeline → driver,无违规
- 确定性排序:`DepScan._sortZpkgKeys`(prelude-first + Ordinal)、`WorkspaceBuild.IndicesByName` / `SortStrings` 全部合规,注释明确指向 common-pitfalls §1
- zpkg strict-pin:Reader 直接引用 Writer 版本常量做精确匹配
- `ZbcFormat.z42` 的 Op/Tag 常量集中模式(zpkg 侧应效仿)
- EmitContext 共享状态模式(寄存器分配/块管理/标签生成集中)
- golden test 框架完善,byte-identical 自举对账(gen2 不动点)为重构提供安全网

---

## 七、推进顺序

全部是 refactor 类变更(不改语义、不引新语法,**不触发 bootstrap-seed 两阶段纪律**;唯一约束:不得使用未随 nightly 发布的 stdlib API——纯重排安全)。按 parallel-development 子系统锁,同子系统项串行,跨子系统可并行:

### P1 — 拆 God Class + 超限函数(硬限违规,规范要求必须拆)

| # | 内容 | 子系统锁 | 状态 |
|---|------|---------|------|
| P1-1 | 拆 TypeChecker(1937 → 6 子绑定器 + Facade 257) | semantics | ✅ 2026-07-12（OverloadBinder/MemberResolver/StmtBinder/ExprTyper/TypeFactsTc/DeclBinder，全 <500，不动点 7/7）|
| P1-2 | 拆 Parser(1743 → 5 子解析器 + mediator 265) | syntax | ✅ 2026-07-12（Type/Expr/Stmt/Decl/Member，全 <500，不动点 7/7）|
| P1-3 | 拆 IrGen(1153)/ ExprEmitter/ FunctionEmitter | semantics | 🟡 ExprEmitter ✅ 2026-08-22（#258，1665→491，拆 Call/TypeOp/Operator/AccessEmitter 四发射簇，hub+spoke）；FunctionEmitter ✅ 2026-08-23（分出 StmtEmitter 语句&控制流簇，841→418，同模型）；**IrGen 待拆**（611，多趟编排器，味道≠dispatch，先诊断）。另 ExprTyper ✅ 2026-08-23（#260，1010→448，拆 Collection/Assign/Construct/TypeOpTyper） |

### P2 — 表驱动化 + 格式单源

| # | 内容 | 子系统锁 | 状态 |
|---|------|---------|------|
| P2-1 | OpcodeRegistry:消灭 ZbcInstr/ZbcWriter 三个平行 if-is 链 | ir | ⬜ |
| P2-2 | OperatorTable + PrimitiveTypeRegistry + 转换规则表 | semantics | 🟡 **大体已存在**（`BinaryTypeTable`/`TypeFacts` 表驱动 operand/result + prim 分类；review 2026-07-05 时未成熟）。剩 `_operatorMethodNameTc`（低 ROI）/ `_isAssignable` 转换表（as-is 更清晰、风险高）/ prim helper 合并（char 语义差、风险>收益）→ 评估后不做，2026-07-12 |
| P2-3 | Parser 修饰符/类型关键字/优先级 OpInfo 表 | syntax | ⬜ |
| P2-4 | Driver 命令表 + exit code 契约常量 | driver | 🟡 2026-07-12 exit-code 契约常量已做；命令表 if→table 部分未做（低 ROI，Main.z42 待 converge）|
| P2-5 | ZpkgFormat.z42 + TsigCodec.z42(读写单源) | project | ⬜ |
| P2-6 | zbc section 描述表 | ir | ⬜(排 P2-1 后) |

### P3 — 债务清理

| # | 内容 | 子系统锁 | 状态 |
|---|------|---------|------|
| P3-1 | StrMap 统一到公共包 | ir + semantics | ✅ 2026-07-12（统一到 z42c.ir，semantics 已依赖 ir，无需 z42c.common）|
| P3-2 | Skeleton 死文件清理 | 各子包 | ✅ 2026-07-12（6 个自引用死簇删除）|
| P3-3 | ExportedTypeExtractor 与 SymbolCollector 共用 mangle 规则 | semantics | ⬜ |
| P3-4 | Parser 错误恢复 `_skipToSync`;ManifestLoader 异常捕获;DiagnosticBag.FormatAll | syntax / project / core+driver | ⬜ |
| P3-5 | 补 OverloadResolver / 环检测测试 | semantics / pipeline | ✅ 2026-07-12（overload 8 单测；环检测既有 topo_tests 已覆盖）|

**设计完整性讨论点(推进 P1 前建议先裁决)**:
1. AST/Bound 分派机制——引入 kind-tag 常量表,还是维持 is/as 现状?(影响 P1 拆分时的接口形态)
2. StrMap 公共包落点——z42c.core 还是新建 z42c.common?(取决于 ir 对 core 的依赖现状)
3. syntax README「Visitor 延后」决策是否在 0.3.x 重估
