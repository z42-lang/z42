# Design: 名义类型身份一律 FQN，歧义与无解一律响亮

**状态：DRAFT，待 User 裁决。** 出身：[`fqn-as-symbol-key`](../../../../.claude/) 程序步 2-接口。
本轮实测（2026-09-27）在「把 `InterfaceNames` 预先归一成 FQN」这一刀上连踩三类缺陷，
逐个修完发现它们**同源**——不是三个 bug，是三条缺失的不变量。本文定的是那三条不变量。

## 1. 问题陈述：名字的「形态」是隐式的、逐处约定的

今天名义类型身份以**字符串**在编译器里流动，而「这个串是短名还是 FQN」**没有任何地方声明**，
全靠每个生产者/消费者各自记得。本轮观测到的四个事实都长在这上面：

| # | 事实 | 证据 |
|---|---|---|
| F1 | **同一个字段两个生产者两种形态** | `ExportedClassZ.Interfaces`：`ClassExtractor.z42:375` 给 FQN（别名 `ct.InterfaceNames`）、`TsigReconcile.z42:561` 给裸名（`_shortName`） |
| F2 | **归一不是稳定函数** | `IfaceFqnOf` 走包级 first-wins 短名表；同一短名在生产包与消费包可归一到不同 FQN |
| F3 | **归一不是纯函数** | 就地改写 `ct.InterfaceNames[i]`，而该数组被导出记录与 `ImportedSymbols` 缓存共同别名 ⇒ 写穿。**实测产出野引用**（探针：下游包 `String.InterfaceNames` 3 条、无一是 `IEquatable`，[0] 的"内容"是约 1.6KB 堆字节）⇒ `E0402 String 不满足 IEquatable`，stdlib 编不过 |
| F4 | **歧义静默** | 短名多 ns 时 first/last-wins 挑一个；**接口连判歧义的数据都没有**（`ClassNsAll` 只有类登记：`StubCollector.z42:386` 在类分支、`ImportedSymbolLoader.z42:181` 在类循环；`_passInterfaces` 与接口导入侧都无对应登记） |
| F5 | **边界不自洽** | `Z42ClassType.InterfaceCount` 可 > `InterfaceNames.Length`（导入侧 `nct.InterfaceNames = cl.Interfaces; nct.InterfaceCount = cl.InterfaceCount` 两值各来各的）⇒ 按 count 遍历越界读 |

⭐ F3 已在阶段 0 修掉（改为产出新数组），字节恒等、全绿。F1/F2/F4/F5 是本文要解决的。

## 2. 三条不变量

- **I1（形态）** 符号表中一切名义类型引用恒为 **FQN**——`Z42ClassType.BaseName` /
  `.InterfaceNames` / `Z42InterfaceType.BaseNames`，无例外。
- **I2（归一是全函数且响亮）** 短名 → FQN 只有**一个出口**，且只有三种结果：
  唯一解 → FQN；**多解 → 诊断**（新码）；**无解 → 诊断**（既有 `E0401`）。
  **不存在「查不到就原样返回」这条静默支**——那条支正是 F2/F4 的载体。
- **I3（不可变）** 归一**从不就地改写**任何数组；产出新数组、只换本类型自己的字段引用。
  名字数组与其计数**同生同换**（消除 F5 的可乘之机）。

## 3. 🔴 两个硬前提（不先做，I2 落地当场全仓爆红）

### P1. 接口的歧义判据不存在——要先建

类有 `ClassNsAll`（短名 → 全部声明 ns），接口**没有对称物**。I2 的「多解 → 报错」需要它。
⇒ 新增 `IfaceNsAll`，两处登记：本地 `StubCollector._passInterfaces`、
导入 `ImportedSymbolLoader` 接口循环（**必须在 first-wins 守卫之外**，守卫之内只有赢家能进——
这正是 `ClassNsAll` 当初踩过的坑，照抄它的手法）。

### ~~P2. 幻影内建接口会把「歧义」制造成全仓常态——必须先消除~~ 🔴 **已实测证伪，不是前提**

