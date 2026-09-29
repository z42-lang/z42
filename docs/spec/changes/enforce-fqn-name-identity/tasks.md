# Tasks: 名义类型身份一律 FQN，歧义与无解一律响亮

设计见 [design.md](./design.md)。**阶段 1 起需 User 先裁 D-A / D-B（身份表示）。**

## 阶段 0 —— I3 + 三条真缺陷（✅ 已完成，待开 PR）

- [x] 0.1 `StubCollector._passQualifyIfaceNames`：归一**不再就地改写**，产出新数组只换本类型字段引用
      （长度与 `InterfaceCount`/`BaseCount` 原样保留；⚠️ 接口侧尤其不能动长度——`BaseNames` 与
      `BaseRefs` 共用一个 `BaseCount`，错位是静默的）
- [x] 0.2 **A1**：归一 pass 补挂 `SymbolCollector.Collect` 与 `CollectWithImports`
      （此前只挂 `CollectAll`；三份平行 pass 序列漏一处 = 那条路上 `Implements` 恒假）
- [x] 0.3 **A2**：`ClassDescBuilder` 接口限定补点号守卫（镜像同文件基类那格），避免 FQN 被二次限定成
      `Demo.Std.IDisposable`
- [x] 0.4 **A3**：`SymbolTable.InterfaceDerivesFrom` 两个参数对称归一 + 改走 `GetInterface` 双键
      （⚠️ 本项原标号「B3」，与 **D-B 分批的 B3**（`InterfaceNames`/`BaseNames` → `TypeRef[]`）
      同名 —— 阶段 0 那三条真缺陷是 A 系列，已改标 A3 消歧。引用旧标号的地方按此对照。）
- [x] 0.5 带 `namespace` 的真门用例 ×6（`collect_tests.z42`）——既有用例源码都没有 namespace ⇒
      `Fqn()` 退化成短名 ⇒ 断言恒绿 = 空门
- [x] 0.6 字节对账：`base3` vs 带改动 **0 差异**；且 stdlib pass1≡pass2（让结论不依赖会挂死的第二遍）
- [x] 0.7 门：stdlib 347 文件全过 / compiler 自举不动点 3/3 gen1==gen2 / lines / diagcodes / stage2
- [x] 0.8 `xtask test fingerprint` —— **结案为「被更直接的测量覆盖」**，不是跑过了。
      该门的判据是「字节变了 ∧ 指纹没变 ⇒ 红」；本程序每一批都做了**完整 stdlib 字节对账、
      全部 0 差异** ⇒ 前提不成立、无需 bump 指纹。门本身要一棵「stdlib 已由 base 编译器建好」
      的 base 源码树（主 worktree 在 #896 分支上不能当 base，另建买不到新信息）。
- [x] 0.9 ✅ **判别力已验**：撤掉 0.2 的挂钩后，
      `test_iface_names_qualified_under_namespace` / `test_implements_accepts_both_spellings_under_namespace`
      / `test_iface_base_chain_qualified_under_namespace` **确实 FAIL**（共 14 FAIL）⇒ 不是空门。
      源码已自动恢复（`git diff` 空）。
- [x] 0.10 ✅ `xtask build sdk` 通过；`xtask test examples` **95 transcript / 153 step 全过**
- [ ] 0.11 文档同步（doc-check 三道门）+ 开 PR

## 阶段 1 —— P2 消除幻影内建接口（🔴 bootstrap 种子，最高风险）

- [ ] 1.1 摸清 `BuiltinTypeDefs._builtinInterfaces()` 的**真实消费者**：哪些 cold-start 入口在没有
      z42.core 真元数据时依赖它（`bootstrap-seed.md`：删兜底前必须先为**所有** cold-start 入口供种，
      且删+供种是**同一个原子变更**）
- [ ] 1.2 `_extractCore` 的注入改为「只在该注入的地方注入」（候选：仅拥有这些声明的包/ns；
      或改由真实本地声明产出、彻底去掉兜底）
- [ ] 1.3 `ISubscription` 单独处理——11 个里唯一不在 `z42.core/src/Protocols/` 的
      （真声明在 `z42.core/src/Delegates/ISubscription.z42`）
- [ ] 1.4 验证：zsym 应显著变小；`CompilerFingerprint` bump；两代自举
- [ ] 1.5 ⚠️ 失败恢复姿势：`rm -rf artifacts/build/{compiler,libraries}`（**保留 `artifacts/.z42`**）

## 阶段 2 —— P1 建接口歧义判据（✅ 已完成）

- [x] 2.1 新增 `SymbolTable.IfaceNsAll`（对称 `ClassNsAll`：短名 → 全部声明 ns，`StrBox` 以 `|` 连接）
      + `NoteIfaceNs` / `IfaceNsOf` / `IsBareIfaceNameAmbiguous`（判据与 `IsBareNameAmbiguous` 逐条同构）
- [x] 2.2 两处登记：`StubCollector._passInterfaces` 两个分支（本地）+ `ImportedSymbolLoader` 接口循环
      （在无守卫的 last-wins `Put` 旁累积——短名表只留得下一份，本表要的正是被塌掉的那个信息）
      + `SymbolCollector._mergeImports` 逐 ns 并入
