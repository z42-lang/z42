# Proposal: struct 字段访问符号化（并重估泛型单态化的必要性）

> 类型：`ir` + `vm`（**改 zbc/zpkg 格式** ⇒ 走阶段 1–9 完整流程，需 User 确认后才 IMPL）
> 状态：**DRAFT**（2026-09-27）
> 前序：`complete-generic-instantiation`（S1 已落地；S2 = 跨包模板投送，未做）
> 出身：结构审计 2026-09 的 **R1**（烘焙偏移）+ 裁决项 **D-2**
> User 已给的两条约束：**① 先正确性、后性能**；**② 值类型不能无端封箱**

## 一句话

今天泛型单态化的**唯一实际动因**是「struct 字段偏移在编译期被烘焙进指令」。
把这件事符号化，单态化的绝大部分就不必存在；但它**免不掉 ABI（sret）那一格**——
那一格要另外决定，而 User 的「不许无端封箱」已经把其中一条出路排除了。

## 为什么重新上桌（这条曾被否决）

`complete-generic-instantiation/proposal.md:156-161` 记着：符号化访问 **2026-09-24 已被否决**，
理由是「User 选择对齐 C# 的零运行期代价」。

**那条理由的前提不成立。** IL 的 `ldfld` 带的是**字段 token**（符号），C# 的「零代价」来自
**CLR 的 JIT 在运行期烘焙偏移** —— 也就是说 .NET 的模型恰恰是「IL 符号化 + 运行期特化」。

⚠️ 但**同一条记录的代价估计又过于乐观**，也要更正。它写「运行期零件齐备」并举了三件：

| 归档稿说的 | 实测（2026-09-27） |
|---|---|
| struct blob 自带类型名 | ✅ 真（`StructAllocInstr.TypeName`） |
| `resolve_layout` 本就按名查 | ✅ 真（`interp/exec_struct.rs::resolve_layout`；`Size` 只是兜底） |
| `TypeDesc.field_index` 是名→槽索引 | ❌ **那是「对象」字段的索引**。struct 的 `StructTypeLayout` **只有 `size` + `ref_offsets` + `ref_kinds`，没有字段名、没有逐字段偏移** |

⇒ 符号化 **需要扩格式**（TYPE 段的 struct 块补一张字段表），不是纯编译器改动。
两处更正方向相反：**比否决理由更可行，比自己的零件清单更贵。**

## 依赖实例化的到底是哪几样（实测）

| # | 东西 | 今天 | 符号化能否免掉 |
|---|---|---|---|
| ① | struct 内**字段偏移**（`struct_fget_prim @off` / `struct_fset_prim @off`） | 编译期烘焙 | ✅ 能（需格式：字段表） |
| ② | blob **尺寸**（`StructAllocInstr.Size` / `StructCopyInstr.Size`） | 烘焙 | ✅ 基本已是（`StructAlloc` 已带名字；`StructCopy` 缺名字要补） |
| ③ | **物理调用约定（sret）** | `Layouts.IsBlobStruct(T)` 编译期定 | ❌ **免不掉** |

③ 决定**物理实参个数**：`T = int` 不走 sret，`T = P2`（多字段 struct）走 sret ⇒
同一个泛型方法的物理签名随实例化变。这正是本仓最惨的一类 bug
（`takes 3 physical argument(s), the call passes 2` 那一族：`unify-blob-return-abi`、
`fix-crosspkg-static-sret`、泛型身份 P4 的 ABI 撕裂）。

> 🔴 **因此符号化不是 S2 的替代品**。审计（含我先前给 User 的口述）把 D-2 说成
> 「符号化 vs S2 二选一」是**错的**：符号化只削掉 ① 这一条动因；只做 ① 的话，
> 跨包**仍然需要 S2** 来让消费方按 ABI 重新实例化。

## 测量（动手前做的，不是推断）

### A. 单态化的另一条动因今天有多活跃

`IrGen.InstNeedsOwnBody` 只有两条理由：`InstDiffersFromDef`（① 布局不同）或
`DefHasStaticState`（泛型定义带静态字段）。后者实测：

- 全仓 **269** 个泛型类型声明；
- 体内出现 `static` 一词的 **5** 个；
- 其中真有静态**字段**的 **2** 个 —— **都在 `src/tests/generics/generic_static_per_instantiation.z42`**，
  即专门测这个特性的 fixture；
- **产品代码（编译器 / stdlib / 工具链 / examples）：0**。

⇒ **今天整套机会性单态化几乎完全由 ① 驱动。**
⚠️ 记忆里那个「全仓 0」是 #831 之前量的，**现在是 2**（fixture 带来的）；本条以本次测量为准。

### B. 符号解析在 interp 里的代价

同一个四字段类型，两种访问形态，`--mode interp`，各 5 轮：

