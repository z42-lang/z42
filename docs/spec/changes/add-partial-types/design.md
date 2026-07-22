# Design: partial 类型——编译期合并、零格式改动、增量共存

## Architecture

```
源文件（同包同 ns，各带 partial class Foo）
  a.z42: partial class Foo : Base(x) { field X; method M1() {...} }
  b.z42: partial class Foo { field Y; partial method M2(); method M1'... }
        │
        ▼  Parser：每碎片 → 一个 ClassDecl(IsPartial=true, Name="Foo")
        │
   ┌────▼─────────────────────────────────────────────┐
   │ SymbolCollector.CollectWithImports（跨全部 CU）    │
   │  _passClassStubs：同名 partial → 指向同一 Z42ClassType│
   │  _passMembers   ：各碎片成员并入同一 Fields/Methods  │
   │                   （碎片按 relPath Ordinal 序拼接）  │
   │  合并校验：基类/主ctor 单碎片；接口并集；重复成员报错 │
   └────┬─────────────────────────────────────────────┘
        │  合并后的完整 Z42ClassType（全字段+全方法，有序）
        ▼
   IrGen（逐文件产 IrModule）
     主碎片   → 发 1 条完整 TYPE record（成员取自 Z42ClassType）+ 本文件方法体
     非主碎片 → 不发该类型 TYPE record，只发本文件方法体
        │
        ▼  zbc TYPE record 与非 partial 完全同构 → VM/loader/格式零改动
```

## Decisions

### Decision 1: 编译期合并 vs 加载期合并 → 编译期合并（方案 A）
**问题：** partial 类型跨文件，完整 TYPE record 在哪里生成？
**选项：**
- A（编译期合并）：前端合成单条完整 record。**零 zbc/zpkg 格式改动、零 Rust loader 改动、
  无格式版本 bump**。代价：改一碎片联动重编同类型兄弟碎片。
- B（加载期合并）：每碎片发 partial 片段，VM 合并。增量粒度更细（单碎片独立），但要改
  zbc/zpkg 格式 + Rust loader 合并逻辑 + strict-pin 版本 bump，且**仍需**编译期符号合并
  （碎片间引用成员才能类型检查），复杂度净增。
**决定：** 选 **A**。B 用 VM 复杂度 + 格式 bump 换边际增量收益，违背"最简实现"。User 已确认。

### Decision 2: 合并顺序必须确定性排序（硬约束）
**问题：** 合并后字段布局顺序 = 对象内存偏移 = zbc 字节；顺序漂移 → 非确定产物。
**决定：** 碎片按**项目相对路径 Ordinal 排序**后再按文件内声明序拼接。**禁止**依赖
`SourceDiscovery` 的文件系统枚举序（见 [common-pitfalls.md 规则 1](../../../.claude/rules/common-pitfalls.md)——
该项目已因非确定加载序出过 CI 红 bug）。合并入口在 `SymbolCollector`，排序键复用增量已有的
`IncrementalBuild.Rel(projectDir, path)`。

### Decision 3: 主碎片选定 —— 路径 Ordinal 最小（单一规则，2026-07-19 修订）
**问题：** 完整 TYPE record（字段布局 / 方法签名表 / vtable / 基类）只能由一个碎片的 IrModule 发**一次**
（VM `merge_modules` 按名 first-wins 去重；多发会丢成员）。发到哪个碎片？
**决定：** **主碎片 = 项目相对路径 Ordinal（逐字节）最小的碎片。单一规则，无例外。**
- **砍掉**早期"基类/主构造器碎片优先"的特例——合并后的 record 内容**取自合并 `Z42ClassType`，与发自哪个
  碎片无关**（基类即使声明在非主碎片，合并 record 里照样有它），故无语义理由偏好基类碎片；min-path 单条
  即确定、零特例、零额外元数据。
- **非 partial 类型**：只有一个声明文件 → 天然就是它，无需选。
- `IrGen._classDesc` 判断"本 CU 是否主碎片"：查合并 `Z42ClassType` 记录的主碎片文件（min-path）== 当前 CU。
- **排序键**：项目相对路径、Ordinal 比较，**禁止依赖 SourceDiscovery 文件系统枚举序**
  （[common-pitfalls 规则 1](../../../.claude/rules/common-pitfalls.md)）。

> **为什么 record 要"选一个"、方法体不用**：partial 是"一条合并 record vs N 个碎片"——record 是**必须去重
> 合并的单一实体**；方法体是 N 个**不同名**的独立全局函数（`Foo.M1`/`Foo.M2`），`merge_modules` 按 FQ 名
> 扁平化时既不冲突也不丢，各留各碎片即可，无需选。这也是 D8"方法体默认散留各碎片"的前提。

