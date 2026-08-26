# Design: `[Record]` attribute 替代 `record` 关键字（等价替换）

## Architecture

```
源码  [Record] class/struct T(int X, int Y) { ...块成员... }
  │
  │  (Parser.z42:316) 先解析 attrs=[Record]，作参传进 →
  │  DeclParser._parseTypeDecl(mods, kind, attrs)   ← 就地展开
  │  ┌────────────────────────────────────────────────────────────┐
  │  │ 解析 (params)；_attrsHaveRecord(attrs) 判 [Record]：          │
  │  │   1. 位置参数 → FieldDecl（[Record]=public / 否则=private）   │
  │  │   2. 合成主构造器 MethodDecl（this.X=X …），置块成员之前       │
  │  │   3. `;` 短形式（无块体）亦可                                  │
  │  └────────────────────────────────────────────────────────────┘
  ▼
AttributedDecl([Record], ClassDecl{ Kind="class"/"struct", Members=[X,Y,ctor,...块成员] })
  │  ← ClassDecl 无新字段（PrimaryParams/IsRecord 均不加——自举约束，见 Decision 2）
  ▼  SymbolCollector → TypeChecker → IrGen → ClassDescBuilder → zbc/反射
     · 走 Kind 对应机制（class→Std.Object 基；struct→无基）——record 不再是独立 Kind
     · IrGen 从原始 AttributedDecl.Attrs 判 [Record]（HandlerRegistry.HasRecord），传
       _classDesc(c, hasRecord) → bit3（Kind=="record" || hasRecord）→ 运行时 __type_is_record 不变
```

**与今天 `record` 的等价性**：`[Record] class T(...)` 经就地展开后，产出的 `ClassDecl` 与今天
`_parseRecord` 产出的 `ClassDecl(Kind="record", 同样的 public 字段 + ctor)` **成员逐一对应**，zbc TYPE 段
flags 也同样置 bit3。差别仅：底层 Kind 从 `"record"` 变成 `"class"`/`"struct"`（→ 基类随 Kind 归位，
见 Decision 4）。运行时行为、反射 `__type_is_record`、zbc/zpkg 格式**全不变**。

## Decisions

### Decision 1: `[Record]` 走 directive 路，不序列化 store-meta

**决定**：`[Record]` 是编译器内建 **directive**（保留名），仿 `[Deprecated]`——`HandlerRegistry.KindOf`
判 `Directive`、豁免 D8 `Attribute` 后缀、不合成 store-meta 反射工厂函数。识别读法照抄
`HasDeprecated(rawMem)`：从原始 `AttributedDecl.Attrs` 按 `Attr.Name=="Record"` 判定。

**为什么不 store-meta**：z42 record 无任何需要持久化到反射 blob 的用户数据；它的全部语义就是「bit3 一位」。
bit3 已在 zbc 格式里，复用它 = **零格式-bump**。若走 store-meta 反而要 bump zpkg 加 attr blob，纯浪费。

> **无需 `RecordAttribute.z42`**（实测修正）：directive 靠**名字**识别、**无 backing 类**——stdlib 里
> 并不存在 `Deprecated`/`Suppress`/`Native` 类，`AttributeSynth` 只为 store-meta attr 合成工厂
> （`KindOf==StoreMeta`），directive 走不到那条路径。`[Record]` 同理——parser 按名字 `_attrsHaveRecord`
> 判位置参数可见性、`IrGen` 按名字 `HandlerRegistry.HasRecord` 判 bit3，均无需 backing 类，省一个文件。

### Decision 2: 位置参数在 parser 就地展开（把 attrs 传进 `_parseTypeDecl`）—— 自举硬约束驱动

**问题**：`[Record] class Foo(int X)` 里，「位置参数是否建 public 字段」取决于有没有 `[Record]`。
[Parser.z42:316-354](../../../../src/compiler/z42c.syntax/src/Parser.z42) 先解析 `attrs`（:316）、再调
`_parseTypeDecl`（:336）产 `ClassDecl`、最后（:354）才用 `attrs` 包 `AttributedDecl`。

**曾选 B（AST 期 pass），CI 上证伪**：最初把 `(params)` 存进新字段 `ClassDecl.PrimaryParams`、新 AST pass
`RecordExpand`（semantics）按 `[Record]` 展开 + 置 `ClassDecl.IsRecord` 供 `ClassDescBuilder` 读 bit3。
**PR #295 首推 CI 炸 `E0401: no field IsRecord on ClassDecl`**——根因：**z42c 自建时 `z42c.semantics` 对着
「上一 nightly 的 `z42c.syntax`」编译**（种子 syntax，非 fresh），给 `ClassDecl` 加新字段并从 semantics 读
就撞种子 syntax 无该字段。CI 证据决定性：当前种子有 PR7 的 `TextEdit` 但无我的 `IsRecord` → CI 只炸
`IsRecord`、不炸 `TextEdit`。这与 PR7「预种 syntax 契约」的两-nightly 约束同源（[[bootstrap-seed]] 轴②）。

