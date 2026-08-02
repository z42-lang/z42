# Design: 按符号惰性 reconcile（T2）——借鉴 .NET/rustc/javac

> 配套 proposal.md。硬约束：**stdlib zpkg 保持 packed 单文件、最小化**（不改 indexed/散装 zbc）。

## 1. 借鉴的成熟模式

| 语言 | 单文件 + 内部索引 | Completer 惰性完成 |
|------|------------------|-------------------|
| .NET (Roslyn) | metadata 表按 token 随机访问 | `PENamedTypeSymbol` 首次 `GetMembers()` 才解码，缓存 |
| Rust (rustc) | `rmeta`：`LazyValue`/`LazyArray` 偏移 + `DefId→offset` 表 | `CStore` 按 DefId 按需解码 |
| Java (javac) | 每类一 `.class`（天然分单元） | **`ClassSymbol.completer`** 首次访问成员才读 |

**通用三件套**：① 符号先是**桩**（名字+引用），成员/基类/方法首次访问才填（completer），缓存；
② 基类链传递惰性（解析类→给基类建桩→按需完成）；③ 重载按 (名字,签名) 索引，非短名 first-wins。

**映射到 z42**：② 已有（`LazyReconWorld.EnsureFq`）；① 是本 change 的 `SymbolTable` completer；
内部索引复用 packed zpkg 的 **MODS 目录**（按命名空间），必要时加紧凑 per-type 偏移表。

## 2. z42 packed zpkg 的内部索引（已存在，零格式改动可用）

`ReadModuleTypes`（`ZpkgReader.z42:281`，packed 分支）已证：MODS 目录每模块 = 头（ns/src/hash/fnCount/
firstSig）+ **5 个长度前缀体**（func/TYPE/dbug/regt/tidx）。当前**顺序读全部模块**。**关键**：体是长度前缀
→ 可 `m.Pos += len` 跳过非目标模块，只对目标模块 `ZbcReader.ReadTypeAt`。→ **按命名空间 seek，零格式改动**。

## 3. Phase 1：按命名空间读（z42.ir，零格式改动，低风险）

### 3.1 新读取入口
```
// packed：遍历 MODS，长度跳过非 targetNs 模块，只解 targetNs 的 TYPE 体。indexed：复用 _readIndexedTypes 的按 FILE 读（已是按命名空间）。
public static ZpkgModuleTypes ReadOneModuleTypes(ZpkgInfo z, string pkgDir, string targetNs)
public static ZpkgModuleSigs  ReadOneModuleSigs(ZpkgInfo z, string targetNs)   // firstSig/fnCount 已是按模块范围
```
- 找不到 targetNs → 返回空（调用方按「该包不声明该 ns」处理）。
- **与全量一致性**：`ReadOneModuleTypes(z, ns)` 的产出 == `ReadModuleTypes(z)` 中 ns 那条（同一 `ReadTypeAt`
  解码，只是跳过别的模块）→ 天然 byte-identical。

### 3.2 LazyReconWorld 粒度：per-package → per-(package, namespace)
现状 `EnsureIdx(i)` 填 `Wp[i]` = **整包全部模块** TYPE/SIGS。改为**按命名空间懒填**：
```
// Wp[i] 从「整包解析结果」变为「已填命名空间 → ReconWorldMod」的 map（每命名空间一份 TYPE/SIGS）。
EnsureNs(pkgIdx, ns)：若 (pkgIdx, ns) 未填 → ReadOneModuleTypes/Sigs(World[pkgIdx], ns) 填入。
EnsureFq(fq)：ns=_nsOf(fq) → 对声明 ns 的每个包 EnsureNs(pkgIdx, ns)（原按包 EnsureIdx 改按命名空间）。
```
- **`_rebuildClass`/`_locate`**（已按单类型）：定位 fq 时走 `EnsureNs`（只读该 ns 模块），基类链每祖先 FQ 同理。
- **eager 兼容**：整包路径（ScanDirs / build）继续用 `EnsureAll`/整包 `Rebuild` → Wp 全填 → **byte-identical
  不受影响**（关键：Phase 1 只加「按命名空间」入口，不改整包语义）。

### 3.3 ReconcileOne
```
// 只 reconcile 一个 fq 类型：读其命名空间模块 → 找到 IrClassDesc → _rebuildClass（基类链经 EnsureNs 按需）。
public static ExportedClassZ ReconcileOne(ZpkgInfo z, string zDir, string fq, LazyReconWorld world)
```
产出 == 整包 `Rebuild` 里该类那条（同 `_rebuildClass`）→ 差分可验。

**Phase 1 门禁**：单测 `ReconcileOne(Console) == Rebuild(Std.IO) 里 Console 那条`（逐字段）；只读 Std.IO+Std
模块不读 z42.core 其余模块（计数验证）。整包路径零改 → 自举不动点天然守住。

