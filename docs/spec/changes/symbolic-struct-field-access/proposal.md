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

### ✅ 那条派生优化的可行性已查清（2026-09-28）：**可行**，条件与 P5-B 同款

「给 struct 叶子做 P5-B 同款内联」的核心疑问是「arena 里的 blob 能不能拿到稳定的
`bytes_ptr`」。查清了：

| 事实 | 后果 |
|---|---|
| `StructArena.slots: Vec<StructSlot>`，每个 `StructSlot.bytes: Box<[u8]>` | Vec 扩容时 `StructSlot` **结构体**移动，但每个 `Box<[u8]>` 的**堆缓冲不移动** ⇒ 裸指针**跨 arena 分配仍然有效** |
| 失效只来自 `truncate(base)`（`pop_frame`） | 只在**拥有该槽的帧**退出时发生；被内联的访问活在自己这一帧内 ⇒ 安全（callee 只截断自己 base 以上的槽） |
| arena 是 per-`VmContext`：「owner 线程独占访问；GC 扫描器在 safepoint 读」 | GC 读的是 `refs` 不是 `bytes` ⇒ 无别名；owner 线程持裸指针健全 |

⇒ **成立条件与 P5-B 逐条对应**：① base 寄存器**从不被重写**（`hoist.rs` 的 `written` 判据）；
② blob 属于**当前帧**（`frame_id` —— hoist 时解析一次，正是 `with()` 每次都在做的校验）。

**唯一的额外复杂度**：`struct_field_get_val` 有 **4 种以上 base 形态**
（arena `StructRef` / 堆对象内联字段 / `StackObject` / `StructRefHeap` 数组元素）。
内联快路只能覆盖一种 ⇒ 要像 P5-B 的 `offset < 0` 那样**一次判形态、不符即回落 helper**。

形态（照 `hoist.rs:78+` 与 `translate/object.rs:120+`）：

```
入口块：jit_struct_blob_slot(frame, ctx, base_reg) -> (bytes_ptr, ok)
        // 不抛；ok=false（非 arena blob / 帧不符 / 无布局）⇒ 该访问走 helper
每次访问：ok ? 原生按宽读写 bytes_ptr + <烘焙偏移 或 P2 之后的 fields[i].offset>
             : hr_struct_field_{get,set}_prim(…)
```

⚠️ **与符号化正交**：符号化把「立即数偏移」换成「序号 + 查表」，内联化把「helper 调用」
整条去掉。两者叠加才是终局；**先做哪个都行**。

🔴 **但这是一条带 unsafe 裸指针的 Cranelift 级改动，健全性有真实风险**
（arena 生命周期 + 绕过 Mutex + 形态判定）。**它值得自己一轮**，不该在别的活的尾巴上赶。
本节把地基（上面那张表 + 成立条件 + 形态）记下来，下一轮可以直接执行。

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

- ~~**P1**：`StructCopyInstr` 补 `TypeName`（② 收口）；`StructAllocInstr.Size` 降级为纯兜底。~~
  🔴 **P1 已结案：两半都早就做完了，且都不需要格式 bump**（2026-09-28 动手前读代码核对时发现）。

  | P1 原计划 | 事实 | 依据 |
  |---|---|---|
  | `StructAllocInstr.Size` 降级为纯兜底 | ✅ 早已如此 | `exec_struct.rs::resolve_layout` 先查 `TypeDesc.struct_layout()`，**只在拿不到时**才用编码的 `size` 造 size-only 布局 |
  | `StructCopyInstr` 补 `TypeName` | ✅ **不需要** —— 它已经在用布局 | `StructArena::copy_into(.., _size)` —— 那个参数**下划线前缀、完全没用**；拷的是 `min(src.bytes.len(), dst.bytes.len())`，即两个 arena 槽**各自的 `layout`** |

  根因：两个 blob 都是 arena 槽、**各自带 `layout: Arc<StructTypeLayout>`**，`StructCopy`
  从来不必在指令里再带类型名。那两件事是 `struct-copy-no-alloc` 与 A-use 为**别的理由**顺带做掉的，
  没人回来记一笔 —— 于是阶段表继续挂着一个不存在的、还标着「要 bump」的阶段。

  ⚠️ **本提案内部曾自相矛盾**：本节把「才需要格式 bump」挂在 P2 上，阶段表却写 P1「同 P0 一次 bump」。
  按事实解决（`StructAlloc` 编码**已含** `typeName(u32 池)`；`StructCopy` 编码是 `src(u16) + size(u32)`）。

  ⇒ **实际做掉的是这块地上一处 R3-⑤ 形态的静默吞**：`copy_into` 的「两个 blob 同类型」是文档化的
  不变式，而代码只用 `min` 兜 —— 破坏时**静默截断半个 struct**。编译器编码的 `size` 本是一个
  **免费的独立见证**，此前被直接扔掉。现改为 debug 校验 / release 放行，见
  `docs/spec/changes/check-struct-copy-shape-invariant/`。