**改选 A（parser 就地展开）**：把已解析的 `attrs` 作参传进 `_parseTypeDecl(mods, kind, attrs)`（Parser 顶层 +
MemberParser 嵌套两处调用点都传），parser 就能判 `[Record]`（`_attrsHaveRecord`，名字匹配）→ 位置参数
就地展开成字段（`[Record]`=public / 否则=private）+ 主构造器（照搬 `_parseRecord` 逻辑）。

**关键收益——彻底不给 semantics 引入新 syntax 字段**：删 `ClassDecl.PrimaryParams`/`IsRecord`、删
`RecordExpand` pass；bit3 由 `IrGen` 从**原始 `AttributedDecl.Attrs`**（既有结构）`HandlerRegistry.HasRecord(cu.Decls[i])`
判、传 `_classDesc(c, hasRecord)`。**semantics 读零新 syntax 字段** → z42c 自建对**旧种子 syntax 兼容** →
单-PR 落地、无需两-nightly。判据：`grep 'PrimaryParams|\.IsRecord' src/compiler/z42c.semantics` 必为空。

> **教训**：给 z42c 内部 syntax 类型加字段并跨包（semantics）读，受「semantics 对种子 syntax 编」约束——
> 要么两-nightly 预种，要么（本 change 的选择）把逻辑收进 syntax 内、只让 semantics 读既有结构。

### Decision 3: 无 `[Record]` 的 `class/struct Foo(params)` = 纯主构造器（选项 A，gate 已确认）

**问题**：`_parseTypeDecl` 接受 `(params)` 后，`class Foo(int X)`（无 `[Record]`）语法上合法了，语义是什么？

**决定**：**(A) 纯主构造器**（C# 12），User 在 6.5 gate 确认。

**关键发现——(A) 是纯 desugar，无需 binder 改动**（初判「大特性」被证伪）：
- [DeclBinder.z42:48-54](../../../../src/compiler/z42c.semantics/src/DeclBinder.z42) 在绑定方法体时把类的
  **全部字段**（含继承）`env.Define(name, type)` 播种进 `TypeEnv`——所以字段名在方法体内是可裸引用的 var。
- [ExprTyper.z42:91](../../../../src/compiler/z42c.semantics/src/ExprTyper.z42) `_bindIdent` 经 `env.LookupVar`
  命中该字段名；[AccessEmitter.z42:420-450](../../../../src/compiler/z42c.semantics/src/AccessEmitter.z42) 把
  裸 `BoundIdent`（在 `_ctx.Fields` 中）发成 `FieldGetInstr(reg0=this, name)` = `this.X` 字段读。
- ∴ 把 primary-ctor 参数 desugar 成**私有字段（同名）+ ctor**，类体里裸 `X` 自然解析并发成私有字段读——
  `class Point(int X, int Y) { int Sum() { return X + Y; } }` 直接可用，**无需改名字解析**。z42c 惯用
  `this._X` 只是风格，语言本身支持裸字段访问。

**与 `[Record]` 的唯一差别**：字段**可见性**（private vs public）+ **bit3**（不打 vs 打）。两条分支共用
同一段展开代码（`_parseRecord` 的字段+ctor 合成），传一个 `visibility`/`isRecord` 参数区分。

**MVP 简化（记录，非缺陷）**：C# 12 有「capture 优化」——参数只在被构造后使用时才成 backing field。本 MVP
**总是**为每个参数合成私有字段。差异轻微（未后续使用的参数仍占私有字段），文档标注，不做按需优化。

**待实现验证的边界**：参数被**另一字段的初始化器**引用（`class C(int s){ int x = s*2; }`）的求值顺序——
见 Implementation Notes「字段初始化器 × primary 参数」。common case（参数只在方法体用）无此问题。

### Decision 4: 基类语义随底层 Kind 自然归位（无特例）

**决定**：删 record Kind 后，基类判定纯看 Kind——`ClassDescBuilder:110` 的 `isStructOrRecord` 去掉
record 分支，变回 `c.Kind=="struct"`：

- `[Record] class T` → Kind=`"class"` → 拿到默认 `Std.Object` 基类（且本就被
  `ExportedTypeExtractor` 注入 Object 四方法，行为一致）。
- `[Record] struct T` → Kind=`"struct"` → 无默认基类（同普通 struct）。

今天裸 `record`「无 Std.Object 基」是历史怪癖；改后 `[Record] class` 拿回基类是**可观察语义变化**，
User 已接受。**不**给 `[Record]` 加「抑制基类」特例——attribute 只管 bit3 + 位置参数糖，不碰身份/基类轴。

### Decision 5: bit3 触发源迁移 = 零格式-bump