### Decision 4: 合并语义与冲突规则（吸 C# 优点 / 规避缺点）
- **每碎片必写 `partial`**：任一同名声明缺 `partial` → 编译错误（C# 同款防误拆好规则）。
- **同包 + 同 namespace**：跨包/跨 ns 同名不合并（维持现有 local-wins 跨包语义）。跨包扩展走 `impl`。
- **基类 / 主构造器**：至多一个碎片声明；多碎片声明冲突 → 报错。
- **接口列表**：各碎片并集，按名 dedup。
- **重复成员**：两碎片声明同名字段 / 同签名方法（非 partial method 声明↔实现配对）→ 报错。
- **Kind 一致**：所有碎片 `Kind`（class/struct/record/interface）必须相同，否则报错。

### Decision 5: partial method —— 做，但只采 C# 9+ 干净形态（2026-07-19 User 确认）
**问题：** partial method 是 C# 里唯一被点名规避的缺点（旧版 void-only + C#9 打补丁的双规则）。
**决定（已确认，封版）：** **做**，但**只实现 C# 9+ 统一形态**，丢弃旧版烂规则：
- 声明侧 `partial R M(params)` 无 body；实现侧 `partial R M(params) { ... }` 有 body。
- **允许任意返回类型、访问修饰符、`out`/`ref` 参数**。
- **无实现时整体擦除**：不发方法桩，`IrGen` 消解对它的调用点（等价方法不存在）。
- 声明↔实现签名必须完全一致（含返回类型、参数、修饰符），否则报错；至多一个实现。
> 2026-07-19 User 确认「做」——本 Decision 封版，实施照此。

### Decision 6: 增量失效——partial 碎片组联动
**问题：** 改一个碎片，如何保证同类型其它碎片一起重编、合并 record 保持一致？
**决定：** `IncrementalBuild` 收集"定义名 → 碎片文件"多所有者链表时，对 `IsPartial` 类型名把
**其全部碎片文件互连成团**（clique）：任一碎片 fresh → 整组 fresh。这保证：
- 合并布局永远由"整组同时重编"产生，cache 一致，无"半更新"。
- **非 partial 文件零影响**——"源没变即跳过"对绝大多数文件完全保留。
- 显式互连，不依赖碎片间是否恰好 token 重叠（比"碰巧连边"更稳）。
对账器新增 partial 腿：touch 任一碎片，增量 dist == 全量 dist（逐字节）。

### Decision 7（D-boot）: 自举能力版本号——本 change 暂不引入
**问题：** bootstrap-seed.md 设想 z42c 带"语法能力版本号"，实际未实现。
**决定：** 本 change **不引入**该数字（避免范围蔓延）；分阶段纪律靠 `xtask test bootstrap`
（下载上一版 nightly 编当前源）兜底：阶段 1 落 support 后，z42c/stdlib 源**不使用** partial，
bootstrap 检查即绿。引入能力版本号另立独立 change。→ 关联 Open Question 已在 proposal 标注。

### Decision 8: indexed 发布态 —— 方法体散留各碎片 zbc（默认），不强制合并（2026-07-19）
**问题：** indexed 布局下，partial 类型的方法体是"散在各碎片 zbc"还是"合并进主碎片一个 zbc"？
**背景（实证 VM）：** `load_zpkg_indexed`（[loader.rs](../../../src/runtime/src/metadata/loader.rs)）把全部散装
zbc 读进后走 `merge_modules`（[merge.rs](../../../src/runtime/src/metadata/merge.rs)）——**函数按 FQ 名
全局扁平化、类按名 dedup、派发按名**（`func_index`）。故 partial 类型"散在多 zbc"与"塞进一个 zbc"，
**加载合并后是逐字节相同的 Module**，散/合对 VM **完全不可见**。
**决定：** **默认散**——各碎片方法体留各自 zbc，dist zbc = cache 条目字节原样拷贝（零重序列化，保住
`add-file-level-incremental` 的字节稳定）。record 仍只由主碎片（D3）发一份。
- **零 VM/格式改动**：散着发不需要加载期合并（方案 B）、也不需要 per-method 跨 zbc 引用——Plan A 的
  "零改动"前提在散模式下天然成立（架构图顶部"非主碎片只发本文件方法体"即此形态）。
- **"一类型一 zbc"是可选磁盘优化，非正确性要求**：若未来要 per-type lazy-load / 按类型符号化，可让主碎片
  zbc 收全类型方法体（代价：该 zbc 丢字节拷贝、随任一碎片变而重写）。**v1 不做**。