- **P2**：`struct_fget_prim` / `struct_fset_prim` 改带 **(owner 类型名, 字段序号)**；
  VM 侧用 P0 的 `field_offset(i)` 解析。**这一步才需要格式 bump**（指令编码变），
  且要按 `bootstrap-seed.md` 先 support、晚一个 nightly 再 use。
  ⭐ P2 落地后**必须实测纯隔离的符号化开销**（当前只有含存储差的上界），对照裁决 #3 的门槛。

## 🔴 P2 的规范冲突（待裁决）+ 普查数据 + A 方案完整规格（2026-09-28）

### ✅ 裁决（User 2026-09-28）：**走 A**

> 「好，请你按照 a 来推进」

⇒ `(root 类型名, 变长索引路径)`；**先不做 memo**（性能后手随时能加，格式块加了就长期背着）。
实施清单见同目录 `tasks.md`。

### 冲突（已裁决，保留记录）

本提案对 P2 的操作数写了**两种互不相容**的答案：

| 位置 | 写的 |
|---|---|
| §「下一步」 | `(owner 类型名, **字段序号**)` |
| 阶段表 P2 行 | `(owner 类型名, **字段路径**)` |

两者给出**不同的 wire 格式**（定长 `u16` vs 变长路径）⇒ 必须裁决后才能实施。

### 普查（10 个 emit 点，全部实测定位）

| 形态 | 站点 | 序号够吗 |
|---|---|---|
| 单层，owner struct 名 + 字段名都在手 | `AccessEmitter:274/281`（`_emitStructFieldSet/Get`）· `RecordSynth:321` · `PatternEmitter:287` | ✅ |
| **累积展平** | `AccessEmitter:303/320`（`_emitBlobFieldGet/Set`，经 `_structChainOffset`）· `AccessEmitter:594/595`（`_copyRegion`，递归）· `OperatorEmitter:408/409`（`_emitLeafEqChecks`，递归） | ❌ 要路径 |

嵌套链恒由**求和展平**（`off(Line,a)+off(P,x)`，可任意深），且**只发射一条**指令。

### 🔴 但决定设计的不是深度，是**编号空间歧义**

编译期有 **2 个偏移编号空间**，运行期有 **4 种 base kind**，而**两个空间在 IR 里无法区分**：

| 空间 | 由谁算 | 运行期 base kind |
|---|---|---|
| ① struct 布局相对 | `StructLayout.FieldByteOffset`（嵌套累加） | `StructRef`（arena blob）· `BoxedStruct`（静态 struct 字段箱）· `StructRefHeap`（`struct[]` 元素，运行期再加 `i*elem_size`） |
| ② composed **对象**布局相对 | `StructLayout.InlineFieldByteOffset` | `Object` · `StackObject` |

站点 3–6 烘出的那个和，**不知道运行期 `Base` 是对象还是 blob 就没有意义**。
今天的正确性靠编译器（`_isInlineStructFieldRoot` / `_isOwnerInlineField`）与运行时
（`exec_struct.rs` 按 `Value` 变体分派）**各自独立地**同意该用哪个空间 —— **指令里一个字都没记**。
这是审计 **R2「判据复制」** 在 struct 路径上的一个实例。

⇒ **P2 的真正价值不是性能**（已实测：符号化边际代价 ≈ helper 内一次查表，helper 调用本来就要付）
**而是把编号空间显式化、可校验**，消掉一整类混用。

### ⇒ 这把 A vs B 判掉了，而且 A 赢

> ⭐ **root 类型名本身就是那个判别器。** 名字解析出来是 **class** ⇒ 第一级索引进
> `composed_object_layout().field_offsets`（它**已经**存在，且注释写明
> 「parallel by index with `TypeDesc::fields`」）；是 **struct** ⇒ 进 `struct_layout().fields`（P0 落的表）。
>
> B 的「展平叶子表」解决的是**深度**问题，**根本没碰空间歧义**，还要再加一个 zbc 块。

### A 方案规格（若获裁决即可实施）

**wire**（zbc 1.46 / zpkg 0.51）：

```
StructFieldGetPrim : op + tag(dst) + dst | base:u16 | root_type:u32(池) | depth:u8 | idx:u16 × depth | kind:u8
StructFieldSetPrim : op + tag(val) + NoReg | base:u16 | root_type:u32(池) | depth:u8 | idx:u16 × depth | kind:u8 | val:u16
```

`depth >= 1`。**不需要新增任何 zbc 块** —— 嵌套字段的类型名由 `TypeDesc.fields[i].type_name`
平行承载（#903 刻意「名/类型名不重复承载」正是为此）。

