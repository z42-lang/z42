# Proposal: 用内建 `[Record]` attribute 替代 `record` 关键字

> 状态：🟡 DRAFT（待 6.5 gate 确认） | 创建：2026-08-26

## Why

User 想**去掉 `record` 关键字，改用内建 attribute `[Record]`**。理由：C# 的 record 既能是 class
又能是 struct，作为独立关键字与 class/struct 两条身份轴纠缠，很混乱；用 attribute 表达能在
**同时支持 class 和 struct** 的前提下简化语言机制。目标写法：

```z42
[Record] class  Point(int X, int Y)   // 合法
[Record] struct Point(int X, int Y)   // 合法
```

**关键事实（已与 User 摆清）：z42 现在的 `record` 不是 C# 那个 record**，几乎全是 parser 语法糖：

1. `record Point(int X, int Y)` 在解析期降级成 `ClassDecl(Kind="record")`：位置参数 → **public 字段**
   + 合成主构造器（`this.X=X`），见 [DeclParser.z42:260-320](../../../../src/compiler/z42c.syntax/src/DeclParser.z42) `_parseRecord`。
2. zbc TYPE 段 **flag bit3**（`+8`，[ClassDescBuilder.z42:230](../../../../src/compiler/z42c.semantics/src/ClassDescBuilder.z42)），仅被反射
   `__type_is_record`（[reflection.rs:1542](../../../../src/runtime/src/corelib/reflection.rs)）读，**运行时无行为差异**。
3. 不给默认 `Std.Object` 基类（[ClassDescBuilder.z42:110](../../../../src/compiler/z42c.semantics/src/ClassDescBuilder.z42) `isStructOrRecord`），
   语义上仍是**引用类型**（`StubCollector` `IsStruct=Kind=="struct"`，record 不算）。
4. **C# record 的值相等 / ToString / with / init-only / 解构，z42 一个都没实现。** 所以 z42 record
   ≈「带主构造器语法糖 + 一个反射 flag 的 class」。
5. record 是与 class/struct/interface **平行的第 4 种 `Kind`**，互斥、不能和 struct/class 组合。

record 唯一沉淀的东西是「位置参数糖 + 一个 `is-record` 反射标记」。把它从独立关键字改成一根
**正交、opt-in 的 attribute**，作用于 `class` / `struct` 两种身份，正是 User 要的简化——且 z42 已有
attribute 全闭环（[[attribute-handler-registry]] 已收官），无需新建地基。

## What Changes（本次 = 纯「关键字 → attribute」等价替换）

- **新增内建 `[Record]` directive**（保留名，走 [[attribute-handler-registry]] 的 **directive 路**，
  仿 `[Deprecated]`；豁免 D8 `Attribute` 后缀；不序列化成 store-meta blob）。
- **`_parseTypeDecl` 接受可选位置参数 `(params)`**（当前只有 `_parseRecord` 有此语法位置）。位置参数
  **不在 parser 就地展开**——存到 `ClassDecl` 新字段 `PrimaryParams`，留给 AST 期看得到 `[Record]` 的
  pass 展开（parser 看不到自己的 attr，见 design Decision 2）。
- **新增 AST 期 pass `RecordExpand`**（挂在 `HandlerRegistry.RunAst` 单一入口）：把 `PrimaryParams`
  展开为 **字段 + 主构造器**（照搬 `_parseRecord` 逻辑）。**两条分支，仅字段可见性 + bit3 不同**：
  - **有 `[Record]`** → **public** 字段 + ctor，置 `ClassDecl.IsRecord=true`（bit3；替代 `Kind=="record"` 哨兵）= record。
  - **无 `[Record]`** → **private** 字段 + ctor，`IsRecord=false`（无 bit3）= **primary constructor**（C# 12）。
- **primary constructor 靠既有「字段入方法体 scope」机制免 binder 改动**：[DeclBinder.z42:48-54](../../../../src/compiler/z42c.semantics/src/DeclBinder.z42)
  把类的**全部字段**播种进方法体 `env`，[AccessEmitter.z42:420](../../../../src/compiler/z42c.semantics/src/AccessEmitter.z42)
  把裸 `BoundIdent`（命中字段名）发成 `this.X` 字段读——故 `class Point(int X){ int Sum => X; }` 里裸 `X`
  自然解析成私有字段读，**无需改名字解析 / binder**（纯 desugar）。