### Decision 9: 嵌套类 —— 扁平限定名 record，按声明文件落位；v1 只做顶层 partial（2026-07-19）
**背景：** 嵌套类在 zbc 是**扁平独立 record + 限定名**（`Outer.Inner`；`ClassDesc` 无 nesting 字段、反射
`GetNestedTypes` 仍 Deferred），**非**物理嵌在外层 record 里。故它与顶层类走**同一条放置规则**。
**决定：**
- **放置**：任何类型（顶层或嵌套）落在"声明它的源文件"的 zbc。D3 的 min-path **只管 partial 外层自身
  record 的落点，不牵动其嵌套类型**——嵌套类型各按自己声明碎片放。
- **(a) partial 外层含嵌套（嵌套只在某碎片声明）**：**允许**。`Outer` 合并 record → 主碎片 zbc；
  `Inner` record → `Inner` 声明碎片的 zbc；按限定名在加载期各归各位。
- **(b) 嵌套类自身 partial（`Outer.Inner` 跨碎片拆）**：**v1 报错 + Deferred**。理由**不是"有害"**——机制
  与顶层同构（同 min-path、同 clique，换个更长的名），而是**嵌套发射/反射链路本身尚未接通**
  （SymbolCollector 未见显式嵌套收集）。接通"嵌套发为扁平 record"后，partial-nested 递归适用同规则即可解禁。
  报错文案要精确：「嵌套类型 `X` 暂不支持 partial（v1）」，**勿写成语义禁令**。
- **v1 scope**：partial **只做顶层类型**；嵌套类发射 + partial-nested 作为独立后续（嵌套支持独立地不成熟，
  不绑上 partial 一起做）。partial 设计只需**不与未来嵌套模型冲突**。

## Implementation Notes