## 4. Phase 2：SymbolTable completer（compiler 核心，高风险）

### 4.1 桩 + completer（借 javac）
`ImportedSymbolLoader.Load` 现状 eager 展平整个 active 集进 `SymbolTable.Classes`。改为：
- **Load 只建桩**：每 active 命名空间的类名 → **空 `Z42ClassType` 桩**（名字+来源包+ns，成员未填），
  加一个 `_completed:bool` + 到 loader/scan 的引用。**不读成员**。
- **`SymbolTable.GetClass/HasClass/ResolveTypeP` miss 或桩未完成** → completer：`ReconcileOne(fq)` →
  `_fillClass`（`ImportedSymbolLoader.z42:249` 已把一个 ExportedClassZ→Z42ClassType）→ 标 `_completed`。
- 基类桩同理（首次访问基类成员才完成）。

### 4.2 三个「整包」难点的成熟解（借索引，不扫全集）
- **arity-mangle**（`List` vs `List<T>` 短名冲突 → `$N` 键）：现需扫全 active 集看同名。**成熟解**：
  预计算一张**轻量「短名 → 该名下全部 (ns, arity)」索引**（从 B 的 ns 索引 + 各命名空间 TYPE 头一次性
  廉价扫，不解成员）。建桩时即可按 (名, arity) 键，无需完成全部同名类。
- **first-wins 顺序**：桩按 prelude-first + Ordinal 建（同现顺序）→ 完成不改键 → 与全量 first-wins 一致。
- **跨包 impl 合并**（Phase3 `_mergeImpl`）：**成熟解（rustc coherence）**：建一张 **`目标类 FQ → 提供 impl
  的 (包, ns)` 索引**，完成目标类时按需拉取其 impl（不预合并全 active 集）。
- **接口先于类**（`_passClassStubs` 需基名分类 iface/class）：桩阶段即登记 kind（TYPE 头有 class_flags 的
  interface 位，廉价读，不解成员）→ 分类不需完成。

### 4.3 只在 REPL 路径启用（守住自举）
- **build/test 全量编译**：`ImportedSymbolLoader.Load` 走**原 eager 整包**路径 → `SymbolTable` 全填 →
  **自举字节不动点天然 byte-identical**（Phase 2 不碰整包路径）。
- **REPL 路径**：`Script._compileSrcOnce` 传一个标志 → Load 走 completer 惰性路径。两路径产出的
  `SymbolTable`（对同一引用集）语义等价，差分验证。

## 5. Phase 3：接入 REPL

`Script._compileSrc` 的 E0401 回退现状「整包加载 usings」→ 改为「completer 按引用类型完成」：
首次 `Console.WriteLine` → 编译触发 `SymbolTable.GetClass("Console")` miss → completer `ReconcileOne` →
只读 Std.IO+Std → 完成 → 编过。**不再 EnsurePackageLoaded 整包**。

## 6. ④（可选，deferred within T2）：紧凑 per-type 偏移表

若测出「命名空间粒度不够」（prelude `Std` 命名空间本身大，读整个 Std 仍慢）：给 TYPE 段体加**紧凑 per-type
索引**（格式迭代，zbc minor bump）：
```
TYPE 体头部：typeCount:varint + 每类 (fqStrIdx:varint 走 STRS 池, bodyOffset:varint)
```
- 名字走已有 STRS 池（去重、零额外字符串），偏移 varint → z42.core 约 +数百字节，**不膨胀、仍 packed 单文件**。
- `ReadOneType(z, ns, fq)`：seek 到 ns 模块 → 读 type 索引 → seek 到 fq 类体 → 只解一个类。
- **先不做**：①②③ 实测不够、且大头确在「读整个 Std 模块」时才上。

## 7. 正确性门禁（GREEN）

1. **自举字节不动点（gen1==gen2）**——铁门禁。Phase 1/2 的整包路径零改 → 天然守住；这是「REPL-only 惰性
   不污染 build」的硬证据。
2. **差分**：REPL completer 路径产出的每个 `Z42ClassType`（成员/基类/接口/impl）== 整包 Load 产出的对应类
   （逐字段）。开发期临时 harness，绿后撤。
3. **REPL eval 正确性**：表达式/Console/List/Dictionary/Math/Convert/Environment/声明/跨轮 var/重载/泛型/
   跨声明基类链——全对（对齐 B+T1 的用例集 + 补重载/泛型/impl 用例）。
4. **首次符号 eval 延迟实测**：Console/List 从 1.7s → 目标 ~0.7-0.9s（①②③）/ ~0.5s（+④）。
5. `xtask test` 全绿。

## 8. 风险与缓解

- **最高风险**：Phase 2 completer 与整包 Load 产出不一致（arity 键/impl 合并/顺序）→ REPL miscompile。
  缓解：差分 harness（2）逐类对齐 + REPL-only（整包路径不动，自举不动点是独立铁证）+ 分阶段（Phase 1 先
  单独落地验证按命名空间读的一致性，再上 Phase 2）。