- [x] 2.3 `WithAliases` 共享该字段（并行段内只读，同其余 origins 表）
- [x] 2.4 门：**字节与基线恒等**（`DIFF_BASE_S2=0`，50 文件）；stdlib 25/25；
      自举不动点 3/3 gen1==gen2；新单测 ×2 PASS、全套 0 FAIL
- [ ] 2.5 ⚠️ **暂不并进 `ClassNsAll`**（并表才合「判据只此一份」）——并表会让 `IsBareNameAmbiguous`
      对 11 个 prelude 协议名全仓开火。**阶段 3 两表合一**，届时
      `IsBareIfaceNameAmbiguous` 与 `IsBareNameAmbiguous` 要一起塌成一个，别各自漂移

## B2 —— 收口解析入口（歧义 / 无解响亮）

### ✅ 归因已完成（2026-09-27，带上下文探针）

```
2 × IFACEUNRESOLVED IReplCompiler  pkg=z42c.driver  ifaces=19 ifacesFqn=19 classes=887
2 × IFACEUNRESOLVED ICompiler      pkg=z42c.driver  ifaces=19 ifacesFqn=19 classes=887
```

`classes=887` ⇒ **导入上下文完整**，不是单 CU 空表。真相：

- `z42c.driver` **不依赖 `z42.build`**，它依赖 `z42c.pipeline`，而 `z42.build` 是
  **传递依赖、未声明** ⇒ 导入进来的 `Z42cCompiler` 带着 `InterfaceNames = ["ICompiler"]`，
  但 `ICompiler` 本身不在 `z42c.driver` 的接口表里（19 接口 vs 887 类）。
- **这是完全合法的程序**：消费方看得见一个类，不必看得见它实现的每个接口。

## 🔴 结论：「无解 → 报错」作为通则**是错的**，会把合法代码判红

原 I2 的三分支里，「歧义 → 报错」成立（实测代价 0、带阳性对照）；
**「无解 → 报错」不成立**，据此撤销 design §2 I2 的那半条。

### 真正的病根：`TsigReconcile:561` 把命名空间**扔了**

生产方元数据里本来就是 FQ（`Z42.Build.ICompiler`），导入路径 `_shortName` 剥成裸名，
然后我们再去一张**根本不含它**的表里猜回来。
⇒ **修法不是加错误，是别扔。** 这同时解决了悬而未决的 A3/F1 契约问题，方向 = **FQN**：

- `ExportedClassZ.Interfaces` 的形态定为 **FQ**（两个生产者统一：`ClassExtractor` 本就给 FQ，
  `TsigReconcile` 停止 `_shortName`）。
- 导入类的接口引用**自带 FQ** ⇒ 不需要归一 ⇒ **「无解」这个状态从源头消失**，
  而不是被翻成一条会误伤的错误。
- 对 B3（`TypeRef`）是硬前提：导入类型的句柄必须能按 FQN 解析到，
  而 FQ 名要活着穿过导入边界才可能。

⚠️ 改 `TsigReconcile` = 改 `z42.ir`（#896 正在改名该包）⇒ 注意冲突。

### 任务

- [x] B2.0 ✅ 归因完成（见上）——结论推翻了「无解 → 报错」
- [ ] B2.1 `Resolve(scope, name) → id | AMBIGUOUS | NOTFOUND`，唯一入口
- [ ] B2.2 歧义 → 新诊断码（**实测代价 = 0**，带阳性对照）。分配码必须逐个
      `git show <每个在飞 PR 分支>:DiagnosticCodes.z42`，扫 main 不够（已四次撞码）
- [x] B2.3 ✅ **不再把「无解」翻成错误**；`TsigReconcile._rebuildClass` 停止剥短名
      （`ExportedClassZ.Interfaces` 形态定为 FQ）⇒ 该状态从源头消失。契约注释同步已落。
      **stdlib 产物字节恒等**（`test fingerprint`：25 个包逐字节一致）⇒ 无需指纹 slug。
- [x] B2.4 ✅ 阳性对照 —— **比原计划强**：不是「某工程变红」，而是一条**静默错值**被堵住。
      新负例 fixture `src/tests/cross-zpkg/iface_shortname_collision_crosspkg/`，
      单变量对照（同树同命令、只换 driver dist 里的 `z42.package.zpkg`）：

      | | main build | 运行期 |
      |---|---|---|
      | 基线 | `EXIT=0` **零诊断** | `VCall: Demo.IfCollide.Widget.Other not found` |
      | B2a  | `EXIT=1` `E0402: cannot assign Widget to IThing` | — |

      ⚠️ 测这条**必须**先刷 driver 自带的库副本
      （`cp artifacts/build/libraries/dist/release/z42.package.{zpkg,zsym}`
      → `artifacts/build/compiler/z42c.driver/release/dist/`），否则改动根本不在运行的编译器里，
      且完全静默（门全绿、字节只动你改的包）。cp 前后 md5 不同 = 该步不可省的阳性对照。