> **2026-09-27 实测更正**：撤掉 `_extractCore` 的内建接口注入后，`build stdlib` 25/25 通过，
> 且产物与基线**逐字节完全相同**（`diff -rq` 0 行）⇒ **那批注入从来没有写进 wire**，
> 导入方根本看不到它们 ⇒ **它们不可能制造任何跨 ns 歧义**。
> 下面这段原本把 P2 写成 I2 的硬前提，**那个因果是错的**（这已是同一个幻影理论第二次被证伪：
> 第一次是误当作 `E0402` 回归的病因）。
>
> 保留这一刀的理由降级为：**纯死代码删除**（14 增 18 删、输出零变化），顺带去掉每 CU 23 条
> 无用的内存条目。**它不再阻塞阶段 3。**
>
> ⚠️ 真实歧义面**由 `IfaceNsAll` 实测给出**（阶段 2 已建好这把量尺），不再由本节推断。
>
> 以下原文保留作为「机制确实存在」的记录——但**机制存在 ≠ 造成后果**，这正是本轮最贵的教训。

`ExportedTypeExtractor._extractCore:168-174` **无条件**把 `BuiltinTypeDefs._builtinInterfaces()`
的 11 个接口塞进**每一个** `ExportedModuleZ`，而模块 ns = 该 CU 的 ns。
z42.core 一个包就横跨 **8 个 ns、116 个 CU**（`Std` 77 / `Std.Reflection` 9 / `Std.Collections` 8 /
`Std.Runtime` 7 / `Std.IO` 7 / `Std.Time` 6 / `Std.Threading` 1 / `Std.Net.Sockets` 1），
其余每个包再各加一批 ⇒ `IEquatable` 以 `Std.IEquatable`（真）+ `Std.Collections.IEquatable` /
`Std.IO.IEquatable` / …（幻影）多份存在。导入侧 `ImportedSymbolLoader.z42:312`
`r.Interfaces.Put(iz.Name, nift)` **无守卫 ⇒ 裸名键 last-wins**。

⇒ 一旦 I2 把歧义翻成错误，`IDisposable` / `IEquatable` / `IEnumerable` … **每一处使用都报歧义**。

**而且不止接口**——同一处 `ExportedModuleZ` 构造还无条件注入 `_builtinEnums()`（1 个 `GCHandleType`）
与 `_builtinDelegates()`（11 个 `Action`/`Func`/`Predicate` 族）⇒ **每个 CU 带 23 个幻影条目**，
光 z42.core 的 116 个 CU 就是约 **2668 条**。既是 zsym 白占，也让 `Action`/`Func` 同样多 ns 幻影。