**解析（VM 侧，`exec_struct.rs`）**：

```
off = 0；cur = root_type
第 1 级：cur 是 class  ⇒ off += composed_object_layout().field_offsets[idx[0]]；cur = fields[idx[0]].type_name
         cur 是 struct ⇒ off += struct_layout().field_offset(idx[0])； cur = fields[idx[0]].type_name
第 2..depth 级：恒走 struct 分支（链已由编译器在非内联处断开 ⇒ 每一节都真内联）
```

- 深度 1（绝对多数）= **1 次下标**，与今天的立即数只差一次下标。
- 深度 d ≥ 2 = d 次下标 + d−1 次类型名注册表查找。⚠️ **这是 A 唯一的代价**，也是要量的那格。
- `StructRefHeap` 的 `i*elem_size` 仍由运行期加，与今天一致。

**校验（A 白送的那一半）**：`root_type` 解析出的 kind 与运行期 `Base` 的 `Value` 变体必须一致
（class ↔ `Object`/`StackObject`，struct ↔ `StructRef`/`BoxedStruct`/`StructRefHeap`），
不一致 ⇒ 报错。今天这条对账**根本无从做起**。

**顺带消掉的 `-1` 危险**：`FieldByteOffset` 查不到返回 **-1**，烘成 u32 立即数即 `4294967295`，
只在运行期以「offset 4294967295 not in type layout」炸出来（注释记着它造成过真 bug
`fix-struct-autoprop-layout-name`）。A 之下烘的是**序号**，查不到就是编译期的事。
⚠️ 注意 `-1` 同时被 `AccessEmitter:540/559`、`FunctionEmitter:72` **当谓词用**
（「这是内联字段吗」）—— 又一处「一个值两种含义」，所以守卫只能加在**烘焙点**，不能让查询函数抛。

### 🔬 `-1` 危险的可达性实测（2026-09-28）—— 结论：不可达，且**并入 P2 做，不单独动刀**

`FieldByteOffset` / `InlineFieldByteOffset` 查不到返回 **-1**，烘成 u32 立即数即 `4294967295`。
做了可达性实验（这是决定「要不要单独开一刀」的前置）。

**方法**：在**烘焙那一刻**（`ZbcInstr.z42` 写 `ByteOff` 处，一处覆盖全部 10 个 emit 点）
插「`ByteOff < 0` 即抛」探针 → 重建 z42.package → 重建编译器 → 用它重编一切。

**结果**：

    25 个 stdlib 库 ×2            全部编过，探针零响
    387 golden 重生 + 748/89/3 e2e  全绿，探针零响
    z42c 24 单测 + 自举不动点 3/3   全绿，探针零响

**两格必需的对照**（否则「零响」毫无意义）：

| 对照 | 结果 |
|---|---|
| 探针条件翻成恒真 + 真有 struct 的用例 | ✅ 响 ⇒ 探针**确实在烘焙路径上** |
| 探针恒真 + 无 struct 的用例 | ✅ 不响 ⇒ 探针**能区分** |
| 强制 `FieldByteOffset` 恒查不到 + 嵌套链用例 | ✅ 抓到 ⇒ 真 miss 能走到守卫 |

🔴 **顺带证实：探针恒真时 25 个 stdlib 库照样全部编过** ⇒ **stdlib 一条
`StructFieldGetPrim`/`SetPrim` 都不发**。所以实验里「stdlib 全绿」**对这件事零信息量**，
有效覆盖只有 e2e 语料。（与既有记录「z42c 自己热路径上几乎不用多字段值 struct」一致。）

⇒ **`-1` 在整个 e2e 语料 + golden + z42c 自举上不可达。** 它**不是活 bug**。
它历史上让项目付过两次代价（`fix-struct-autoprop-layout-name` / `fix-struct-property-getter`），
每次都以「离现场很远的运行期崩」现形 —— 所以值得有守卫，但那是**防御、不是修复**。

#### ⚠️ 一条**未被证明**的推理（不要当事实引用）

我曾判断「`-1` 会被求和掩盖」：站点 3–6 烘的是
`_structChainOffset(...) + FieldByteOffset(...)`，若链偏移为 8 而叶子查不到，
和是 **7** —— 一个看起来合法的错偏移，哨兵消失。

**这条至今只是构造上的推理，实测没能产出它**：把哨兵改回 `-1` 并强制 miss 后，
`struct_nested` / `generic_struct_chain` **照样被守卫抓到**（`ByteOff=-1`）——
因为守卫先在一个深度 1 的站点响了（那里链偏移为 0，和仍是 -1）。
要产出掩盖需要一个「链偏移 ≥1 **且** 叶子恰好 miss」的构造，我没造出来。
⇒ **标记为未证明。** 它不该被当作「已知缺陷」引用。