| 形态 | 中位 |
|---|---|
| blob 叶子（烘焙立即数） | **0.266s** |
| 类字段（符号化 + 既有 `FieldIC`） | **0.279s** |

**≈ +5%**，在一个「除了字段访问几乎什么都不干」的饱和循环上（每轮 3 次访问），方差 0.002/0.005。

⚠️ 这是**括起上界，不是纯隔离**：两个变量同时变了（存储 arena→堆、解析 烘焙→符号）。
真实的「符号化 struct 叶子 + IC」应当 **≤ 这个数**，因为它保留 arena 存储。
⚠️ 有先例可依：对象字段当年因哈希查找太贵、导致栈分配反被堆分配反超，加 `FieldIC` 后拉平
（密集访问 interp **+5%**）。所以「靠 IC 拉平」有据，但**落地后必须实测，不能假设**。

## ③ 的出路（B 已被 User 排除）

| 出路 | 做法 | 代价 | 状态 |
|---|---|---|---|
| **A** | 今天的路：S2 跨包模板投送，消费方编译期重新实例化 | 模板跨包投送：新格式面 + 新不动点 + 消费方 CU 膨胀 | 可行 |
| **B** | 统一 ABI：泛型体里 T 一律装箱 | 每次带值类型的泛型调用都上堆 | ❌ **User 2026-09-27 排除**（「值类型不能无端封箱」）|
| **B′** | 统一**间接** ABI：仅**型参位置**走隐藏出参指针 / 传地址 | 泛型代码内部多一次间接；**不上堆、不装箱** | **本提案推荐** |
| **C** | 运行期单态化（VM 在加载/JIT 期特化） | 模板随 zpkg 进运行期；**interp 也得做** | 备选 |

**B′ 的细节**：`T` 出现在返回位或按值形参位时，caller 传一个槽/地址：
`T = int` ⇒ callee 把标量写进槽；`T = P2` ⇒ 传 blob 地址、callee 照常按值复制
（值语义本来就要复制这一次）。**全程无堆分配、无装箱**，满足 User 的约束。

## 提案：① + B′ 作为一套架构，分阶段做

**① + B′ 合起来 ⇒ 泛型体只编一份 ⇒ S2 变成不必要**，且单态化只剩
`DefHasStaticState` 这一条（产品代码零命中）。这是比「只做 ①」大得多的收益，
也是把 ① 的格式设计做对的前提 —— 所以两者必须**一起设计**，可以**分阶段落地**。

| 阶段 | 内容 | 格式 |
|---|---|---|
| **P0** | TYPE 段 struct 块补**字段表**（名 / 偏移 / 宽 / kind）；VM 侧 `StructTypeLayout` 带名→偏移索引 | **bump zbc/zpkg minor** |
| **P1** | `StructCopyInstr` 补 `TypeName`（② 收口）；`StructAllocInstr.Size` 降级为纯兜底并在文档中标明 | 同 P0 一次 bump |
| **P2** | `struct_fget_prim` / `struct_fset_prim` 改带 (owner 类型名, 字段路径)；VM 侧解析 + **struct 叶子 IC**；实测 interp 开销 | 同上 |
| **P3** | B′：型参位置统一间接 ABI（`SretAbi.Of` 单入口 —— 正好是审计批 C 点名的那条重构） | 无（ABI 是两侧协议，但不改 wire 结构） |
| **P4** | 拆掉 ① 驱动的单态化：`InstDiffersFromDef` 不再强制 own body；留 `DefHasStaticState` | 无 |

⚠️ **每一阶段都要能独立 GREEN**，且 P2 之前 P0/P1 必须先随一个 nightly 进种子
（`bootstrap-seed.md` 的「support 先行、晚一个 nightly 再 use」）。

## 需要 User 裁决

1. **范围**：按本提案把 ① 与 B′ 作为一套架构推进，还是**只做 ①**（那么跨包仍需 S2，
   收益窄得多）？
2. **B′ vs C**：③ 走「统一间接 ABI」还是「运行期单态化」？
   B′ 不需要模板进运行期、interp 天然支持；C 能拿回零间接但要让 interp 也会特化。
3. **性能门槛**：P2 落地后若实测 interp 开销超过某个线（建议：真实编译负载 **< 2%**、
   字段饱和 micro **< 6%**），是否接受？超了就回退 P2 保留 P0/P1？

## 不做（Out of Scope）

- **不碰对象字段**（`FieldGetInstr` 早就是符号化的，且有 IC）。本提案只动 struct blob 叶子。
- **不动泛型类的独立身份 / 静态分槽**（#829~#835 已完结）。`DefHasStaticState` 那条动因保留。
- **不在本提案内做 JIT 侧特化**（C 路的核心）。若选 C，另开。
- **不删 S2 的 spec 容器**：若 ①+B′ 落地使其不必要，走归档流程注明「被 X 取代」，不静默删。
