# Proposal: struct 字段访问符号化（并重估泛型单态化的必要性）

> 类型：`ir` + `vm`（**改 zbc/zpkg 格式** ⇒ 走阶段 1–9 完整流程，需 User 确认后才 IMPL）
> 状态：**方向已批**（User 2026-09-27「d-2 可以」）；**P0 的格式面已由 #903 落地** ⇒ 计划见下方更正
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

## 🔴 补测：JIT 侧（原提案只测了 interp，而那是错的那一侧）

User 问「性能有没有损耗」。原 §B 只有 interp 的 +5%。补测 JIT 之后，结论**方向变了**。

四字段类型、饱和循环（2000 万轮 × 每轮 1 写 2 读）、各 5 轮取中位，同一 release VM：

| 形态 | interp 总时 | jit 总时 |
|---|---|---|
| 纯局部变量（循环基线，无字段访问） | 0.39 | **0.09** |
| baked：struct blob 叶子（偏移是立即数） | 0.89 | 0.64 |
| symbolic：类字段（名字 + `FieldIC`，**未 hoist** ⇒ 走 helper） | 0.92 | 0.75 |
| symbolic：类字段（**已 hoist** ⇒ JIT 的 P5-B **内联**快路） | 0.93 | **0.11** |

减去基线，得「字段访问本身」的成本：

| | baked | symbolic(helper) | symbolic(**内联**) |
|---|---|---|---|
| interp | 0.50 | 0.53（+6%） | — |
| **jit** | **0.55** | **0.66（+20%）** | **0.02** |

### 这组数字推翻的不是一件事，是两件

**① 「烘焙偏移零代价」在 JIT 里也不成立**（原提案只推翻了 C#/IL 那个类比）。
`jit/translate/structs.rs:24-38`：`StructFieldGetPrim` / `StructFieldSetPrim` 在 JIT 里
**恒是 helper 调用** —— 烘焙的 `byte_off` 只是传给 `hr_struct_field_get_prim` 的一个
`iconst` **实参**，不是内联的常量位移。对象字段则有内联快路（P5-B，`hoist.rs:78+`：
对**从不被重写**的对象寄存器把 `(bytes_ptr, offset)` 在入口块解析一次）。

⇒ **符号化的边际代价 = 在一个已经付了函数调用的 helper 内部，把立即数换成一次查表。**

**② 真正的余量根本不在符号化上**：helper 调用 vs 内联 = **0.55 → 0.02**。
今天的烘焙偏移在 JIT 里**几乎什么都没买到**，调用开销全吃掉了。

⚠️ 两条如实说明：
- 0.02 那档里 JIT 能看穿访问（store-to-load 转发 / 寄存器提升），所以「27 倍」**夸大**了纯访存代价；
  但结论方向不受影响 —— 成本在 **helper 边界**上，不在偏移的表示上。
- +20% 这一格**含**「arena vs 堆对象」的存储差（两个变量同时变），**真实的符号化代价更小**。
  要拿到纯隔离数只能等 P2 落地后实测 —— 那正是裁决 #3 的门槛该管的事。

### ⇒ 由此派生一条比 D-2 本身更值钱的优化（独立一刀）

**给 struct blob 叶子做 P5-B 同款内联**（对从不被重写的 blob 句柄，把 arena 槽的 `bytes_ptr`
在入口块解析一次，之后按宽内联读写）。数量级余量摆在那里，且**与符号化正交** ——
符号化只把「立即数偏移」换成「序号 + 查表」，内联化把「helper 调用」整条去掉。
两者叠加才是终局。**记在这里，不并进本刀。**

## 🔴 P0 的设计更正：不要字段名，用**字段序号**；而且格式面已经做完了

原 P0 写「TYPE 段 struct 块补字段表（**名** / 偏移 / 宽 / kind）」+ 「bump minor」。两处更正：

1. **格式面已由 #903 落地**（zbc 1.45）：`ClassDesc.struct_field_table: Box<[StructFieldEntry]>`，
   每项 `{offset, size, kind}`，**按字段位置索引**，`class_flags2` bit0 门控。
   ⇒ **P0 不需要再 bump 任何格式。**
2. **不需要字段名**。指令带**字段序号**即可 —— **序号是实例化不变的，偏移才随实例化变**
   （`Pair<A,B>` 的 `First`/`Second` 永远是 0/1，偏移随 A/B 变）。
   ⇒ 解析 = `table[index].offset`，**O(1) 数组下标，无哈希、无字符串、不需要 IC**。
   ⇒ 比原设计（名字 + IC）**更便宜、wire 更小、且省掉一整套 IC 机制**。