- [x] B2.5 ✅ **对称的另一半：接口父链** `ExportedInterfaceZ.BaseNames`。
      剥名点在**消费侧**（`ImportedSymbolLoader` 接口循环），与类轴（生产侧剥）方向相反。
      `_bareShortName` → **`_fqTrimTypeArgs`**：只截泛型实参、**保留 ns**
      （不能简单地「不剥」—— 它兼着截实参，且必须先截 `<` 再动 ns，顺序反了「最后一个点」
      会落进实参里、剥出 `String>`）。

      ⭐ **先复现再动手，而复现推翻了我自己的保守猜测**。原以为「导入类的 `InterfaceNames`
      已是传递闭包 ⇒ `Implements` 多半第一层就命中、父链未必可观测」——**恰恰相反**：
      正因为闭包里装的是 FQ、第一层**必然比不中**本地那个同短名接口，才必然落到
      `_anyInterfaceDerivesFrom` 的父链那半。实测（在**已打 B2.3** 的编译器上）：

      | | main build | 运行期 |
      |---|---|---|
      | 仅 B2.3 | `EXIT=0` **零诊断** | `VCall: Demo.IfBase.Impl.Q not found` |
      | + B2.5  | `EXIT=1` `E0402: cannot assign Impl to IParent` | — |

      ⇒ 两条路**互不覆盖**，各有各的门：新负例
      `cross-zpkg/iface_base_shortname_collision_crosspkg`（两层继承 `Impl → IChild → IParent`）。
      关键对照 `iface_base_chain_crosspkg`（父接口带实参 `ILeaf : IMid<int>`）仍 PASS
      ⇒ 「截 `<`、保留 ns」没碰坏实参处理。

## B3 —— `InterfaceNames` / `BaseNames` → `TypeRef[]`（🔨 进行中）

> 前置：B1（intern 表）✅#907 · B2.3/B2.5（FQ 名活着穿过导入边界）✅#916。
> 后者是**硬前提**：绑定 pass 要按 FQN 精确解析，名字被剥过 ns 就绑不准。

### 测绘（2026-09-28 实测，design 估的 ~246 偏大）

`InterfaceNames` 72 处 + `BaseNames` 67 处 = **139**，减去两类**不在范围内**的：

- `GenericConstraint.z42` 7 处 —— 那是 `ConstraintBundle.InterfaceNames`，**同名但另一个类**；
- `z42.package/src/ExportedTypes.z42` 5 处 —— **wire 面，保持 `string[]`**（见下「边界」）。

⇒ 真正要动 **~127 处**，集中在 `z42c.semantics`：`StubCollector` 26 · `Z42Type` 20 ·
`ImportedSymbolLoader` 10 · `SymbolTable` 9 · `InheritanceResolver` 8 · `InterfaceClosure` 6 ·
`SymbolCollector` 4 · `ClassDescBuilder` 4 · `ConstraintChecker` 3 + 测试。

### 🔴 三条定死的设计决策

1. **不能切片，必须一次翻两个字段**。`SymbolTable.Implements` 的 BFS 把
   `ct.InterfaceNames[i]`（种子）与 `it.BaseNames[b]`（沿父链展开）压进**同一个 queue**
   ⇒ 只翻一个，那个 queue 的元素类型就自相矛盾。
   ⭐ 好消息：句柄化让这种「翻一半」变成**编译错误**，而不是字符串时代的静默失配
   —— 那正是选 D-B 的理由（错误写法无法表达）。
2. **登记期拿不到 id ⇒ 必须两趟**。`AddInterfaceName` 被调用时目标接口常常还没进表
   （前向引用）。故：
   - `InterfaceNames : string[]` / `BaseNames : string[]` **降级为收集期暂存**（B5 删）；
   - 新增 `Interfaces : TypeRef[]` / `Bases : TypeRef[]` 为**权威**，由新 pass
     `_passBindTypeRefs` 填充；
   - **所有消费点改读句柄**。B3 之后「消费面」已无裸名，B5 再删暂存字段兑现「写不出来」。
3. **相位：绑定 pass 必须在 `InternAllTypes()` 之后**。现有顺序是
   `_passQualifyIfaceNames` → `_seedObjectStub` → `InternAllTypes()`
   （后者**必须**在 `_seedObjectStub` 之后，它也登记类型）⇒ 新 pass 挂在 `InternAllTypes()`
   之后，三个挂载点（`SymbolCollector` 的 `Collect` / `CollectWithImports` / `CollectAll`）
   **一个都不能漏** —— 漏一处那条路上的句柄全是 0，见 [[parallel-pass-sequences-miss-new-hook]]。

### 边界与不变量

- **哨兵 `0` = None**，有效 id 从 1 起（design §4'.2 已更正）。`new int[n]` 默认全 0
  ⇒ 新建的 `TypeRef[]` 天然全是 None，不必手工填。
- **`BaseRefs` 不动**：它与 `BaseNames` 平行、共用 `BaseCount`，但存的是**声明形态**
  （`IMid<int>` 的 TypeExpr），不是查找键。⚠️ 改 `BaseNames` 长度时 `BaseRefs` 必须同步，
  错位是静默的。
- **wire 一行不改**：`ExportedClassZ.Interfaces` / `ExportedInterfaceZ.BaseNames` 仍是
  FQ 字符串；转换只发生在导入（`ImportedSymbolLoader`，字符串→句柄）与导出
  （`ClassExtractor`/`ClassDescBuilder`，句柄→FQ 字符串）两个边界。⇒ **无格式 bump**。