- **bit3 触发源迁移**：`ClassDescBuilder` 的 bit3 从 `Kind=="record"` 改读 `c.IsRecord`，并**自然扩展到
  struct**（`[Record] struct` = bit2 struct + bit3 record）。**零格式-bump**（bit3 已在格式里），运行时
  `__type_is_record` 一行不改。
- **删除 `record` 关键字**及其全部下游分支（token / lexer / `_parseRecord` / parser·MemberParser 分派 /
  ~22 处 `|| Kind=="record"` 子句 / E0431 文案）。record 身份种类从此不存在——`[Record] class` 走 class
  机制、`[Record] struct` 走 struct 机制。
- **基类语义随 Kind 自然归位**（Decision，User 已接受）：`[Record] class` **拿回默认 `Std.Object` 基类**
  （今天裸 record「无 Object 基」是历史怪癖，且它本就被注入 Object 四方法）；`[Record] struct` 无默认基类
  （同普通 struct）。这是一处可观察语义变化，**不**为 attribute 加「抑制基类」特例。
- **迁移** stdlib / examples / 测试里的 `record X(...)` → `[Record] class X(...)`（见 Scope；**分两 nightly**，
  见「自举纪律」）。

**不在本次做（值语义 Deferred）**：`[Record]` **不**生成 `Equals`/`GetHashCode`/`ToString`/`==` 等值成员。
本次只做等价替换——`[Record] class/struct Foo(params)` 的运行时行为与今天的 `record Foo(params)` 逐字节
一致（引用语义、无值相等）。真值语义是独立 change（见 Out of Scope）。

## Scope（允许改动的文件）

### nightly N —— support（零格式-bump；z42c/stdlib 源码**不使用** `[Record]`）

| 文件 | 变更 | 说明 |
|------|------|------|
| ~~`RecordAttribute.z42`~~ | ~~NEW~~ | **不需要**——directive 靠名字识别、无 backing 类（同 `[Deprecated]`/`[Suppress]`/`[Native]`，实测 stdlib 无这些类）。省一个文件。 |
| `src/compiler/z42c.syntax/src/DeclParser.z42` | MODIFY | `_parseTypeDecl` 接受可选 `(params)` → 存 `ClassDecl.PrimaryParams`；`;` 短形式 body 可选 |
| `src/compiler/z42c.syntax/src/Decl.z42` | MODIFY | `ClassDecl` 加 `Param[] PrimaryParams` + `bool IsRecord` 字段 |
| `src/compiler/z42c.semantics/src/RecordExpand.z42` | NEW | AST 期 pass：`[Record]` → 展开 `PrimaryParams` 为 public 字段 + ctor、置 `IsRecord` |
| `src/compiler/z42c.semantics/src/HandlerRegistry.z42` | MODIFY | `RunAst` 串入 `RecordExpand.Run`；加 `IsRecordDirective`/`HasRecord`（仿 `IsDeprecatedDirective`/`HasDeprecated`）；directive 集 + D8 豁免加 `Record` |
| `src/compiler/z42c.semantics/src/ClassDescBuilder.z42` | MODIFY | bit3 触发 `Kind=="record"` → `c.IsRecord`（:230）；`isStructOrRecord` 去 record 分支（:110，基类随 Kind） |
| （无需改 IrGen 传参）| — | bit3 改读 `ClassDecl.IsRecord`，`_classDesc(ClassDecl)` 签名不变 |
| 新增单测 / golden | NEW | `[Record] class`、`[Record] struct` 解析 + bit3 反射 + 等价性 |