- **中风险**：LazyReconWorld 粒度从 per-package 改 per-(package,ns) 触及 lazy-type-world 的 Wp 结构 → 波及
  ScanDirs（整包）。缓解：整包路径继续 `EnsureAll`，按命名空间入口是**新增**、不改 EnsureAll 语义。

## 9. 分阶段交付建议

- **PR-1（Phase 1，低风险）**：`ReadOneModuleTypes/Sigs` + `LazyReconWorld.EnsureNs` + `ReconcileOne` +
  差分单测。整包路径零改、自举不动点守住。**可独立合并**（为 Phase 2 铺路，且本身让 REPL 的 EnsureFq 只读
  引用命名空间——已有部分收益）。
- **PR-2（Phase 2+3，高风险）**：`SymbolTable` completer + 三难点索引 + REPL 接入 + 差分/延迟门禁。
- **PR-3（④，可选）**：紧凑 per-type 索引（若需）。

## 10. 已评估并否决的 Phase 2 捷径（2026-08-01，避免下轮重走）

试图绕开「SymbolTable completer」核心改动的两条捷径都不成立：

- **①（否决）解析 E0401 错误文本提未解析类型名 → 只 ReconcileOne 那一个 + 重试**：诊断措辞多样
  （`unknown type in \`new\`: X` / 成员访问失败 / `unknown type parameter` …，散落 ExprTyper/
  ConstraintChecker 等多处），无单一干净格式可靠提取类型名。即便加整包回退保正确性，也基本退化成整包
  加载，无净收益。
- **②（否决）逐个 using 增量加载 + 每次重编**：为用到第 N 个 using 的求值需编译 N 次（每次 ~0.2-0.4s），
  比「一次全载 + 重编一次」更慢；且 `Console`→`Object` 基类链仍整包拉 z42.core，省不掉大头。

**结论**：真正的 per-type 只能走 **③ SymbolTable completer**（编译器自身在类型解析点 miss → 回调
`ReconcileOne` 精确补一个类型）。这是名字解析核心改动，须专门一轮做：miss 回调 + arity/impl/顺序按类型解
+ REPL-only 双路径 + 逐类差分 + 自举不动点。**Phase 1 基础设施（`ReconcileOne` 等）已就位、编译通过。**

## 11. 实测尝试记录 + worktree 环境态告警（2026-08-01/02）

试过**中层** per-ns：`LazyReconWorld.EnsureFq` 从整包 `EnsureIdx` 改为按命名空间 `EnsureNs`（只读引用的 ns
模块）。跑自举字节不动点 → 报 `z42c.syntax: no method Count/Get/Add on DiagnosticBag`（DiagnosticBag 丢
方法）+ `no static method ToDouble on Convert`（z42.ir bootstrap）。

回退后本地不动点仍报错——**根因是 worktree 环境态**（① `artifacts/build` 被数十次 churn 坏 → `rm -rf` 清掉；
② `.z42` seed 是 07-29，比 B+T1(#91) 旧 → gen1≠gen2，换 08-01 匹配 nightly seed）。修复环境后：**additive
infra 基线不动点 5/5 gen1==gen2 ✅**。

**per-ns 真实 bug 定位 + 修复（clean env，2026-08-02）**：环境修好后接 per-ns `EnsureFq→EnsureNs`，不动点
**可信地**报 `DiagnosticBag no method Count/Get/Add` + `Convert no static ToDouble`。根因：**一个命名空间可跨
多个 MODS 条目**（z42 的 module = 源文件，同 ns 的多个源文件 → 多个 MODS 条目）；`ReadOneModuleTypes/Sigs`
原在第一个匹配处 `return`，漏掉同 ns 其他文件的类/方法。**修复：合并同 ns 的所有模块**（按 MODS 序累积
classes/functions，与整包同序 → byte-identical）。**修复后 per-ns 不动点 5/5 gen1==gen2 ✅**（增量构建残留导致
一度误判"仍红"，`rm -rf artifacts/build` 强制重编后通过）。

**结论**：per-ns 中层（`EnsureFq`/`EnsureIdx` 按命名空间填 + `ReadOneModuleTypes/Sigs` 合并同 ns 模块）
**已验证 byte-identical、字节不动点守住**。`EnsureFq` 现只读引用命名空间的模块（不整包）。下一步：completer
（首次符号 eval 只 reconcile 引用类型，用 `ReconcileOne`）——真正的首次 Console 提速。

## 待 User 裁决
1. 分阶段交付（PR-1 先落 Phase 1）认可？
2. Phase 2 的「REPL-only 惰性、build 保持整包」双路径策略认可？（守自举的代价是 Load 维护两条路径。）
3. ④ 的紧凑索引格式（varint 偏移 + STRS 池名字）在「最小化」约束下认可作为 deferred 备选？