- **验收 = stdlib 字节对账恒等**（`test fingerprint`）。

### 🔴 风险与恢复

新 pass 跑在 `CollectAll` = **自举必经路径** ⇒ 它崩就编不出修好它的编译器。
按「它会跑在一个我不能重建的编译器里」写：边界、null、空表全部先判，宁可保守返回也别崩。
恢复 = `rm -rf artifacts/build/{compiler,libraries}` + 重铺 nightly SDK 冷种子
（⚠️ 保留 `artifacts/.z42`）。

### 任务

- [x] B3.1 `Z42Type`：`Z42ClassType.Interfaces : int[]` / `Z42InterfaceType.Bases : int[]`
      + `AddInterfaceName` / `AddBaseRef` 同步等长扩容 + 实例化接口视图**共享** def 的 `Bases`
- [x] B3.2 `SymbolTable.BindTypeRefs()`（名字→句柄，走 `GetInterface` FQN 双键；查不到留 0
      并**不报错** —— 合法情形见 B2.0 的归因）
- [x] B3.3 三个挂载点接线（`Collect` / `CollectAll` / `CollectWithImports`，均在
      `InternAllTypes()` **之后**）+ 门 `FirstUnboundIfaceRef()`（只抓**矛盾**的那种：
      名字非空、`GetInterface` 查得到、句柄却仍是 0 ⇒ 只可能是绑定没跑到）
- [x] B3.4 **相等判定**改走句柄：`SymbolTable._sameIface`（唯一出口）+ `Implements` BFS 种子
      + `_anyInterfaceDerivesFrom` 父链展开。
      🔴 **0（未绑定）不参与相等判定** —— 否则「目标绑不到」与「实现方那格绑不到」会
      `0 == 0` 撞成假阳性，而「绑不到」是合法状态。两边都有句柄才比句柄，否则退回名字
      ⇒ 行为逐字不变、**字节恒等**；B5 删这条回落时要单独裁「绑不到」的语义。
- [x] B3.5 单测 `tests/collect/typeref_bind_tests.z42`（6 条，**带 `namespace`**；
      单独成文件是因为 `collect_tests.z42` 已 854 行、逼近 886 硬限）

### 🔴🔴 本批最值钱的产出：逼出了 B1 的一个**既有缺陷**（已在 main 上）

`StrMap.ValAt(i)/KeyAt(i)` 按**槽位**索引，`Count()` 是**条目数** —— 索引空间不同。
`#907` 的 **`_internAllIn`（扫描）与 `_firstUninterned`（门）都写成了 `while (i < tbl.Count())`**
⇒ 只扫前 `_count` 个槽，条目散到靠后的槽就**漏 intern**、留下 `TypeId == 0`；
而门用同一个错误上界 ⇒ **看不见自己漏的东西，永远不红**。

⭐ 它是被 B3 一条无关断言逼出来的：`interface IDer : IBase` 的 `IDer` 恰好落在靠后的槽
⇒ 句柄绑不上；同轮里类那根轴（`Foo` 在前面的槽）**正常通过**，一度误导我去查「两条路的
相位差异」。⇒ **同一个 pass 对 A 生效、对 B 不生效时，先怀疑遍历本身。**

已修四处（B1 的 2 + 本批新写的 3 之中的 3），并按「`while` 上界取 `Count()` + 循环变量直接进
`ValAt/KeyAt`」精确扫过全仓 `.z42`：**误用仅此，其余都是 `ValAt(Find(k))` 的正确用法**。
细节见 memory `strmap-count-vs-slot-scan`。

### 本批**没做**的（如实记，别当已完成）

句柄这一批只接管了**相等判定**。**查找**（`GetInterface(name)`）与**发射**
（`ClassDescBuilder` 写 TYPE 段、`ClassExtractor` 导出）仍走名字 —— 那两者本来就需要名字
（wire 面是字符串），把它们句柄化属于 B5「删字符串回落」的范围。
`Implements` 的 BFS **queue 仍装名字**（下游要按名字 `GetInterface` 找定义）。
⇒ 「裸名在符号表里写不出来」这条**尚未兑现**，B5 才兑现。

## B4 —— `BaseName` → `TypeRef`（类的基类那根轴）

与 B3 同构、同一套约定（**0 = 未绑定**、绑不到不报错、只接管**相等判定**）。
差别只在 `BaseName` 是**标量**、查的是**类**表（`GetClass` 双键）。

- [x] B4.1 `Z42ClassType.Base : int`（int 字段默认 0 ⇒ **不必改 ctor**，正是哨兵取 0 的好处）
- [x] B4.2 `_bindClassBase` 并入 `BindTypeRefs()`（挂载点不变，三处都已覆盖）
- [x] B4.3 **判定收敛成一份** `SymbolTable._sameRef(a, an, b, bn)` ——
      B3 的数组版 `_sameIface` 改为转发到它。🔴 同一判据两份实现正是本仓反复失手的形状
      （`InterfaceKeyOf` 那次「只修一份反而从误报升级成运行期崩」）