### nightly N+1 —— use + remove（新 nightly 发布后）

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/libraries/z42.build/src/Models.z42` | MODIFY | `record X(...)` → `[Record] class X(...)`（Target/Dirs/Inputs/Output/ExecResult 5 处） |
| `src/libraries/z42.build/src/ICompiler.z42` | MODIFY | CompileRequest/CompileResult 2 处 |
| `examples/patterns.z42` / `examples/oop.z42` | MODIFY | Expr/Num/Add/Mul/Neg/Shape2/Circle2/Rect2 / Point 迁移 |
| `src/compiler/z42c.syntax/tests/decl/decl_tests.z42` | MODIFY | `test_record` 迁移；加「`record` 不再是关键字」负测试 |
| `src/libraries/z42.core/tests/reflection.z42` | MODIFY | `__type_is_record` 断言迁到 `[Record]` |
| `src/compiler/z42c.syntax/src/TokenKind.z42` | MODIFY | 删 `Record=23` |
| `src/compiler/z42c.syntax/src/Lexer.z42` | MODIFY | 删 `_kw("record", ...)` |
| `src/compiler/z42c.syntax/src/DeclParser.z42` | MODIFY | 删 `_parseRecord` |
| `src/compiler/z42c.syntax/src/Parser.z42` / `MemberParser.z42` | MODIFY | 删 record 分派 |
| semantics 各文件（~22 处） | MODIFY | 删 `\|\| Kind=="record"` 子句：MemberCollector/StubCollector/TypeChecker/InheritanceResolver/DeclBinder/ExportedTypeExtractor/IrGen/IrDump/ConstraintChecker/CuCompile/CuPreprocess/TestIndexBuilder |
| `src/compiler/z42c.semantics/src/DeclEnforcer.z42`（E0431） | MODIFY | kind 列表文案去 "record" |

### 文档（nightly N 落地即写）

| 文件 | 变更 |
|------|------|
| `docs/design/language/language-overview.md` | record 段重写为 `[Record]` 语义（删 record struct/class/with/判别联合等**从未实现**的承诺） |
| `docs/book/src/language/record-attribute.md` | NEW 机制页（`[Record]` 语义 + 位置参数展开 + bit3 反射 + 实现原理）+ 挂入 `SUMMARY.md` |
| `docs/design/language/grammar.peg` | record 产生式 → class/struct 的可选 `(params)` |
| `docs/book/src/compiler/zbc-format.md` | bit3 说明：`Kind==record` → `[Record]` attribute |
| 目录 README（z42.semantics / z42.core） | 加 RecordExpand / RecordAttribute |

**只读引用**（理解上下文）：`BenchmarkDesugar.z42`（AST pass 样板）、`AttributeSynth.z42`（seam）、
`HandlerRegistry.z42`（directive 先例 `HasDeprecated`）、`_parseRecord`（展开逻辑源）。

## Out of Scope（Deferred）

- **真值语义**（`Equals`/`GetHashCode`/`ToString`/`==` 值相等生成）——独立 change
  `add-record-value-semantics`。本次 `[Record]` 只是「今天 record 行为」的 attribute 化，不补值相等。
- **primary constructor 的 capture 优化**：C# 12 只在参数被「构造后使用」时才合成 backing field；仅用于
  字段初始化器的参数不成字段。本 MVP **总是**为每个参数合成私有字段（简单、正确、够用），不做按需优化。
  这是可观察但轻微的差异（未被后续使用的参数仍占一个私有字段），文档记录。
- **`with` / `Deconstruct` / init-only** —— C# record 特性，均 Deferred。

## 已裁决（User gate 确认）

- **无 `[Record]` 的 `class/struct Foo(params)` = 纯主构造器（选项 A）**。User 在 6.5 gate 确认。
  实现路径：desugar 成 **private 字段 + ctor**，参数在类体内经既有「字段入 scope + 裸引用发 `this.X`」
  机制可用（见 design Decision 3）——纯 desugar，无 binder 改动，比预估便宜得多。
- ⚠️ **待实现时验证的边界**：参数被**另一个字段的初始化器**引用时（`class C(int s){ int x = s*2; }`）的
  求值顺序——合成 ctor 必须在跑字段初始化器前完成 `this.s = s`（或让初始化器里的裸 `s` 解析到 ctor
  参数局部而非字段）。common case（参数只在方法体用）无此问题。见 design Implementation Notes。