10/11 个接口在 `z42.core/src/Protocols/` 有真声明（唯一例外 `ISubscription` 在
`z42.core/src/Delegates/ISubscription.z42`），靠注释里写的「与 Protocols/*.z42 **逐字锁步**」
手工维持——即 memory 早记的候选「prelude 漂移」。

**出身**：`git log -S"_builtinInterfaces"` 只翻得到两次重构，注释自陈「镜像 C# `sem.Interfaces`
单条目」「字节形态 = C# 同源实测」⇒ 这是**移植残留**：C# 那边是每次编译一份全局列表，
移植时按**每模块**复制了一份，没人注意到「模块带 ns」这个后果。

⚠️ **P2 受 `bootstrap-seed.md` 管辖**：`BuiltinTypeDefs` 是**构建期兜底种子**，
删兜底必须在**同一个原子变更**里为所有 cold-start 入口供种。本程序已两次实测踩到
「新逻辑毒化自举 ⇒ 编译器编不动修好它的源码」的死锁（恢复 = `rm -rf artifacts/build/{compiler,libraries}`，
⚠️ 保留 `artifacts/.z42`）。

> **澄清**：2026-09-27 我一度把 P2 当作本轮 `E0402` 回归的病因**并据此请 User 裁了方向**，
> 那是错的——三次复现均不触发，探针证明查询侧 `q=Std.IEquatable` 完全正确，真因是 F3。
> P2 作为 **I2 的前提**成立，作为**本轮回归的病因**不成立。两件事，别再混。

## 4'. ✅ User 已裁：走 **D-B（句柄）**，「按最根本的方案推进」

### 4'.1 核心属性

要的不是「约定名字都写 FQN」，而是**让「裸名」在符号表里无法表达**。
⇒ 名义引用的存储类型**不再是 string**：

```
Z42ClassType.InterfaceNames : string[]  →  Interfaces : TypeRef[]
Z42InterfaceType.BaseNames  : string[]  →  Bases      : TypeRef[]
Z42ClassType.BaseName       : string    →  Base       : TypeRef
```

`TypeRef` = 指向 intern 表的**稠密整数句柄**。于是：

| 今天可能出的错 | D-B 之后 |
|---|---|
| 某处塞进一个裸名 | **写不出来**（类型不对，编译期红） |
| 归一在 A 包与 B 包给出不同答案 | 不存在「归一」这一步；身份在**登记时**就定了 |
| 就地改写共享数组产出野引用 | `TypeRef[]` 由登记方一次建好，消费方只读 |
| `Count` 与 `Length` 错位越界 | 数组与计数**合一**（长度即计数） |
| 歧义静默挑赢家 | 解析是**唯一入口**，三态：唯一 / 歧义→诊断 / 无解→诊断 |

### 4'.2 表示

仓里**没有整数键表**（无 `IntMap`）⇒ 用**稠密 id + 数组索引**：

- `TypeIntern`：`StrMap fqn → int`（**只在登记时查一次**）+ `Z42Type[] byId`（消费期 O(1) 数组索引）。
- ⇒ 顺带兑现「句柄路性能是正向的」：今天 `__str_hash_code` 是 FNV-1a over UTF-8、
  **O(n) 每次重算无缓存**，`GetHashCode` profile 自占 1.93%；FQN 化还让键长 +107%。
  句柄把消费期的字符串哈希整个消掉。
- **L1 约束**：没有 newtype，`TypeRef` 就是 `int`。
  这保不住「任意 int 不能乱塞」，但保得住**核心那条：裸名字符串塞不进去**。
  不引 struct 包装 —— 本仓 struct 仍有已知拷贝缺陷，不值得为此冒险。
- 🔴 **哨兵 = `0`（未登记 / None），有效 id 从 1 起**。本行原写「配 `TypeRef.None = -1`」，
  **已按 B1 的落地实现更正**（#907 的 `TypeIntern.z42`，抬头注释给了理由：z42 字段默认 0，
  用 -1 当哨兵就得在**每个** `Z42Type` 子类 ctor 里赋值，多生产点必漏）。
  ⭐ **这条在 B3 语境下比 B1 时更要紧**：B3 把 `string[]` 换成 `TypeRef[]`（即 `int[]`），
  而 `new int[n]` 的元素**默认就是 0** ⇒ 哨兵取 0，新建数组天然全是 None；
  取 -1 则每建一个数组都要手动填满，**漏一次就静默指向越界 id**。

### 4'.3 边界：VM 与 wire **一行不改**

句柄只活在编译器内存里。持久化面仍写 FQ 字符串（zbc 引用侧本就是 pool idx，
STRS 还是 segment-dict）⇒ **无格式 bump**，每一批都能做字节对账。

### 4'.4 分批（每批独立全绿、可单独 PR、字节可对账）

| 批 | 内容 | 字节 | 风险 |
|---|---|---|---|
| **B1** | `TypeIntern`（FQN↔id + byId）；**所有登记点**（本地 stub / 导入 loader）登记时 intern。**零消费方** | 恒等 | 低 |
| **B2** | 唯一解析入口 `Resolve(scope, name) → id \| AMBIGUOUS \| NOTFOUND` + 诊断；先只接到接口归一那条链（歧义实测为 0 ⇒ 仍恒等，但**门从此会红**） | 恒等 | 低 |
| **B3** | `InterfaceNames`/`BaseNames` → `TypeRef[]`（今天出事的那根轴；~246 处） | 恒等 | 中 |
| **B4** | `BaseName` → `TypeRef`（~116 处） | 恒等 | 中 |
| **B5** | 删字符串回落、`Fqn()` 由 id 派生、两张歧义表合一 | 恒等 | 低 |

🔴 **顺序不能反**：先建 intern（超集、无人消费）→ 再收口解析 → 再翻存储类型。
反过来做，中间态就是「一半句柄一半裸名」——正是本轮已经付过学费的形状。

### 4'.5 每批的验收门（固定三条）

1. **字节对账**：与基线逐字节相同（本程序全程用它当主门，已抓出过真回归）。
2. **阳性对照**：新加的门必须用合成用例证明**它真会红**
   （已备好：`namespace Demo.A/Demo.B` 各声明 `IFoo` 的工程）。
3. `xtask test compiler`（含 z42c 单测 + 自举不动点 gen1==gen2）+ `build sdk` → `test examples`。

---

## 4. （原始待裁记录，保留）身份的**表示**——字符串 + 不变量，还是句柄？

这是「从根本上解决」的分叉点。

### D-A：保持字符串，用不变量 + 门约束

- 改动面小，阶段可拆，每步字节可对账。
- **但 I1/I2 靠纪律与门维持，不靠类型**：任何新代码仍然**写得出**一个裸名塞进 `InterfaceNames`，
  而这类错误**全是静默的**（本轮四个事实无一在编译期报错）。

### D-B：名义身份 intern 成句柄 `TypeId`（推荐）

- 符号表键与一切名义引用换成整数句柄；短名 → `TypeId` 的解析成为**显式一步**，
  只能产出「唯一解 / 歧义诊断 / 无解诊断」。
- **「裸名」在符号表里变得无法表达** ⇒ I1/I2 由类型系统强制，而不是由注释和门强制。
  这正是本仓自己总结过的判据：**好的机制让错误写法无法表达**（`static-init-unification-program`）。
- 顺带收掉 F2（句柄天然无歧义）、F5（身份与计数不再是两个可错位的字段）。
- memory 已记它是「未采纳的更优解」，且**性能是正向的**：实测 `__str_hash_code` 是 FNV-1a over UTF-8、
  **O(n) 每次重算无缓存**，`GetHashCode` 在 profile 里自占 1.93%；FQN 化让键长 +107%
  ⇒ 字符串路对内存查表是负向的，句柄路反而更快。
- 代价：改动面最大（符号表键面 + 167 处访问器调用点的语义、虽然调用点本身已收在四个访问器里）。
  **VM 与 wire 格式不受影响**——句柄只活在编译器内存里，持久化面仍写 FQ 串（zbc 引用侧本就是 pool idx）。

**建议**：D-B 作为身份内核，D-A 的三条不变量作为**迁移期**的过渡契约。
若 User 选 D-A，则必须接受「I1/I2 永远是纪律而非强制」，并为此配一道**响亮探测**门
（扫符号表里是否存在不含 `.` 且其短名多 ns 的名义引用）。

## 5. 分阶段（每步独立全绿、单独 PR）

| 阶段 | 内容 | 字节 | 备注 |
|---|---|---|---|
| **0** ✅ | I3（不就地改写）+ A1 归一 pass 补挂三条 pass 序列 + A2 点号守卫 + B3 对称 | **恒等**（实测 0 差异） | 已全绿，可立即落地 |
| 1 | **P2** 消除幻影内建接口（供种 + 删兜底同一原子变更） | zsym 显著变小 ⇒ 变 | bootstrap 种子，最高风险 |
| 2 | **P1** 建 `IfaceNsAll`（对称 `ClassNsAll`） | 恒等 | 纯增数据，无行为变化 |
| 3 | **I2** 归一唯一出口改「唯一解/歧义报错/无解报错」，删静默支 | 可能变 | 新诊断码；会让今天靠偶然对上的写法变红 |
| 4 | **I1** 扩到 `BaseName`（类基类轴）+ 删死码（发射端「歧义→退回短名」降级） | 变 | |

若选 D-B，阶段 3/4 换成句柄化，阶段 0~2 不变（它们是共同前置）。

## 6. 风险

- 阶段 1 踩错会**锁死自举**（已实测两次）。
- 阶段 3 会把「今天靠偶然对上」的写法翻红，可能波及 `examples/` 与 learn 书
  （⚠️ `run.console` 把期望的错误也记着 ⇒ 修好缺陷 = 演示该缺口的示例变红）。
- 阶段 1/3/4 都会改字节 ⇒ 各自需要 `CompilerFingerprint` bump；格式 minor 是否要动按
  `version-bumping.md` 逐阶段判。

## 7. Out of scope

- `ExportedClassZ.Interfaces` 的 wire 形态：本轮字节对账 **0 差异**证明落到 wire 的形态**没变**，
  故 F1 目前只是**内存契约 + 5 处过期注释**问题（`ExportedTypes.z42:161`、`TsigReconcile.z42:556`、
  `z42c.semantics/README.md:36`、`source-compile.md:81`、`conversions.md:38`），随阶段 3 一并收口。
- F5 的彻底修法（`InterfaceCount`/`InterfaceNames` 合成一个不可分对象）随 D-B 自然消解；
  选 D-A 则需单独一刀。