- [x] B4.4 `IsSubclassOf` 沿 base 链的比较走 `_sameRef`
- [x] B4.5 门 `FirstUnboundIfaceRef()` 扩到基类轴（`_firstUnboundBaseIn`）
- [x] B4.6 单测 +4（基类绑定 / 两层继承判定 / **0 不参与判定** / 门），与 B3 的 6 条同文件

### ⚠️ 顺带堵的一个一致性风险

`StubCollector` 的 partial 合并会改写 `prev.BaseName`。今天它跑在 `BindTypeRefs` **之前**
所以无害，但**相位是会变的** —— 名字换了而句柄没跟着失效，`Base` 就会指着旧基类，且是静默的。
⇒ 该处显式 `prev.Base = 0`，让绑定 pass 重新解析。

### 🔴🔴 B4 **改变了行为**，而且那是修复 —— 别照抄 B3 那句「字节恒等」

我一开始照 B3 的经验写了「行为逐字不变」，**那是错的断言**，被一次「撤掉改动跑探针」纠正：

`BaseName` **从不经过归一 pass**（`_passQualifyIfaceNames` 只归一接口那两根轴）⇒
它存的是**源码里写的短名**。探针跑在**不带 B4** 的编译器上：

```
B4PROBE B.BaseName=A | C<:B(short)=T | C<:Demo.A(fqn)=F | B<:Demo.A(fqn)=F
```

即 `IsSubclassOf("B", "Demo.A")` 修前返回 **false** —— 同一个继承关系，换成 FQN 拼写就答错，
与 B2 那两条「导入边界剥 ns」**同族的静默错值**。句柄化后两种拼写解析到同一个 id ⇒ 都答 true。

⇒ **输出会变** ⇒ 按 version-bumping 累加指纹条目 `fqn-typeref-base-chain`。
⭐ **累加的理由是「行为实测变了」，不是「指纹门红了」** —— 后者在两侧 driver 不同代时会因
漂移而红，照那种红 bump 是治症状（上一条 `relocate-compiler-domain-libs` 就是那么来的）。
⇒ 单测里那两条 FQN 断言是**修复门**（撤掉 B4 就会红），不是保持原状的回归门。

### 本批同样**没做**的

只接管相等判定。沿 base 链的**查找**仍走 `BaseName`（`GetClass(name)`）——
句柄没有「按 id 取下一层」的路径，那要等 `Base` 成为唯一真相（B5）。

🔜 **顺带暴露的独立缺口**：`BaseName` 那根轴**从来没有归一 pass**，所以今天「基类写限定名 /
拿限定名查基类」全靠拼写碰巧一致。B4 用句柄绕过了它，但**名字那一侧仍是短名** ——
B5 删字符串回落时必须正视这条，或单独给 `BaseName` 补一趟归一。

## B5 第一刀 —— 删名字回落（🔨 已实现）

design 说 B5「恒等、低风险」。**没照做，先量后删** —— B4 刚证明这类估计不可靠。

### 测量（探针带阳性对照 `refHit`，对照失败就中止）

| 面 | `refHit`（走句柄的次数） | `fbTotal`（走名字回落的次数） |
|---|---|---|
| stdlib 全量 19 包 | 42~66 | **0** |
| 真·跨包编译（导入类 → 本地同短名接口） | 2 | **0** |

⇒ 回落**一次都没触发**。机理也说得通：能在源码里写出 `IThing t = ...` 就意味着该接口
解析得到、在表里 ⇒ `targetRef != 0`；而「目标在表里、被查那一格却没绑上」是**矛盾**状态，
由 `FirstUnboundIfaceRef()` 那道门专门抓。两种 0 都不该出现在判定里。

- [x] B5.1 `_sameRef(a, b)` 删名字回落；**连参数一起删**（留着 `an`/`bn` 就是留着那条路）
- [x] B5.2 `_sameIface` 跟着收窄
- [x] B5.3 `_anyInterfaceDerivesFrom` 里**手写的第二份**同款逻辑收敛进 `_sameRef`
      —— 那是 B3 时留下的「同一判据两份实现」，本仓反复失手的形状

### 🔴 它把一个**隐含契约**变成了硬约束（一条单测红了，红得准）

`ConversionTests.test_class_ref_conversions` **手搭符号表**（`syms.Classes.Put(...)`）、
从不调 `InternAllTypes()`/`BindTypeRefs()` ⇒ 句柄全是 0 ⇒ 删回落后判不出继承关系。

核实过**不是**真缺陷：生产路径的 3 个 `new SymbolTable()` 构造点**全部**紧跟 Intern+Bind
（第 4 个是 `WithAliases` 视图，共享同一份 intern）⇒「没 intern 的符号表」只存在于测试里。
另外 3 个手搭符号表的测试文件（layout/codegen/bound）既不设继承关系也不走名义判定 ⇒ 不受影响、
**也不是空门**。

⇒ 处置：测试补调两步 + **把契约写进 `SymbolTable` 类抬头**。漏调的后果是**静默的**
（所有名义关系判 false，不报错），不写明下一个手搭的人照样踩。

## A 轴 —— 查找也句柄化（User 2026-09-29 裁定走这条，**不是**把键翻成 FQN 字符串）

### 测绘推翻了原计划