**而且连查表都省了**：arena 的每个 blob 槽本来就携带 `layout: Arc<StructTypeLayout>`
（`struct_arena.rs::StructSlot`）—— helper 手上已经有它，不必按类型名再查一次。

⇒ **P0 缩成纯 VM 侧铺线**：`StructTypeLayout` 加逐字段表（今天只有
`size`/`ref_offsets`/`ref_kinds`），在 `loader/type_registry.rs:119` 从
`desc.struct_field_table` 填上，加一个 `field_offset(i)` 访问器；
`exec_struct.rs::resolve_layout` 的兜底给空表。
**纯附加元数据、零行为变化 ⇒ 零格式 bump、零指纹变更**
（形态同 `unify-object-byte-layout (PR-1)` 的「dormant metadata, not consumed yet」）。

## ✅ P0 已落地（2026-09-27）

纯 VM 侧铺线，**零格式 bump、零指纹变更、零行为变化**：

| 件 | 内容 |
|---|---|
| `StructTypeLayout::fields: Box<[StructFieldLayout]>` | 逐字段 `{offset, size, kind}`，**按字段声明位置索引** |
| 访问器 | `field_offset(i)` / `field_at(i)` / `field_count()` |
| 填充 | `loader/type_registry.rs` 从 `desc.struct_field_table`（zbc 1.45，#903）取 |
| 兜底 | `exec_struct.rs::resolve_layout` 的 size-only 路径给空表 |

⚠️ **`inline_layout` 刻意留空**，并有一条测试钉住：`struct_field_table` 描述的是该 struct
**类型自身**的布局（基准 = blob 起始），而 `inline_layout` 是「该 class 把 struct 内联进对象之后」
的合成布局（基准 = 对象起始）。拿前者填后者会得到**偏移全错**的表，而它一旦被 P2 消费就是
**静默错值** —— 这是「顺手复用」最容易犯的错，所以用测试而不是注释来挡。

### 🔴 P0 的验证测试当场挖出一个真缺陷

这张表在 P0 是**休眠元数据**（无消费方）。我仍然给它写了「从 zbc 到 layout 逐格一致」的测试 ——
结果**两条立刻红了**，根因是：

`loader/type_registry.rs` 的**冷区裁剪条件漏守 `struct_layout`**。相邻两条
（`inline_layout` / `composed_object_layout`）都显式守着，注释还分别写着
「值 struct 总有 own_fields，**但还是显式守一下**」与「**别把正确性押在那个巧合上**」——
第三张表却没跟。于是冷区只剩 struct 布局的类会被**整条裁掉**，`TypeDesc::struct_layout()` 返 `None`。

今天大概不可达（值 struct 总有 `own_fields`），但**P0 之后代价变大**：字段表就住在
`struct_layout` 里，冷区一丢、表跟着丢，而症状要等 **P2 接通后**才以「符号化解析拿不到偏移」
的形式出现 —— 离现场很远。已补守卫。

⇒ **教训：休眠元数据必须在落地时就验证它填对了**，否则它最可能的结局是「填错了也没人知道」。
按「纯附加、零行为变化、不用测」处理，这个缺陷会一直潜伏到 P2。

### GREEN

`cargo test --lib` **1374/0**（新增 3 条）· 5 个 feature 组合全过
（默认 / `interp-only` / `ios` / `android` / `wasm32`+`--features wasm`）·
`test e2e` 746/87/3 · `test compiler` 24/24 + 不动点 3/3 · `test stdlib` 347 组。

### 下一步（P1/P2 未做）

- **P1**：`StructCopyInstr` 补 `TypeName`（② 收口）；`StructAllocInstr.Size` 降级为纯兜底。
- **P2**：`struct_fget_prim` / `struct_fset_prim` 改带 **(owner 类型名, 字段序号)**；
  VM 侧用 P0 的 `field_offset(i)` 解析。**这一步才需要格式 bump**（指令编码变），
  且要按 `bootstrap-seed.md` 先 support、晚一个 nightly 再 use。
  ⭐ P2 落地后**必须实测纯隔离的符号化开销**（当前只有含存储差的上界），对照裁决 #3 的门槛。

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