- **`IrGen._classDesc(ClassDecl)` 改造是核心**（[IrGen.z42:713](../../../src/compiler/z42c.semantics/src/IrGen.z42#L713)）：
  当前逐 `ClassDecl` 读 AST 本地成员生成 `IrClassDesc`。partial 下须改为：成员序取自合并后的
  `Z42ClassType`（SymbolCollector 已按稳定序合并），且仅主碎片 CU 产出该 record。
- **`SymbolCollector` 合并点**在 `CollectWithImports` 的 CU 循环（stub 全跑完再跑 members，
  已保证跨 CU 声明序无关）——partial 合并天然嵌入，无需新增 pass。
- **`ExportedTypeExtractor`** 按合并后的 `Z42ClassType` 导出一次 TSIG，跨包消费方
  （`ImportedSymbolLoader`）零改动——它们只看到一个完整类型。
- **partial method 擦除**：实现缺省时，`IrGen` 既不发签名也不发 body；调用点在 TypeChecker
  绑定阶段解析为"目标不存在"→ 若被调用即编译错误（与 C# 一致：擦除的 partial method 不可被
  依赖返回值/out 的调用点使用；void 无参无 out 的擦除调用可静默消解）。

### Phase-1 实现约束（2026-07-21 源码勘察）：`partial` 必须作**上下文关键字**，不新增 token

> 勘察 `z42c.syntax` 得出的硬约束——写实现前必须遵守，否则触发大范围不可控改动。

- **不新增 `TokenKind.Partial`**：token 编号 [`TokenKind.z42`](../../../src/compiler/z42c.syntax/src/TokenKind.z42) 是**稠密连续**的——
  word-keyword 占 `9..93`、符号从 `LParen=94` 起紧接。中间插一个 `Partial` 会**顺移 94 起的所有符号编号**，
  连带 zbc token 序列化、`Lexer._isWordKeyword` 的**硬编码区间 `9..93`**、及一切按区间判"是否关键字"的逻辑全部漂移
  → 大范围、易错、且**跨 nightly 的格式/能力边界**（违背分阶段引入纪律）。
- **改为上下文关键字**（同 C# `partial` 语义）：`partial` 词法上仍是 `Identifier`，仅在**修饰符位置**被识别为修饰符。
  集成点是 [`DeclParser._parseModifiers()`](../../../src/compiler/z42c.syntax/src/DeclParser.z42)——现循环条件
  `while (_isModifier(_peek().Kind))` 扩为 `|| _isContextualPartial()`，命中即把 `.Text`（"partial"）并入 mods 串。
- **消歧用 `_peekAt(int)`**（Parser 已有任意前瞻）：`_isContextualPartial()` = `_peek()` 是 `Identifier "partial"`
  **且** `_peekAt(1)` 为 `class/struct/record/interface`（partial 类型）或后续构成方法头（partial method）——
  避免把名为 `partial` 的字段/变量误当修饰符。
- **下游零特殊化**：`IsPartial` 由 `_modsHas(mods, "partial")` 派生（同 `event`/`sealed` 现有 string-mods 路数），
  `ClassDecl`/`MethodDecl` 加 `IsPartial` 位即可。**不动 `_isModifier`**（它是 kind-based，Identifier 不进）。
- **好处**：零 token 新增、零编号顺移、零 `_isWordKeyword` 改动、零格式边界——纯前端、当前 nightly 可编，符合
  support-先行纪律。

## Testing Strategy

- **解析单测**（`z42c.syntax/tests/parser/partial/`）：partial class/struct/record/interface
  解析；partial method 声明/实现；缺 `partial` / Kind 不一致 → 报错。
- **合并单测**（`z42c.semantics/tests/collect/partial_merge/`）：两碎片字段/方法合并；
  重复成员报错；基类双声明报错；**合并顺序确定性**（交换文件发现序，产物字节不变）。
- **端到端 golden**（`src/tests/partial-types/`）：partial 跨文件 build + run，验证合并类型
  实例化 / 方法调用 / partial method 有实现 vs 擦除。
- **增量对账**（`src/tests/partial-types/incremental/`）：touch 单碎片，增量 dist == 全量 dist
  逐字节；非 partial 文件未被重写（mtime 断言）。
- **GREEN gate**：`xtask test`（e2e + cross-zpkg + stdlib + compiler 自举 7/7 byte-identical）。
- **自举边界**：`xtask test bootstrap`——上一版 nightly 仍能编当前源（阶段 1 support-only 期）。

## Deferred / Future Work（实施期 2026-07-22 识别）

### partial-future-cross-fragment-overload-mangle
- **来源**：实施期（Phase 2/3）。
- **触发原因**：方法键 mangle 的重载预扫描（`SymbolCollector._fillClass`）是**逐碎片**的
  （只扫当前碎片成员）。同名方法的多个重载**分处不同碎片**时，各碎片按各自 sibling 集算键 →
  键可能不一致 → 派发/配对错位。partial method 与同名普通方法同碎片时也受此影响。
- **当前形态**：碎片间**不共享方法名**的用法（覆盖全部 spec 场景 + 主用例平台拆分）完全正确。
- **前置依赖**：把 partial 类型的 mangle 预扫描改为**合并集**（先聚合全碎片成员再算键）。
- **触发条件**：出现「重载方法跨碎片拆分」的真实需求。
- **当前 workaround**：把同名重载放同一碎片。

### partial-future-interface-method-dup + property/indexer-dup
- **来源**：实施期。
- **触发原因**：跨碎片重复成员检测覆盖字段 + 普通方法；**接口方法** / **属性·索引器访问器**的
  跨碎片重复未检测（静默 last-wins）。
- **前置依赖**：在 `_fillInterface` / property·indexer 填充处加 `ct.IsPartial` gated dup 检查。
- **触发条件**：需要严格拒绝这类重复时。

### partial-future-incremental-reconcile-test
- **来源**：实施期（Phase 6）。
- **触发原因**：`src/tests/partial-types/incremental/` 逐字节对账夹具未随本 change 落地——需
  `xtask test incremental` 对账器的语料接入（driver-path，非 CI-GREEN 关键路径）。
- **前置依赖**：了解 `test incremental` 对账器夹具组织。
- **当前形态**：Phase 4 clique 代码已实现机制；merge 单测 + code review 覆盖正确性。

## Design 精化（实施期，2026-07-22）

- **D2 精化**：合并顺序确定性由 `SourceDiscovery` 的 **Ordinal 排序**天然达成（`srcs`/`files`
  数组已 Ordinal 序），无需在合并处额外排序；原设计设想复用 `IncrementalBuild.Rel` 排序键，
  实测发现 SourceDiscovery 已排 → 直接依赖之。
- **Phase 5（ExportedTypeExtractor）无需改动**：`drop-tsig-expt` 已删 EXPT+TSIG 段，跨包解析
  改读 TYPE/SIGS（`TsigReconcile.Rebuild`）。依赖 zpkg 打包全碎片 zbc → 消费方自动重建完整合并
  类型（主碎片 TYPE record + 各碎片 SIGS）。原 Scope 列 ExportedTypeExtractor 系 drop-tsig-expt 前
  的设想。
- **同文件多碎片去重（emittedPartial）**：`PartialMainFile` 是文件级；同一文件内多个碎片都
  `ptIsMain` → IrGen 加 CU-局部 `emittedPartial` 集，合并 TYPE record 每类型只发一次。
- **合并机制实际挂载点**：`SymbolCollector.CollectAll`（设计文写作 `CollectWithImports`，实际
  多-CU 入口是 `CollectAll`）。