原计划（design 的「A：键面 FQN 化」）是把 `SymbolTable` 的键换成 FQN 字符串、删掉四个
访问器的短名回落。**实测否掉了它**：

| 指标 | 值 |
|---|---|
| 四访问器的短名回落命中（一次真·跨包编译） | **165** |
| 同一次的 FQN 命中 | 142 |
| 回落命中的类型 | `Exception` `String` `Int32` `List` `Dictionary` `Console` … 全是 stdlib |
| 全仓 `Name()` 调用点 | **386** |
| 全仓 `Fqn()` 调用点 | 30 |

⇒ **A 轴的回落是主力路径，不是死代码**（与 B 轴正相反 —— 那边实测 0 触发所以能删）。
原因不是导入类没进 `ClassesByFqn`（`ImportedSymbolLoader:266` 登记了），而是**调用方传的
就是短名**：`cur.BaseName` / `c.Name` / `ownerKey` / `clsName` …… 而 `Name()` 按设计返回短名，
386 处依赖它。

⇒ 「删回落」的前提是先让那 386 处拿到 FQN，那等于 design 标着「硬需 zbc/zpkg 格式 bump」的
**B 轴身份 FQN 化** —— 不是收尾动作，是另一个大工程。

### 🔴 而且把键翻成 FQN 字符串是**性能负向**的

design 自己写着：`GetHashCode` 是 FNV-1a over UTF-8、**O(n) 每次重算无缓存**，profile 里
自占 **1.93%**；FQN 化让**键长 +107%**。⇒ 翻成 FQN 字符串 = 把最热的哈希再拉长一倍。

真正正向的是**句柄**：int 比较替掉字符串哈希。B1–B5 已经在名义关系轴上兑现了这一点
（`Implements`/`IsSubclassOf` 现在零字符串哈希）。A 轴照同一条路走。

### 目标形态

**解析一次、之后传 id**：

```
源码名字 ──(一次)──> Resolve(scope, name) -> TypeRef ──(之后全程)──> int 比较 / 数组索引
```

- `ResolveTypeP` 已经是**事实上的作用域解析入口**（型参 → 别名 → prim → 内建 → enum →
  delegate → `ScopeNs` 优先 → 表查），A 轴把它的结果 intern 成 id 并让消费点持有 id。
- `Classes` / `ClassesByFqn` 最终退成**解析期**的索引（只在 `Resolve` 里用一次），
  消费期不再碰它们 ⇒ 四个访问器的短名回落随之自然消失，而不是硬删。
- `Name()` / `Fqn()` 保留为**显示/wire** 用途（诊断文本、TYPE 段），不再是判定依据。

### 分批（每批可单独对账、顺序不能反）

- [ ] A1 `TypeIntern` 扩出 **FQN → id** 的反查（今天只有 `id → Z42Type`；`Of()` 靠
      `Z42Type.TypeId` 幂等）。**零消费方**，预期字节恒等
- [ ] A2 `Resolve(name) -> TypeRef` 唯一入口，内部复用 `ResolveTypeP` 的作用域规则；
      先**只加不用**，用探针量它与现有 `GetClass` 的答案是否逐一致（分歧即缺陷）
- [ ] A3 热点消费点改持 id（先挑 `Implements`/`IsSubclassOf` 的 base 链走查 —— 那里今天
      每跳一层都要 `GetClass(名字)` 一次字符串哈希）
- [x] A4 ✅ `Z42ClassType.Base` 已是 id（B4）⇒ base 链走查**完全脱离名字**：
      `IsSubclassOf` / `Implements` 的链上每一跳走 `Intern.At(ct.Base)`（数组索引、零哈希），
      只有**入口**那一次按名字查。`Base` 为 0（跨包基类不可见）仍回落名字，留到 A5。
      ⚠️ 顺带把 `Implements` 的 `Classes.Find`（**只查短名表**）换成 `GetClass`（双键）= 超集。
      **判别力已验**（注入 `_bindClassBase` 不绑 ⇒ 断链）：20 条红，新门
      `test_base_chain_is_fully_handle_linked` 精确命中，且 `test_derived_to_base_arg_is_clean` /
      `test_closed_hierarchy_*` 也红 ⇒ **断链是真回归、不只是性能退化**。
      ⭐ 新门守的是「那条路还在」—— 断链在功能上仍对（回落名字），既有断言一条都不会红。

