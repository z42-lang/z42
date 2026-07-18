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

### Decision 5: partial method —— 做，但只采 C# 9+ 干净形态
**问题：** partial method 是 C# 里唯一被点名规避的缺点（旧版 void-only + C#9 打补丁的双规则）。
**决定：** **做**，但**只实现 C# 9+ 统一形态**，丢弃旧版烂规则：
- 声明侧 `partial R M(params)` 无 body；实现侧 `partial R M(params) { ... }` 有 body。
- **允许任意返回类型、访问修饰符、`out`/`ref` 参数**。
- **无实现时整体擦除**：不发方法桩，`IrGen` 消解对它的调用点（等价方法不存在）。
- 声明↔实现签名必须完全一致（含返回类型、参数、修饰符），否则报错；至多一个实现。
> 若 User 实施前改主意不要 partial method：删本 Decision，`partial` 仅作类型修饰符，工作量更小。

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