**决定**：`ClassDescBuilder._classDesc(c, hasRecord)` 的 bit3 = `if (c.Kind=="record" || hasRecord) flags+=8`。
`hasRecord` 由 `IrGen` 从原始 `AttributedDecl.Attrs` 判（`HandlerRegistry.HasRecord(cu.Decls[i])`）后传入——
**不新增 ClassDecl 字段**（自举约束，见 Decision 2）。**扩展到 struct**：`[Record] struct` 也能置 bit3
（bit2 struct + bit3 record 共存）——今天 record 无法是 struct，这是新增能力，但格式位早已存在。

**格式影响**：类形状 flags 字节不变、bit3 语义不变（仍是 `is-record`）、reader（`__type_is_record`
[reflection.rs:1542](../../../../src/runtime/src/corelib/reflection.rs)）一行不改。**非格式-bump**——
warm 本地可验，无两代自举墙。

## Implementation Notes

- **不加 ClassDecl 字段**：自举约束（Decision 2）——位置参数在 `_parseTypeDecl` 内就地消费展开，无需存字段。
- **parser 就地展开逻辑**：`_parseTypeDecl(mods, kind, attrs)` 内直接搬 `_parseRecord`:283-307 的两段——
  ① 位置参数 → `FieldDecl(vis, ...)`（`vis` = `_attrsHaveRecord(attrs)` 时 `"public"`，否则 `"private"`）；
  ② 合成 ctor（`this.X=X` 赋值 BlockStmt + `MethodDecl`）。置于块成员前部，再接块成员（或 `;` 短形式）。
  `_attrsHaveRecord`（DeclParser 内，名字匹配 `"Record"`）不依赖 semantics 的 HandlerRegistry（syntax 自足）。
- **字段初始化器 × primary 参数**：ctor 内**参数是局部**（`_lookupIdent` 局部优先于字段，:419>:420），故
  `this.X=X` 里右侧 `X`=参数、左侧 `this.X`=字段，正确。若某字段初始化器（`int x = X*2;`）引用 primary
  参数 `X`：需确认 z42 字段初始化器在合成 ctor 中的注入时机——若初始化器在 ctor 体内、且 primary 参数
  作为 ctor 局部在整个 ctor 体可见，则裸 `X` 解析到参数局部（正确）。实现时以一条 golden 验证此序；
  若序不对，退路 = 合成 ctor 把 `this.X=X` 排在字段初始化器之前（本就该如此）。
- **`[Record]` 识别**：`HandlerRegistry.HasRecord(rawMem)` / `IsRecordDirective(name)`，逐字节仿
  `HasDeprecated` / `IsDeprecatedDirective`。`IsDirectiveAttr` 加 `|| IsRecordDirective(name)`；
  D8 后缀豁免集加 `Record`。
- **`Kind=="record"` 站点清理**（~22 处，删关键字阶段）：绝大多数是 `Kind=="class"||"struct"||"record"`
  → 删 `||"record"`（record 现是 class/struct，已被覆盖）。逐站点见 tasks 3.x。
- **自举纪律（[[bootstrap-seed]]）**：`[Record]` 是**新语法位置**（class/struct 后跟 `(params)`）+ 新内建
  directive——**support 在 nightly N 落地，z42c/stdlib 源码本 change 不使用 `[Record]`**（受限写法延续），
  故上一 nightly 的 z42c 能编当前源、self-host 字节不变。**use（迁移 stdlib/examples）在 nightly N+1**。
  改完编译器跑 `xtask test bootstrap` 边界检查。

## Testing Strategy

- **parser 就地展开**：`[Record] class C(int X)` → Members 含 **public** 字段 `X` + ctor；`class C(int X)`
  （无 attr）→ **private** 字段 `X` + ctor；`[Record] class C { ... }`（无 params）也合法；`;` 短形式合法。
- **等价 golden**：新 `[Record] class` 端到端产出与迁移前 `record` 版**行为一致**（`__type_is_record`
  反射为 true；字段/ctor 可用）。`[Record] struct` 的 bit2+bit3 反射。
- **GREEN gate**：完整 `xtask test`（e2e / cross-zpkg / stdlib / **compiler 自举 5/5 byte-identical** /
  vscode-syntax）。self-host 字节不变是核心（z42c 源不用 `[Record]`）。**格式中立** → warm 本地可验。
- **nightly N+1**：删关键字后 `record X` 应报 "expected declaration"；stdlib/examples 迁移后全绿。

## Deferred / Future Work

- **add-record-value-semantics**：`[Record]` 生成 `Equals`/`GetHashCode`/`ToString`/`==` 值成员
  （C# record 真语义）。本 change 只做等价替换，值语义独立成 change。
- **primary constructor capture 优化**：C# 12 的「参数仅在被构造后使用时才成 backing field」优化。本 change
  的 primary ctor（Decision 3=A）总是合成私有字段，不做按需优化——功能完整，仅省一个内存优化。
- **`with` / `Deconstruct` / init-only**：C# record 特性，各自独立。