> 🔴🔴 **对账前必须把两侧代数拉平，否则结论是噪声**（同一个陷阱本 change 内踩了**四次**：
> B4 / B5 / A4 各一次，A5b 一次，每次都表现为「`z42.json` 差 11 字节」）。
> `build compiler` / `build stdlib` **每跑一遍推进一代**，而 base 树不动 ⇒ 相邻两次实验的基线
> 在悄悄移动。**固定流程**：`rm -rf artifacts/build/compiler` → 铺同一份 SDK 种子 →
> `build compiler` **一次** → `test fingerprint`。（不要再跑 `build stdlib`：门内部会用本树
> driver 编 base 的 stdlib 源码，不需要本树的 stdlib 产物，多跑一次就多一代。）
>
> ⚠️ **第四次换了伪装，单独记一笔**：前三次是「多跑了一遍 `build stdlib`」；第四次是
> **`test all` 自己推进了一代** —— 它内含 `--workspace` 自建两遍做自举不动点检查。
> 于是「冷建 → `test all` → 对账」与「冷建 → 直接对账」**不可比**，而两条看起来都像在照流程做。
> ⇒ **对账必须紧跟冷建，中间不许插任何会自建的命令**（`test all` / `build stdlib` / `build all` 都算）。
> 顺序应是：冷建 → **对账** → 再跑 `test all`。
>
> ⭐⭐ 这次差点判错的方向是**反的**、也更危险：对照（无改动）说「19 个包一致」、实验（有改动）
> 说「z42.json 差 11 字节」，我一度据此写下「同一签名前三次是噪声、**这次是真的**」，差一步就去
> bump 指纹。救回来的不是模式匹配，是**「为什么效应这么窄」这一问**：`Std.Object` 正好 11 个字符、
> 看着极像字符串池多了一条，但 z42.json 的源码里**根本没有裸 `Object` 类型引用** —— 它没理由
> 只进这一个包。追这个不一致才发现两次运行的协议不同。
> ⇒ **A-B-A（同协议把实验再跑一遍）才是这类归因的终判，单次 A-B 不够** —— 因为门自己会往
> `artifacts/` 写东西，两次运行并不独立。
- [x] A5**a** ✅ base 链走查的 **16 处**消费点全部收敛到 `SymbolTable.BaseOf(ct)` 这一个出口
      （句柄在场走 `Intern.At`＝数组索引；`Base == 0` 时才回落名字）。顺带塌掉三处**双重**哈希
      （`HasClass(name)` + `GetClass(name)` 双查）。**字节恒等**（代际受控对账，19 包逐字节一致）
      ⇒ **不 bump 指纹**。
      > ⚠️⚠️ **第一刀只迁了 13 处，漏了 3 处** —— `ClassExtractor` / `DeclBinder` /
      > `ForeachProtocol`，它们的持有变量不叫 `ct`（`walk` / `curCls`），**凭记忆扫目录扫不出来**，
      > 是写 PR 描述时回头做全仓 grep 才抓到的。同 `rename-sweep-must-start-from-grep`：
      > **清扫的第一步是 grep，不是回忆**。自检判据已写进 `BaseOf` 头注：
      > `grep -E "(GetClass|HasClass)\([^)]*BaseName" src/compiler/` 只应剩两处 ——
      > `Origins:527`（**门的判据**，刻意按名字查以发现「查得到却没绑」的矛盾）与 `BaseOf` 自身的回落。
      > ⭐ `ForeachProtocol` 那处**不是**单纯的链上走一步，而是拿 `HasClass` 当**可见性探针**
      > （基类不可见 ⇒ 保守当它有 `Dispose`）。塌成一次前验了等价性：`HasClass` 与 `_refOfClass`
      > 走同一套双键（`ByFqn` → 短名）⇒ `Base != 0` ⟺ `HasClass` 真。**两者键覆盖若不同这一合就是
      > 静默行为变更** —— A4 就撞到过 `Classes.Find` 只查短名表。
      > ⚠️ 行数门顺带红了（`SymbolTable.z42` 891 > 886 硬限）⇒ 按 `SymbolTable.Origins.z42` 先例
      > 拆出 `SymbolTable.Nominal.z42`（名义关系那一簇，652 + 263 行），**纯搬运**、方法体逐字未改。
- [x] A5**b-1** ✅ **`Object` 的两个名义身份合一** + prelude ns 收成一份 SoT（`PreludeNs.z42`）。
      `_seedObjectStub` 造的 `Object` 骨架此前既没有 `Namespace`（`Z42ClassType` 的构造函数把它
      默认成 `""`、由**注册方**覆写，而这里从没写过）也不进 `ClassesByFqn` ⇒ 它的 `Fqn()` 是裸
      `Object`，与真·导入的 `Std.Object` 是**两个不同的名义身份、两个不同的 intern id**。
      实测（全量 `build stdlib`）：`fqn=Object` **4975** 次 · `fqn=Std.Object` **887** 次。
      它同时是 A5b「删短名回落」的**唯一**阻塞 —— 那 4975 次查到的对象连 FQN 都没有，
      任何按 FQN 的解析都够不着。**字节恒等**（A-B-A 验过）⇒ 不 bump 指纹。
- [x] A5**b-2a** ✅ `GetClass` 收敛到作用域感知的 `_resolveClass`（`ScopeNs` → FQN → `using` →
      prelude → 裸名）；**同时**把 `MemberCollector._fillClass` 从直接摸 `Classes` 收敛到访问器 ——
      否则同短名跨 ns 时成员填进赢家、绑定解析到输家，`ms` 为 null 直接崩。字节恒等。
- [x] A5**b-2b-1** ✅ 补上 4 处**同包跨 ns 缺 `using`** 的既有违规（`TsigIndex` / `ZbcStringPool` /
      `ZbcFormat` / `z42.core/Type.z42`）。字节恒等。

### 🔴🔴 A 轴收口的**依赖顺序**（2026-09-30 定，顺序不能乱）