#### ⇒ 为什么不单独开一刀

曾考虑两种守卫：① 烘焙点查 `< 0`（**抓不到**被掩盖的情形）；
② 放大哨兵到 `-2^24` 让掩盖不可能（正确，但只是补救「哨兵进入了算术」）。
真正对的形态是 **③ 让求和站点根本拿不到哨兵**（查不到就在那一刻抛）。

而 **③ 正是 A 方案自带的性质**：A 之下编译器烘的是**序号**，「查不到」在编译期就是
一次查表失败，**根本没有偏移算术**，没有哨兵可被掩盖。

⇒ 单独做 ③ 要动 `StructLayout` + `AccessEmitter`（4–6 处）+ `RecordSynth` +
`PatternEmitter` + `OperatorEmitter` + 编译器/SDK 重建 + 全量 GREEN + golden 重生，
**而 P2/A 会把它一并消掉**。且它修的是一条**实测不可达**的路。
⇒ **并入 P2 实施，不在 P2 之前单独动刀。**

### 📊 路径长度分布（实测，2026-09-28）—— A 的成本所系

在两个链式站点（`_emitBlobFieldGet` / `_emitBlobFieldSet`）插深度探针
（镜像 `_structChainOffset` 的递归写一个 `_probeDepth`），编译**全 e2e 语料**：

| 路径长度 | root | 次数 | 占比 |
|---|---|---|---|
| **1** | struct | **681** | **85.1%** |
| 2 | struct | 72 | 9.0% |
| 2 | class | 29 | 3.6% |
| 3 | struct | 16 | 2.0% |
| 4 | struct | 2 | 0.25% |
| | | **800** | |

⇒ 深度 1 = **85.1%**，深度 ≥2 = **14.9%**，**最大深度 4**（⇒ `depth:u8` 绰绰有余，
最坏 4 次下标 + 3 次注册表哈希）。

⚠️ **两条必须同时引用的限定**：
① 这是**静态发射数**，**不是动态执行数** —— 一个深度 3 的热循环能压倒 2% 的站点占比，
**动态权重未测**；
② 只覆盖两个链式站点，另外 4 个扁平站点（`AccessEmitter:274/281` · `RecordSynth:321` ·
`PatternEmitter:287`）**全是深度 1** ⇒ 真实深度 1 占比比 85% 更高。

> 📌 此前提案里「深度 1（绝对多数）」是**未测断言**；方向对了，但现在换成实测数。

### 🔴 「解析一次后 memo」不是免费的（C 与 A+memo 共同的失效义务）

`try_fixup_inheritance`（`type_registry.rs:326-431`）**会在类型注册之后改写
`composed_object_layout`** —— 跨包基类在构建期贡献不了布局（own-only, base_shift 0），
要等它解析后重新合成。所以任何「把解析结果缓存住」的方案都背一条**失效义务**，
否则会记住 fixup 前的偏移（本仓栽过多次的「陈旧派生值」形态）。

✅ 好消息：这只影响**类根**路径（实测 **29/800 = 3.6%**）；`struct_layout` **不被 fixup 改写**
⇒ struct 根路径的 memo 是安全的。

⇒ 建议：**A 先不做 memo**。性能后手随时能加；格式块一旦加了就长期背着。

### ⚠️ B 欠一份未做的验证：叶子序号的实例化不变性

「序号实例化不变」是 D-2 用序号取代名字的**全部理由**。但泛型 struct 的布局按**定义**算，
型参字段擦除成**引用叶子**；实例化后该节是否内联**会改变叶子枚举**
（见本文件「链节必须真内联」一节 / `fix-generic-struct-chain-access`）。
⇒ B 的「展平叶子表」其序号是否实例化不变**存疑且未实测**。
A 不受影响（它用的是**声明字段序号**，声明面不随实例化变）。

### 备选（记录在案）

| | 做法 | 为什么不选 |
|---|---|---|
| **B** | 新增「展平叶子表」zbc 块，指令带 `(type_name, leaf_index u16)` | 恒 O(1)，但**不解决空间歧义**，且要加格式块；P0 的表是「逐声明字段」（`Line{P a;P b}` 只有 2 条，叶子有 4 个）⇒ 不能复用 |
| **C** | A + 每站点 IC 记住解析结果 | 深度 ≥2 的占比未量，先付 IC 的复杂度与失效维护不划算；**留作 A 实测超门槛后的后手** |

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
| ~~**P1**~~ | 🔴 **已结案、无剩余工作**：两半均早已完成（见 §「下一步」的更正表）。原标的「同 P0 一次 bump」是错的 —— `StructCopy` 无需带类型名 | **无** |
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