User 2026-09-30 裁定走 **A：按规范办**（补 `using` + 让规则 2 在包内也真生效），而不是
承认「同包跨 ns 免 using」。实施时发现**四步互为前置**，跳步就会立出瞎门：

1. ✅ **补既有违规的 `using`**（本批）。实测全仓**只有 4 条**可判违规 —— 都是
   **同包跨命名空间**（`z42.package` 一个包里住着 `Z42.IR` / `Z42.IR.BinaryFormat` /
   `Z42.Package` 三个 ns），而 `E0436` 的判据 `cm.UsedDepNs` **只覆盖跨包依赖** ⇒ 全部漏掉。
2. 🔜 **修 enum 身份**（`Z42ClassType.Enum()` 的 `SymbolTable:544` / `MemberResolver:67`
   两个调用点不查 `EnumTypeNs` ⇒ 造出的 enum `Namespace` 为空）。
   ⚠️ **不先做这步，门在 enum 轴上是瞎的** —— 空 ns 不参与可见性判定，而实测
   `ZbcFormat.z42` 缺的那个 `using Z42.IR;` 正是为了一个 **enum**（`IrType`）。
   典型的「门会绿，但它没在守」。
3. 🔜 **补作用域**：`ResolveTypeP` 仍有大量调用跑在**没设作用域**的表视图上
   （A5b-2a 实测 2802 次 / 本批探针 174 次引用方 ns 为空）⇒ 那些路径上可见性判不了。
4. 🔜 **立门**（把规则 2 的判据从 `UsedDepNs` 扩到「所有用到的 ns」）→ 然后才能**删裸名回落**。

> ⭐ 教训：**我一度把「父 ns 隐式可见」当成 z42 的语言规则**（C# 的直觉），据此以为
> 「裸名表在替一条没实现的规则工作」。读 `namespaces.md` 规则 2/4 才发现**方向是反的**：
> z42 刻意没有这条规则，是**裸名表让违反规范的写法静默通过**。
> ⇒ 碰到「实现与预期不符」时，先查规范怎么写，别用别的语言的直觉补全。

- [ ] A5**b-2c** 删四访问器的短名回落 + 清死码（**必须在上面 2/3/4 之后**）

### A5b 测绘（2026-09-29，探针跑全量 `build stdlib`，21,078 次 `GetClass`）

`Resolve` 的四步各能吃掉多少 —— 决定 A5b-2 的量级：

| 解析步 | 次数 | 占比 |
|---|---|---|
| ① `ScopeNs` 外围 ns 优先（`ResolveTypeP` 同口径） | 10,142 | 48% |
| ② 调用方给的就是 FQN | **0** | **从来没有** |
| ②′ 逐个 `using`（含 prelude） | 3,123 | 15% |
| ③ **裸名表（要删的那支）** | 7,813 | 37% |
| ⓪ 查不到（合法） | 62 | — |
| **跨 `using` 歧义** | **0** | — |

🔴 **`GetClass` 的第一条分支 `ClassesByFqn.Find(name)` 一次都没命中**（21,078 次全 miss）——
热路径上每次白算一遍 FNV-1a。A5b-2 的 `Resolve` 要顺手把这条顺序理顺。
🔴 **差分 0 条、歧义 0 条** ⇒ `Resolve` 不改变任何答案，A5b-2 应当也是字节恒等。
⭐ 655 个走到短名回落的名字里**带点的是 0 个** ⇒ 回落唯一在干的事是**解析非限定名**，
不是给限定名兜底 ⇒ **不能先删再看谁红**（135 个调用点全是字符串键，删了一处都不会编译报错）。
- [ ] A6 性能对账：`GetHashCode` 在 profile 里的占比应下降（基线 1.93%）

## 阶段 3（原字符串路的计划，D-B 选定后由 B2~B5 取代）—— I2 归一唯一出口响亮化

- [ ] 3.1 `IfaceFqnOf` 三分支化：唯一解 → FQN；多解 → 新诊断码；无解 → `E0401`。**删掉原样返回**
- [ ] 3.2 新诊断码走 `DiagnosticCodes.z42` 登记表（⭐ 分配码**必须逐个
      `git show <每个在飞 PR 分支>:DiagnosticCodes.z42`**，扫 main 不够——已四次撞码）
- [ ] 3.3 消息与判据**各只一份**（照 `AmbiguousBareNameMsg` / `IsBareNameAmbiguous` 的既定纪律）
- [ ] 3.4 收口 5 处过期契约注释（design §7）
- [ ] 3.5 ⚠️ 预期会让 `examples/` 变红——`run.console` 把期望的错误也记着

## 阶段 4 —— I1 扩到类基类轴 + 删死码

- [ ] 4.1 `BaseName` → FQN（`IsSubclassOf` 及其调用方）
- [ ] 4.2 ⚠️ `SymbolTable.Implements:361` 的 `this.Classes.Find(cur)` **绕过 `GetClass`**——
      今天安全（7 个调用方第一参数全是短名），但**翻 `BaseName` 那一刀落下时它会静默截断 base 链**
- [ ] 4.3 删死码：发射端「歧义→退回短名」降级（`FunctionEmitter.z42:243` / `ClassDescBuilder.z42:81`）、
      `ResolveTypeP` 的两个 ScopeNs 改道分支
