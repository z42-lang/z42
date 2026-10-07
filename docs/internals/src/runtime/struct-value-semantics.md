# struct 值语义（内联字节 blob）

> 本页讲**多字段 struct 的真值语义**如何在编译器 + 运行时实现。

## 目标

z42 的 `struct` 是 **C# 真值类型**：赋值 / 传参 / 存容器 = **字段级复制**，不是共享堆对象引用。

```z42
struct Point { public int x; public int y; public Point(int x,int y){this.x=x;this.y=y;} }
var a = new Point(1, 2);
var b = a;      // 值复制
b.x = 99;       // 只改 b
// a 仍是 (1,2)  —— 引用语义下 a.x 会跟着变成 99
```

若 `struct` 与 class 同为 `Value::Object(GcRef)`，`b=a` 克隆句柄会串味。故
**多字段复合 struct** 用内联字节 blob 值语义。

## 布局：字节精确

编译器 `StructLayout`（`z42c.semantics`）为每个 struct 类型算**字节扁平布局**：每个直接字段的
`(byte_offset, size, kind)`、类型总 `size/align`，以及**带种类的引用叶子表**（引用位图）。

- 基元字节精确：`i8/u8/bool=1`、`i16/u16=2`、`i32/u32/f32/char=4`、`i64/u64/f64=8`（`char`=4B
  Unicode 标量）。
- 引用叶子（`string` / object / array 字段）= 16B 托管句柄，进**引用位图**（种类 ArcString / GcRef）。
- `Point{x:int,y:int}` → `size=8, {x:@0, y:@4}`，引用位图空。

## 运行时：per-context 字节 arena + 侧表引用叶子

未装箱 struct 值 = per-`VmContext` **字节 arena**（`interp/struct_arena.rs`）里的一段 blob；寄存器持
`Value::StructRef{idx, frame_id}` 句柄（仿 `StackObject`，LIFO 随帧退出截断，`frame_id` staleness 守卫）。

一个 blob（`StructSlot`）= **两部分**：

| 部分 | 存什么 | 为什么分开 |
|------|--------|-----------|
| `bytes: Box<[u8]>` | 基元叶子，**字节打包**在布局偏移处 | γ 密度：逼近 C# 内存密度 |
| `refs: Box<[Value]>` | 引用叶子（`string`/object/array），作**真 `Value`** | Rust 内存安全：`Arc<str>`/`GcRef` 的裸字节写进 `[u8]` 会漏引用计数（泄漏/double-free）、moving GC 也无法改写；侧表由 `Value` 的 clone/drop 正确托管 |

`refs` 按引用位图（`ref_offsets`）排序；字段访问按 byte offset 映射到 `refs` 槽。

### 四条 IR 指令（zbc opcode 0xC0–0xC3）

| 指令 | 语义 |
|------|------|
| `StructAlloc dst, type_name, size` | 在 arena 分配零初始化 blob，`dst` = StructRef 句柄 |
| `StructCopy dst, src, size` | 复制 blob：字节 memcpy + 逐引用叶子 `Value::clone`（值语义） |
| `StructFieldGetPrim dst, base, (root_type, path), kind` | 读叶子：基元走字节 codec、引用走 `refs` 侧表 |
| `StructFieldSetPrim base, (root_type, path), kind, val` | 原地写叶子（3a lvalue），同上分流 |

`kind` 是运行期 `TypeTag`（`TAG_I32`/`TAG_STR`/…），给字节宽 + 解码 / 或标识引用叶子。

字段 `byte_off` **不**由编译期烘焙为立即数（zbc 1.46 起），见下一节。

⚠️ **两条指令的 `size` 也不是权威**（并非「offset / size … 运行时无需查表」）：

| 指令 | `size` 的地位 | 谁说了算 |
|---|---|---|
| `StructAlloc` | **纯兜底** | `resolve_layout` 先查 `TypeDesc.struct_layout()`；只有**拿不到布局时**才用 `size` 造一个 size-only 布局 |
| `StructCopy` | **完全不用** | `StructArena::copy_into` 拷 `min(src.bytes.len(), dst.bytes.len())`，即两个 arena 槽**各自的 `layout`** |

两个 blob 都是 arena 槽、各自带 `layout`，所以 `StructCopy` 从来不需要在指令里带类型名。
⇒ `StructCopy` 无需 `TypeName`、`StructAlloc.Size` 只是兜底，均不需要格式 bump；
只有 `byte_off` → 字段序号才动编码。

### 符号化的叶子寻址（zbc 1.46）

两条叶子指令携带的不再是**烘焙好的字节偏移**，而是 **`root_type`（池 idx）+ 字段序号路径**
（`depth:u8 + idx:u16 × depth`，`depth >= 1`；扁平 `a.x` 就是 depth 1）。

#### 🔴 动机不是性能

实测：符号化**是中性的**。struct 字段访问在 JIT 里**恒是 helper 调用**
（`jit/translate/structs.rs`），烘焙偏移只是传给 helper 的一个 `iconst` **实参**、不是内联位移
⇒ 符号化的边际代价 = 那次**本来就要付的调用**里多一次查表。
（真正的性能余量在 **helper vs 内联**，那是一条正交的优化，见提案。）

#### ⭐ 真正的回报：消掉一处编号空间歧义

偏移活在**两个互不相容的编号空间**里：

| 空间 | 基准 | 运行期 base kind |
|---|---|---|
| struct 布局相对 | blob 起始 | `StructRef` · `BoxedStruct` · `StructRefHeap` |
| composed 对象布局相对 | 对象起始 | `Object` · `StackObject` |

而**指令里一个字都没记是哪个** —— 正确性靠编译器（`AccessEmitter._isInlineStructFieldRoot`）
与运行时（按 `Value` 变体分派）**各自独立地同意**。那是「判据复制」在 struct
路径上的实例。

⭐ **`root_type` 本身就是判别器**：名字解析成 class ⇒ 第一级索引
`composed_object_layout().field_offsets`；解析成 struct ⇒ 索引 zbc 1.45 那张 `struct_field_table`。
**不需要额外标志位，也不需要新增 zbc 块** —— 路径下一跳的类型名由 `TypeDesc.fields[i].type_tag`
平行承载（1.45 刻意「名/类型名不重复承载」正为此；那条平行性载入期有 `debug_assert` 守着）。

⭐ **A 白送一条对账**：`root_type` 解析出的 kind 必须与运行期 `base` 的
`Value` 变体一致，不一致即报错。

#### 为什么用序号而不是字段名

**序号实例化不变、偏移才变**（`Pair<A,B>` 的 First/Second 永远 0/1）⇒ 索引本身是
**O(1) 下标**，无哈希、无字符串。

⚠️ **不能以「索引是 O(1)」为由说不需要 IC**：拿到布局**之前**要先
`try_lookup_type`（注册表哈希 + 读锁），**每一级一次** ⇒ 成本**由类型查找主导**，
不由索引主导。序号仍是对的选择，但理由是**编号空间歧义**（上一节），不是这条。

嵌套链（`line.a.x`）保留为路径（而非被编译器求和展平成一个立即数）。
实测分布（全 e2e 语料 800 次链式发射）：**深度 1 占 85.1%**、≥2 占 14.9%、**最大 4**。

#### 🔴 别再给这条路做提速 —— 动态权重已实测

上面那个 85.1% 是**静态发射**占比，**推不出**「85.1% 的开销是深度 1」。挂 debug 计数器
跑全 e2e 语料（361 个程序）实测**动态**分布：深度 1 = **80.6%**、2 = 17.4%、3 = 1.8%、
4 = 0.2% —— 方向与静态一致。

**但决定性的是总量：全语料一共只有 2303 次解析。** 按逐深度实测
（深度 1 = 37.3ns / 深度 2 = 144ns），361 个程序的符号化成本**合计 ≈ 85 微秒**。
而**真实编译负载是 0 次** —— debug VM 跑 z42c 编一个文件（12 秒），计数器一次没动。
这与「stdlib 一条 `StructFieldGetPrim` 都不发」是同一件事：**z42c 自己也一条都不执行**。

⇒ 两项候选优化据此撤销：**深度 1 快路**（用 arena 槽自带的 `layout` 省掉
`try_lookup_type`）与**把它扩到深度 ≥2**。快路还额外有害：它在 release 下绕开
`check_base_space`，而那条对账正是符号化的**全部**回报（性能本来就是中性的）。

⚠️ 限定：仓里**没有 struct 密集的基准**（stdlib 的 bench 全不走这条路）⇒ 绝对量只对
树内负载成立。真实 struct 密集用户代码会完全不同，但我们手上没有那样的负载。

#### 解析只有一个实现

`exec_struct::resolve_for_access`（**先对账、后解析**），**interp 与 JIT helper 共用**。
顺序是刻意的：错的编号空间下算出的偏移是个**看起来合法的数**，先解析再对账等于把最有
信息量的诊断让给一个更晚、更远的失败。

#### 🚧 载入期的两道门：平行性 + 覆盖

字段表的两条不变式都在 `loader/type_registry.rs` 落成会响的断言
（政策 **debug 响 / release 放行** —— 只有编译器/写端能违反，不是用户的错）：

| 门 | 要求 | 不设它会怎样 |
|---|---|---|
| **平行性** | 表在场时，长度必须等于 `fields` | 路径沿着错的类型继续解析，产出一个**看起来合法的错偏移** |
| **覆盖**（`check_struct_field_table_coverage`） | blob struct 只要有实例字段，就必须有**非空 struct 块**且**非空字段表** | 每次访问都报 `field index N out of range …（0 field(s)）`，而现场离写端很远 |

⭐⭐⭐ 覆盖门堵的是一个**验证缺口**，不是假想形态。编译器有两条发描述符的路径
（**声明** / **实例化**），若只给声明那条填表，则漏了的那条没有任何东西会响，
而「每条程序都加载了**某个**带表的 struct」这类对照仍会全绿。
覆盖门问的才是「**每个需要的**类型都有表吗」。

判据取**并集**（`class_flags` bit2 **或** 交付了 struct 块）：声明路径按 `Kind == "struct"`
置旗、实例化路径硬写 `Flags = 4`，而 struct 块本身还额外受 `Layouts.IsStructType` 门控 ——
只认一侧就会把门做成恒不响的摆设。前提是**格式严格钉死**（reader 只接受自己写的那个 minor），
所以不存在「产物合法地没带表」这一档。

#### ⚠️ 连带：字段访问依赖类型元数据

`resolve_layout` 的 size-only 兜底（类型没带布局时）之下**无法解析路径**。
实测 289 条 e2e 语料只有 **1** 条走到兜底（`Std.GCHandle`，`TYPE-NOT-LOADED`），
而它**每个成员都是 `[Native]`**、`_slot` 无任何 z42 代码触碰 ⇒ 不受影响。
若将来真发生，给的是「type `X` is not loaded」的**精确报错**而非错偏移。

#### ⭐ 没有 `-1` 哨兵

若 `FieldByteOffset` 查不到返回 `-1`，而站点把多个偏移**求和** ⇒ `-1` 可能被加成正数、
哨兵消失、烘出一个看起来合法的错偏移。符号化后 **序号不参与求和**，
`AccessEmitter._pathAppend` 在 `idx < 0` 时直接抛；编译器侧不再有累加偏移的 walker。

🔒 **`StructCopy` 的「两个 blob 同类型」是不变式，现在有门**
：`copy_into` 先过
`check_copy_invariant`，比对 ① 两侧 `bytes`/`refs` 长度是否相等、② 编译器编码的 `size`
是否等于运行期布局的大小。**debug `bail!` / release 放行**（照 `__box_prim` /
`prim_value_mismatch` 的政策 —— 用户写不出能走到布局偏斜的 z42，只有编译器或加载器 bug 能）。

> 为什么值得设这道门：`min` 保证内存安全，但同时保证**不变式被破坏时没人知道** ——
> 结果是静默截断的半个 struct，腐坏在离现场很远处现形。而编译器编码的 `size` 本来是
> 一个**免费的独立见证**（「编译器与运行期对这个 struct 的大小是否一致」）。布局偏斜
> 若不设门，只会在恰好有一次写跑出尾端时以 `struct field write out of blob bounds` 崩出来。
>
> ⚠️ 验它必须用 **debug VM**：`cfg(debug_assertions)` 的门在 release VM 里不存在，
> `xtask test`（release）对它一个字都没说。

> ⚠️ **`StructCopy` 是「值语义的复制点」，它的成本被所有赋值/传参/返回摊到**——
> `StructArena::copy_into` 用 `split_at_mut` 取两个不相交借用就地复制（src 与 dst 是同一个 `Vec` 的
> 两个下标；若为绕借用检查而 `(s.bytes.to_vec(), s.refs.to_vec())`，就是**每次复制两次无条件堆分配**，
> 且与 struct 大小无关）。
>
> 实测（**两个二进制**交替 A/B，对比有分配的版本）：struct 复制密集的 micro **interp −6.0% / jit −9.8%**；
> 而**真实编译负载（编一个 30 文件的 stdlib 包）量不出差异**（+0.26%，stdev 1.7%）——
> 因为 **z42c 自己热路径上几乎不用多字段值 struct**。**别把这条引用成「编译器提速」**：
> 它提速的是用了值类型的**用户代码**。
>
> 📜 待办：`new S(...)` 仍是 3 次堆分配 + 1 锁 + 1 RwLock + 1 哈希，
> 「值 struct 目前比堆对象更贵」这个与值类型存在理由相反的事实尚未消除。

### GC：arena 是根，无写屏障

字节 arena 每次采集都作 **GC 根**整体重扫（`scan_roots` 遍历每个 blob 的 `refs`，与 `stack_alloc`
arena 同）→ blob 内引用叶子恒被重标记。因此**写引用进 arena blob 不需写屏障**——写屏障只对「引用写进
**堆对象**」必需（堆对象不作根重扫），即 struct 内联进对象/数组（见下），而非局部 struct。

## codegen 翻转

`z42c` 的 `ExprEmitter`/`FunctionEmitter` 对 **blob 值 struct**（`StructLayout.IsBlobStruct`：至少一个字段且布局
`Size>0`，字段可为嵌套 struct）发射上述指令：

- `new P(...)` → `StructAlloc` 句柄 + `call ctor(句柄, args)`；ctor body 的 `this.f = a` 因所属类是
  blob struct 翻转为 `StructFieldSetPrim(句柄, offset, tag, a)`，**原地**填 blob（句柄携创建帧
  `frame_id`，跨 ctor 子帧仍解同一 arena 槽）。
- `P b = a`（非 `new`）→ `StructAlloc b` + `StructCopy(b, a)`；`P b = new P(...)` 直接别名 fresh 句柄。
- `b.x = v` → `StructFieldSetPrim`；`a.x` 读 → `StructFieldGetPrim`。
- `this.x` / 裸字段（struct 方法/ctor 内）→ 同上（`this`=reg0 句柄）。
- **属性 getter 读（`x.Prop`，`Prop` 是 `T Prop { get {...} }`）→ 静态 Call `<Struct>.get_Prop`（传 blob
  句柄，sret-aware），不是 `StructFieldGetPrim`**。判据：成员有 `get_Prop`
  方法即属性（`MemberCollector` 把属性名也登记进 `Fields` 供类型检查，但计算属性无 byte-layout 存储、
  auto-property 存储在 `__prop_Prop` 而非源名——故不能按源名查字段偏移）。**镜像 class 属性 getter 的
  `AccessEmitter._emitMember` 判据（只查 `Methods` 有无 `get_X`，不查 `Fields`），只是 struct 无虚方法 → 走
  静态 Call 传 handle 而非 `VCall`**（VCall on `StructRef` receiver 会崩「expected object, got StructRef」，
  同 struct 实例方法调用）。若把 struct 成员一律当字段发 `StructFieldGetPrim`，属性名查布局落空得
  offset `-1` → 运行期 `struct ref leaf at byte offset 4294967295`。

**优化器完整性**：4 条指令的 def/use 必须录入 `IrOptInfo`（`DstId`/`AddReads`/`ReplaceReads`/`SetDst`）
+ 逃逸分析汇点表——漏 `StructFieldSetPrim` 的 `Val` 读 → DCE 误删喂值的 `const`（实测踩坑）。struct
方法暂不入 inline 允许集（`_isInlinable`），保守不内联。

## 嵌套 struct 字段

`struct Line { P a; P b; }`——字段本身是 struct。布局早已递归展平（嵌套 P 的叶子按偏移平移并入 Line
的字节区间 + 引用位图），故 `line.a.x` 的叶子寻址是 `root_type=Line` + 字段序号路径 `[a, x]`，运行期沿路径解出累积 offset `off(Line,a)+off(P,x)`。

**准入**：`IsBlobStruct` 接受含嵌套 struct 字段的 struct（要求 `FieldCount>=1` 且
`Size>0`——后者兜住自引用 struct 的空布局，见下）。

**叶子读写（3a 原地）**：`line.a.x` / `line.a.x = 3` 沿成员链**累积字段序号路径**，对根 blob 句柄发射
**单条**现有 `StructFieldGetPrim` / `StructFieldSetPrim`——无新指令、无格式 bump。链根解析两遍互补、
不重复发射：`_structChainRoot` 只 Emit 根一次（局部 / `this` reg0 / 拥有者裸 struct 字段），
`_structChainRootType` / `_structChainPath` 纯查布局表得根类型名与序号路径。扁平单层 `a.x` 是其退化情形（depth 1）。

**链节必须真内联**：沿路径内联解析的前提是「这一节的字节就在
容器 blob 里」。判据是 `AccessEmitter._isInlineChainLink`：容器是 blob struct **且** 该字段在容器布局里
`FieldIsStruct`。**只看容器是 blob struct 不够**——泛型 struct 的布局按**定义**算，字段声明类型是 `T`，
擦除成一个**引用叶子**（存另一块 blob 的句柄）；实例化后这一节的静态类型虽是 struct
（`ValueTuple2<int,string>`），存储却不在容器里：

```text
((int,string),int) t            ValueTuple2 布局（按定义 T1,T2）
                                ┌────────────┬────────────┐
                                │ Item1 : T1 │ Item2 : T2 │   两个引用叶子
                                └─────┬──────┴────────────┘
                                      └──► 另一块 blob (int,string)

t.Item1.Item2  错：路径 [Item1, Item2] 按内联解成 off(VT2,Item1)+off(VT2,Item2) 在 t 的 blob 上读 ⇒ 读到 t.Item2（静默错值）
               对：Item1 非内联 ⇒ 断链：先取 t.Item1 的句柄为根，再在它上面读 Item2（偏移从 0 起）
```

若不断链，症状是：`t.Item1.Item2` 读出外层 `Item2`、`t.Item1.Item1` 读出整块内层句柄后装箱崩、
`pp.First.Y = 5` 写进外层别的字段。**先读进局部**（`var x = t.Item1; x.Item2`）总是对的——
单节读正好走「非内联字段 = 取句柄」路径。属性 getter 出现在链中间（无布局存储）也按同一判据断链。
读写共用 `_structChainRoot` / `_structChainPath`，故读、写、复合赋值一致。golden
`src/tests/types/generic_struct_chain.z42`（interp + jit）。

> 命中闸门的实例化拿到自己的布局，
> 型参字段是**真内联字节**而非句柄，故 `pp.First.Y = 5` 写的就是 `pp` 独占的那段字节。
> 闸门外（跨包实例化 / 布局与定义相同者）仍是擦除句柄表示——见下方该条目的「仍未覆盖」。

**整字段复制**：`P p = line.a`（读出）/ `line.a = q`（写入）= 对子 struct 的叶子**逐叶子分解复制**
（递归到真叶子；基元走字节 codec、引用叶子走侧表 `get_ref`/`set_ref`），复用现有 Get/SetPrim，
不引入区间复制指令。值语义：`p` 得独立副本，改 `p.x` 不动 `line.a.x`。

**自引用兜底**：`struct Node { Node next; }` = 无限大小（C# `CS0523`）。`LayoutOf` 的 `_inProgress`
环检测置 `ErrorType` 并返回空布局（`Size==0`）→ `IsBlobStruct` 的 `Size==0` 门拒之 → 退化引用语义
（与今日一致、不崩）。显式 `E0438` 诊断留 follow-up。

## struct 值相等（`==` / `!=`）

blob 值 struct 的 `==` / `!=` 是**字段级值相等**，而非句柄身份。若不脱糖，两操作数持
`Value::StructRef{idx, frame_id}` 句柄，VM 的 `Eq` 比 arena 下标 → 字段完全相同的两个 struct 恒判不等。

**脱糖（纯前端、无新指令、无格式 bump）**：`OperatorEmitter._emitBinary` 检测 `==`/`!=` 两操作数均
`IsBlobStruct` 时，分流到 `_emitStructEquality`——操作数**各求值一次**（`a`/`c` 为 blob 句柄，避免
`f()==g()` 重复求值），`_emitLeafEqChecks` 递归展平叶子（镜像 `_copyRegion`：嵌套 struct 字段递归累积
offset），每个真叶子发射两条现有 `StructFieldGetPrim` + 一条现有 `Eq` + `BrCond` 短路——任一叶子不等
即跳共享 `seq_ne` 失败块。结果 `result` 寄存器在「全等」与「fail」两分支各写 `ConstBool`，end 块读汇合
（镜像三目 `_emitConditional`）。`!=` 只是翻转两分支的 `ConstBool`（全等→false / fail→true）。

**叶子比较语义完全复用现有 `Eq`**——基元→值相等（**float NaN → false**，符合 `==` 运算符语义）；
`string` 叶子→**内容相等**（`Arc<str>` deref 比较）；`object`/`array` 叶子→**引用相等**（符合 z42 对象
`==` 默认 + C# `ValueType.Equals` 对引用字段的行为），不递归深比较堆对象。

```
p1 == p2   ⟹   逐叶子: la=field_get(p1,off,tag); lc=field_get(p2,off,tag); cmp=eq(la,lc)
                        br.cond cmp → 下一叶子块 / seq_ne(失败)
               全叶子相等 → result=const true;  seq_ne → result=const false
```

> 仅拦截 `==`/`!=` 且两侧均 blob struct；`<`/`<=`/`>`/`>=` 对 struct 无序（类型检查器不允许），非 blob
> 操作数（基元/引用类型/单叶子 wrapper）走原 `_emitCompare` 不变。**衔接**：`_emitLeafEqChecks` 确立的
> 逐叶子值相等，就是未来 struct 合成 `Equals`（C# `ValueType.Equals`）/ boxed struct 相等要复用的语义
> （struct→object 装箱见下节）。

## struct→object 装箱 + 身份

值 struct 是 C# 真值类型：不形式继承 `Object`、无 vtable（`z42.core/Object.z42` 契约）。要当 `object`
用（赋给 `object` 变量 / 参数 / 数组、`is`/`as`/`GetType`）靠**装箱**桥接——把帧作用域 blob 拷到堆稳定
表示，而非给值类型加 vtable。

**为什么必须装箱**：`object o = someStruct` 类型合法（`TypeFactsTc._isAssignable` 的「任何类型可赋给 object」
规则），但若无装箱就**裸拷帧作用域 `Value::StructRef` 句柄进 object 槽**——创建帧一退出（arena LIFO
truncate）即 use-after-free。

**堆表示**：`Value::BoxedStruct` 载荷是 `GcRef<ScriptObject>`（共享堆句柄，struct blob 存进对象
`struct_bytes`/`struct_refs`；引用叶子作真 `Value`，GC 扫描）→ 对齐 C# 引用身份，复用 `region_object`，
见下「装箱引用身份」节。**不**给 struct 加 base+vtable，只是把值装进对象容器。

**装箱**（`__box_struct` builtin，复用 `Builtin` opcode → 无格式 bump，同 `__box_prim`）：`TypeChecker.BoxIfNeeded`
对 blob 值 struct 擦除到 `object`/接口插 `BoundBox`；`TypeOpEmitter._emitBox` 发 `__box_struct(structHandle)`；
VM 从 arena slot 拷 `bytes`+clone `refs`+类型名（类型名从 slot 取，无需 class 参数）→ 堆 `BoxedStruct`
（值快照，脱离帧）。

**拆箱**（`(P)o`）：C 风格强转 `(T)x` 绑 `BoundConvert`；`_emitConvert` 见「目标 blob struct ∧ 源非
struct（object/boxed）」→ 发 `AsCast`（复用现有 opcode）。VM `as_cast` 对 `BoxedStruct` 精确类型匹配 →
`unbox_struct`：在**当前帧** arena alloc + 拷 bytes/refs → 返回值 struct `StructRef`（独立副本）。

**身份**：`is_instance` / `as_cast` / `builtin_obj_get_type`（interp + JIT helper 对称）加 `BoxedStruct`
分支——`is P`/`is object` true、`GetType()` → 精确 struct `Type`（type_name 驱动）、`as P` 拆箱 /
`as object`·base·接口 保持 boxed / 不匹配 Null。`o.GetType()` 经 VCall 的 `BoxedStruct` 分支特判到
`builtin_obj_get_type`（保留精确类型，不拆箱 this）。

> **JIT**（见下「JIT 值路径」节）：`jit_as_cast` 对 boxed struct 精确匹配**拆箱**到当前帧
> arena `StructRef`（`frame_id` 惰性分配）；`as object`·base·接口保持 boxed。`jit_is_instance`/
> `jit_vcall`(GetType) 的 `BoxedStruct` 身份分支（无 alloc）与 interp 对称。

## struct 合成对象协议方法

落地 `z42.core/Object.z42` 契约「compiler synthesises value-semantic Equals/GetHashCode/ToString」——boxed
struct 的完整对象协议。unboxed struct 仍无 vtable（这些方法经装箱后的对象协议 / 名字派发，非 vtable）。

- **`Equals(object)`**：**编译器合成** IR 函数 `{FQ}.Equals$1`（`IrGen` 类成员循环末尾注入，与合成 ctor
  同位；用户显式声明则不合成；`build_func_index` 按名注册）。body（`FunctionEmitter.EmitSynthStructEquals`
  → `ExprEmitter.EmitSynthEqualsResult`）= `(other is P) ? leafEq((P)this,(P)other) : false`——**this/other
  均按 boxed 处理、内部 `AsCast` 拆箱到 callee 帧 arena StructRef**（避开 JIT 帧无 frame_id），再复用
  `_emitStructEquality` 逐叶子比较（NaN 精确、嵌套递归、string 内容 / object 引用）。
- **`GetHashCode()`**：**native `__struct_hash_code`**（VM boxed-vcall 臂路由）——对 boxed blob 的 `bytes`
  FNV-1a + 混入引用叶子哈希（string 内容；object/array 叶子弱贡献常量，因 Equals 对引用叶子按引用比较）。
  `& 0x7fffffff` 非负（Dictionary 契约）。同值 → 同 `bytes`/`refs` → 同哈希。
- **`ToString()`**：VM boxed-vcall 臂直接返回**短类型名**（C# `ValueType.ToString` 默认；字段 dump 留后续）。
- **`GetType()`**：`builtin_obj_get_type`（PR2a）。

**VM 派发**（`exec_vcall.rs` + `jit/helpers/vcall.rs` BoxedStruct 臂，interp+JIT 对称）：`GetType`/`GetHashCode`/
`ToString`（arity 0）→ native 特判；否则 prepend `{type_name}.{method}$arity` 候选命中合成/用户方法（this=boxed
值，合成 body 内拆箱），fallback `Std.Object.{method}`。

### 泛型（实例化）值 struct 的值相等边界

合成 `Equals(object)` 的 body 首指令是 `other is P`（`is_instance`）——**只有 `other` 是 `BoxedStruct` 时**
runtime 才认（`is_instance` 无裸 `StructRef` 臂）。因此调用点**必须把 struct 实参装箱到 `object`**，否则
类型测试失败 → 直接走 else 返 `false`（值明明相等）。泛型 record struct（`GRec<int,int>` / `ValueTuple2<int,int>`）
要避开两个独立缺口：

1. **实参装箱须覆盖 `Z42InstantiatedType`**（`TypeChecker.BoxIfNeeded`）：**泛型实例化** struct 的静态类型是
   `Z42InstantiatedType`，若装箱判据只认 `Z42ClassType` → 漏装箱 → 实参裸 `StructRef` 传入 `Equals(object)`。
   做法：unwrap `Z42InstantiatedType.Def` 再判 `IsStruct`，装箱名用 `.Def.Name()`（擦除基名，与合成 `{FQ}.Equals$1`
   注册名 + runtime `type_desc` 一致）。**本地**用户泛型 record struct 由此成立。

2. **跨包同签名歧义**（`OverloadBinder._collectOverloads`）：**imported** record struct（如 stdlib `ValueTuple2`）
   的合成 `Equals$1(object)`（RegKey `Equals$1`）跨包可见，与继承的 `Object.Equals(object)`（RegKey `Equals`）
   **签名相同**。若**按 RegKey** 去重则二者并存 → 同签名两候选 → 重载决议歧义（伴随伪 E0425）→ 解析失败
   → `.Equals` 松绑 `Unknown` **不经 `BoxArgs` 装箱** → 运行期同 ① 症状。故 `_collectOverloads` **按有效签名**
   （简单名 + 形参规范类型，`_overloadSigKey`）跨基链去重——派生类型自己声明/合成的方法**隐藏**基类同签名方法
   （C# method hiding，基链自派生向基遍历、首见者胜）。合成 `Equals$1` 隐藏 `Object.Equals` → 唯一候选 → 正常
   装箱决议。type-based 重载（形参类型不同 → 签名各异）与 `override`（同 RegKey 同签名）行为不变。

   > **本地泛型 struct 不踩 ②**：本地 struct 的合成 `Equals$1` 在类型检查**之后**注入 `IrGen`，类型检查期不可见
   > → `_collectOverloads` 只见继承的 `Object.Equals` → 单候选、无歧义。歧义只在合成方法已随 zpkg 导出、对
   > consumer 可见的**跨包**场景出现。

### 编译器派发：值类型 receiver 的 Object 方法

上面是**运行期**协议；**编译器**须把值类型实例的 Object 方法调用路由到它——若 `a.GetType()`/`a.ToString()`
一律发**静态 `Call {Struct}.{method}`**（blob-struct 分支），Object 方法在 struct 上无函数体 → 运行期
`undefined function`；`E.Red.GetType()` 因 enum 值是裸 i64 → `primitive_class_name(I64)` 会得到错误的 `Std.Int32`。
故 `CallEmitter._emitCall` 的 instance 分支按方法分两路（覆盖 struct + enum，含空/单字段 struct 与 scalar
之外的值类型）：

- **`GetType()`（0 参）→ 折叠 `typeof(静态类型)`**（`_emitValueTypeGetType`）。值类型 **sealed**（无多态）→
  编译期静态类型 == 运行期类型，GetType 结果编译期已知；发 `TypeofInstr(FQN)`（复用 typeof codegen，真句柄），
  **无装箱开销**。判据 `_isStructOrEnumStatic`：`Z42ClassType.IsStruct && !IsScalarValue`（用户 struct；scalar
  基元的 `IsStruct` 亦 true，但 `5.GetType()` 已由运行期正确处理，故排除以免自举字节漂移）或名在 `EnumTypes`
  表（enum 变量 receiver，其 `Z42ClassType.IsStruct=false`）。**enum 成员引用 `E.Red`** 的载体仍是
  `BoundLitInt`（codegen 照发整数字面量），绑定时打的 `EnumTypeName` **origin 标记**让 `CallEmitter`
  折叠回 `typeof(E)`。
  > `E.Red` 的静态类型是 `E`（不是 `long`）；`E.Red == 0` 与「传 int 参」都不成立（双向都要显式 cast）。
  > **载体是 `BoundLitInt`、运行期表示是 i64**，但**身份**是 `E`：擦除到 `object` 时装箱成挂 enum 自己
  > `TypeDesc` 的盒，`GetType()` 因此与这里的编译期折叠答案一致。
  > enum 的完整语义 SoT 见 [`language/enums.md`](https://z42-lang.github.io/z42/reference/language/enums.html)。
- **`ToString`/`Equals`/`GetHashCode`（struct 未自声明时）→ `__box_struct(recv)` 装箱 + VCall**
  （`_emitBoxedStructObjectCall`），命中上面的 runtime 装箱-struct 协议。**自声明**（record 合成 / 用户覆写，
  `EmitContext.ChainHasMethod` 命中）仍走各自静态 `Call`——保 record 的 `ToString`（`R { A = 1, B = 2 }`）/
  值 `Equals` 与用户 `ToString` 不被装箱短名拦截。

> **struct 导出方法表仍不注入 Object 四方法**（`ClassExtractor`）——保 zbc TYPE 段元数据 /
> 自举字节不变。这只决定 `typeof(struct).GetMethods()` **是否列出** Object 四方法（反射枚举完备性维度，收益边际、
> blast-radius 大，留后续），**不影响调用**——调用由上面的 `CallEmitter` 路由解决。（C# struct 经 `ValueType:Object`
> 有这些方法，并非 ExcludeFromImplicitObject。）

**定案**：`==`/`!=` on `object`-typed boxed struct = **值相等**（`Value::BoxedStruct` `PartialEq`：
type_name∧bytes∧refs），延续对 struct `==` 的值语义（`PartialEq` 先 `ptr_eq`（同盒短路，避免
双 borrow 死锁），否则经共享对象比 `struct_bytes`/`struct_refs`——值相等语义不变）；`.Equals()` = 合成叶子方法
（float `Eq` → NaN≠NaN 精确）。**边角**：float NaN
`==` 按位判等 vs `.Equals` 浮点== → 极少含 NaN 的 struct 二者微差（pre-1.0 标注，要完全一致须让 `==` 也走
vcall Equals，代价不值）。

**待办**：
- ToString 字段 dump；`IEquatable.Equals(P)` typed 重载；反射 GetMethods 报告合成方法（SIGS 元数据，
  可选、动 SIGS 有自举字节稳定性风险，留后续）。

## struct 泛型容器装箱

`Dictionary<P,V>` / `List<P>` / `HashSet<P>` 存 struct 键/值/元素——**泛型边界装箱**（非字节内联；密度内联见下节）。格式中立：复用 `__box_struct`（存）+ `AsCast`（取）+ `as_cast` 的 StructRef 恒等臂，容器 backing
（`TKey[]/T[]`，运行期擦除）与 ABI 不变。

**问题**：泛型路径把 struct 实参当**未装箱 `Z42GenericParamType`（K/T）** 传入——`BoxIfNeeded` 只对
`object`/接口目标装箱，type-param 不装箱 → 裸 `StructRef`（帧作用域 arena 句柄）流入容器：`Dictionary.Set`
的 `key.GetHashCode()` = 对 StructRef 的 VCall → 崩；`keys[slot]=key` 存进堆数组 → 帧退出 use-after-free。

**装箱（存入）**：`TypeChecker.BoxIfNeeded` 的 `erasesS` 谓词加 `|| (target is Z42GenericParamType)`——
覆盖所有走 `BoxArgs` 的方法实参（`List.Add`/`Dictionary.Set`/`Contains`…）。`d[key]=v` 的 indexer-set
（`AssignTyper._bindAssign` 手搭、**绕过 BoxArgs**）与 `d[key]` 读的 get_Item 索引实参（`ExprTyper._bindIndex`）单独按
`set_Item`/`get_Item` 的 `ParamTypes` 装箱。→ 容器存 `Value::BoxedStruct`（堆稳定），`GetHashCode`/`Equals`
走上面的 boxed-vcall 臂。

**拆箱（取出）**：取回到具体 struct 类型需拆回值 struct。`TypeChecker.StructUnboxTarget` 判「泛型返回
（get_Item / 方法返回 T）subst 后是否 blob struct」，是则调用点把结果包 `BoundConvert(→P)`，复用
`TypeOpEmitter._emitConvert` 的 `AsCast` 拆箱臂。`foreach (P p in list)` 在 `FunctionEmitter` 对元素发 `AsCast`。

**`as_cast` 的 StructRef 恒等臂**（关键统一点）：泛型容器迭代/取值统一走 `AsCast`，但元素运行期可能是
`BoxedStruct`（泛型容器，Add/set 装箱）**或**已是 `StructRef`（普通 `P[]`）——静态同为 `P[]` 不可辨。故 VM
`as_cast`/`jit_as_cast` 加 **StructRef 源 → 原样返回**（已是值 struct，`as P` 恒等；编译器仅在静态类型即该
struct 处发此 AsCast），使两种运行期种类统一：`BoxedStruct`→拆箱 / `StructRef`→恒等。取出的 struct 是拷到
当前帧 arena 的**独立副本**（值语义：改它不动容器）。

泛型容器里是 boxed 堆对象，非内联字节；真**字节内联**进堆对象字段 / `struct[]` backing（密度 + FFI）+ 写屏障见下节。

## struct 内联进堆对象字段 + struct[] backing

泛型容器靠**装箱**（每元素一个堆 `BoxedStruct`，无密度）；而 struct 值也可**字节内联**进
**堆对象字段**（`class C { Point pt; }`）与 **`Point[]`**——真密度（基元字节精确打包，逼近 C# 布局）+
FFI 零 marshaling + 零 per-field 堆分配。这是 struct 值语义功能面的闭合项。

### 决策：基元内联 + 引用叶子侧表（非裸内联）

内联 struct 的引用叶子（string/object/array）怎么存，是核心设计分叉。选定**侧表**方案：
- **基元叶子**按字节精确**打包进对象字节区** `ScriptObject::struct_bytes`（密度/FFI 收益全在此）；
- **引用叶子**走对象的 `struct_refs: Box<[Value]>` **侧表**（真 `Value`），**不裸内联** 16B 句柄进字节区。

> **为什么不裸内联引用叶子（否决）**：GC 访问协议是 `visitor(&Value)`，`Value` enum 远大于 16B 且带
> 判别式——无法只存 16B 句柄再还原完整 `&Value`；`Arc<str>` 裸字节要手工 `ManuallyDrop`/`Arc::from_raw`
> 管引用计数，漏一处即 double-free。而引用叶子无论放侧表还是字节区**都是 16B 句柄，密度无差**。故侧表既拿
> 全部密度收益、又换回内存安全 + 与 arena `StructSlot`/`BoxedStruct` 完全同构（`StructCopy` 无转码）。

### 对象内联表示与访问（路线 α）

`ScriptObject` 加 `struct_bytes`（内联字段基元打包）+ `struct_refs`（引用叶子侧表）。`TypeDescCold.inline_layout`
= 类的**合成内联布局**（对象相对字节区 size + 引用位图，复用 `StructTypeLayout`——对象内联区 = 字节 blob +
引用侧表，与 struct 同构）。alloc 时零初始化（= struct 默认值）。**内联字段仍保留一个 dead slot**（不重排
`field_index`/slots，最简；真数据只在 struct_bytes，dead slot 恒 Null；1 slot/字段为小浪费）。

访问复用现有 `StructFieldGetPrim/SetPrim`（0xC0–0xC3，**无新 opcode**）——`base` 从「仅 arena StructRef」扩到
「也可为堆 `Value::Object`」：叶子基元读写 `obj.struct_bytes[byte_off]`（叶子的对象相对复合 offset
`off_field + off_leaf` 由 `root_type`（类）+ 字段序号路径解出）；引用叶子读写 `obj.struct_refs[inline_layout.ref_index(byte_off)]`。

### GC：扫描 + 写屏障

内联 struct 的引用叶子落在堆对象字节区内，**不再是独立 GC 根重扫**（arena 每采集重扫故无屏障，见上「GC：arena 是根，无写屏障」）——堆里的内联叶子需两件事：
- **扫描**（mark 追踪）：`scan_object_refs`/`trace_children` 的 `Object` 臂遍历 `obj.struct_refs`（与
  `BoxedStruct.refs` 一行同构，零 unsafe）——侧表让这平凡复用 `visitor(&Value)`；
- **写屏障**（并发/分代正确性）：写内联引用叶子 = 写 `struct_refs[k]` 一个 `Value` 槽 → 复用现有
  `write_barrier_field(owner, k, new)`（STW 默认 no-op）。**无新屏障机制**——这是侧表相对裸内联最大的工程简化。

### 格式 wire：内联字段表（zbc 1.32 / zpkg 0.37）

类描述符尾部加**合成内联布局块**（`CLASS_FLAG_HAS_INLINE_STRUCT` bit7=0x80 gated，紧随 struct 块）：
`size:u32 + ref_count:u16 + (byte_off:u32, kind:u8)×n`——同 struct 块 shape（reader 复用 `StructLayoutDesc`）。
writer 侧 `ClassDescBuilder` 用 `StructLayout.InlineLayoutOf`（`BuildFromSymbols` 为每个非-struct class 预计算，
**writer 与 codegen 同源取对象相对 offset → 一致**）。叶子访问指令携带 `root_type` + 字段序号路径，运行期解出 byte offset，不入块。

### codegen（对象字段）

`AccessEmitter` 谓词 `_isInlineStructFieldRoot`（字段类型 `IsBlobStruct` ∧ 容器是 class）+ `_isOwnerInlineField`
（class 方法内裸 `pt`=this.pt，靠 `EmitContext.OwnerClassName`）。`_structChainRoot`/`_structChainRootType`/`_structChainPath` 扩两
根（内联字段根 = 对象句柄 / reg0）→ 叶子 `c.pt.x`/`pt.x` 复用嵌套链发 `StructFieldGetPrim/SetPrim`；整字段读
（`Point p = c.pt`）→ `StructAlloc` + `_copyRegion` 拷出（值副本）；整字段写（`c.pt = q`）→ `_copyRegion` 拷入。

### `struct[]` 字节 backing

`ArrayBacking::StructBytes{elem_size, bytes, refs, layout}`（C# inline `struct[]`：元素基元紧凑
`bytes[len*elem_size]` + 引用叶子并行 `refs[len*ref_count]`）。`arr[i]` 元素 offset 运行期定 → 需**堆 base 句柄**
`Value::StructRefHeap{idx, frame_id}`（指向 `VmContext::transient_arena` 里的 `StructArrayElem{arr, index}`；arena `StructRef` 热路径不动；仅数组需句柄）。GC：
`ArrayObj::gc_refs()` 统一 `Boxed ∪ StructBytes.refs` 供扫描；元素引用叶子写触发 `write_barrier_array_elem`。

- **创建**：`array_new`/`array_new_lit` 对 **blob 值 struct 元素**（`try_struct_backed`：`TypeDesc.fields≥2` +
  `struct_layout`，匹配编译期 `IsBlobStruct`）造 `StructBytes` backing（`ArrayObj::struct_backed`，经
  `Heap::alloc_array_obj` region-alloc 保 backing）；字面量经 `pack_struct_elem` 把各元素（`StructRef` 经 arena /
  `BoxedStruct`）字节+引用叶子拷进元素槽。
  - **泛型值 struct 数组的类型查找按擦除裸名**：`array_new` 携带的元素类型名
    是**非擦除全名**（`Kv<string, int>`，供 `arr.GetType().GetElementType()` 反射），但泛型是**类型擦除**——
    一个泛型定义只注册**裸名** `Kv` 的单一 `TypeDesc`。故 `try_struct_backed` 查类型前须 `element_type.split('<')`
    剥泛型实参、用裸名 `try_lookup_type`，否则用全名查会 miss → 数组退化成**引用背衬**（元素 `Null`）→
    `StructFieldSetPrim` 崩 `expected StructRef, got Null`（`KeyValuePair<K,V>[]` = `Dictionary.Entries()` 的返回，
    正是此路径）。全名仍传给 `struct_backed` 以保元素反射。该处理覆盖 interp/jit/数组字面量三条创建路径。
- **取值**：`array_get` 对 `StructBytes` backing 产 `StructRefHeap` 元素句柄（有 array `GcRef`，替代 `get_boxed`）。
- **codegen（AccessEmitter）**：`_emitArrayElemHandle`（ArrayGet 直发句柄不拷贝）；`_emitIndex` 对 struct[] 出
  `StructAlloc`+`_copyRegion` 拷出（standalone `arr[i]` 值副本）；`_structChainRoot` 对 BoundIndex struct[] 根=句柄
  （`arr[i].x` 原地叶子读写复用嵌套链发 `StructFieldGetPrim/SetPrim`）；`arr[i] = p` 走句柄+`_copyRegion` 拷入。**无新 opcode、格式中立。**

### 覆盖范围

- **对象内联 struct 字段**（`class C { Point pt; }`）：默认零初始化 / `c.pt.x` 叶子读写 / 整字段拷入拷出值语义
  独立 / 方法内裸字段 / string 引用叶子内联 / 多对象独立——golden `struct_heap_inline.z42` 端到端验证。
- **`struct[]` 值类型数组**（`Point[]`）：默认零初始化 / `arr[i].x` 叶子读写 / 整元素拷出拷入值语义独立 /
  元素独立 / `new Point[]{}` 字面量 / string 引用叶子内联——golden `struct_array.z42` 端到端验证。格式中立。
- **class 实例方法返回 struct**（`Point GetPt(){ return pt; }`）：`_emitCall` instance 分支返回 blob struct 时
  三派发路径（devirt 直 Call / DepIndex Call / VCall fallback）均追加返回 blob 句柄作**末尾隐藏 sret 实参** + void
  dst；object VCall 按 vtable slot(方法名) 派发 arity 不入解析键 → 不破派发。golden `struct_heap_inline.z42`（GetPt）验，格式中立。
- **foreach over struct[]**（`foreach(P p in arr)`）：foreach 数组路径对值 struct 循环变量发的 AsCast，runtime
  `as_cast` 加 `StructRefHeap` 臂 → `copy_array_elem_out` 把元素拷出到当前帧 arena StructRef（值副本，循环变量非
  别名进数组）。runtime-only、格式中立，golden `struct_array.z42` foreach 段验（含 `foreach{e.x=999}` 不动数组）。
- **装箱引用身份 + struct 字段反射**：`BoxedStruct` 改共享 `ScriptObject`
  + `FieldInfo.GetValue/SetValue` 反射装箱 struct 字段（见下「装箱引用身份 + struct 字段反射」节）。
- **对象内联 struct 字段反射**：反射 `GetValue/SetValue` 读写
  `class C { Point pt; }` 的内联 struct 字段（复刻类级内联布局，见下同名节）。

## 基元装箱统一到 `BoxedStruct`

**为什么统一**：struct 装箱与基元装箱若是两套不对称模型（struct = `Value::BoxedStruct(GcRef<ScriptObject>)`，
GC 管理 + 引用身份；基元 = 轻量 `Box`、无引用身份、非 GC 管理），则 `is`/`as`/`GetType`/`value_to_str`/GC visit/equality/
反射/vcall 等 ~20 处 helper 要双写；且 `object o=5; object p=5; ReferenceEquals(o,p)` 在 C# 是 `false`（两个不同盒），
无引用身份就是语义偏差。

**唯一装箱模型**：`__box_prim`（`corelib/convert.rs`）产堆 `ScriptObject` + `Value::BoxedStruct`，
与 struct 装箱同一路径 → 每次装箱 alloc 新盒（C# 引用身份），复用 `region_object` 全套 GC。无 `Value::Boxed`
变体（判别号 13 留空，`#[repr(C,u8)]` 显式判别 14-18，JIT 原始布局不受影响）。

**标量存储（与 struct 装箱完全同构，零格式 bump）**：`__box_prim` 只装**整数**（bool/char/double/
string 各留自己的 `Value` variant，不经此路）。整数标量的 **LE 字节存进盒的 `struct_bytes`**
（宽度按 wrapper 名查 `well_known_names::int_wrapper_scalar_spec` → `Std.Int32`→4 / `Std.Byte`→1 /
`Std.Int64`→8…），`slots`/`struct_refs` 空。**关键**：基元 wrapper（`Std.Int32` 等）是 phantom struct
（零字段 / layout size 0），故装箱走**专用 alloc** `MagrGC::alloc_boxed_prim`（调用方按标量宽度定 `struct_bytes`
尺寸），**不**走 `type_desc.inline_regions()`（那会给零字段 wrapper 空 `struct_bytes`）→ wrapper 的 emitted
struct_layout / zbc TYPE section 完全不动，**无格式 bump**。

**拆箱**：`ScriptObject::boxed_prim_i64()`（`metadata/types/object.rs`）按 `type_desc.name` 的
`(width, signed)` 从 `struct_bytes` 前 `width` 字节还原 i64——signed narrow 符号扩展、unsigned 零扩展；
非整数盒（多字段 struct 装箱 / 名不在整数 wrapper 表）返 `None`。**这个 `Some`/`None` 是「基元盒 vs struct 盒」
的分流判据**：所有原 `Value::Boxed` 消费点收敛成单一 `BoxedStruct` 臂，需要区分时用 `boxed_prim_i64()`：

| 消费点 | 基元盒（`boxed_prim_i64()==Some(n)`） | struct 盒（`None`） |
|--------|-------------------------------------|---------------------|
| `is`/`as`（`exec_object` + `jit/helpers/object`） | 精确 wrapper 命中 → 拆回裸标量 `I64(n)` | 精确 struct 命中 → 拷 blob 回 arena `StructRef` |
| `(T)o` 数值转换（`convert_value`） | 拆回 `I64(n)` 再 convert | 不拦截（struct 不走数值转换） |
| VCall（`exec_vcall` + `jit/helpers/vcall`） | `this = I64(n)` 交基元 struct 方法体 | `this = 盒` 交合成对象协议方法 |
| `value_to_str` | 标量字符串（`WriteLine(object)` 打印 `5` 非 `Std.Int32{...}`） | `类型名{...}` 占位 |
| `GetType` | `type_desc.name` = 精确 wrapper（`Int64` 不丢宽度） | `type_desc.name` = struct 类型 |
| equality（`Value::eq`） | 装箱整数 vs 裸整数透明拆箱比较；盒 vs 盒按 `struct_bytes` | 盒 vs 盒按类型名 + `struct_bytes` + `struct_refs` |

**格式中立**：`__box_prim` 发射点 / 装箱路由不依赖盒的运行期表示 → zbc/zpkg 格式不变。golden `types/boxed_primitive_is_as.z42`（Int64/Byte/Int32 跨宽度
is/as/GetType）+ `types/box_unbox.z42`（`(int)o` 拆箱 + `WriteLine` 装箱打印）验端到端。

### 为什么装箱必须带精确类型标记

`Value` 的内联 payload 只有 8 字节，塞不下「宽度 tag + i64」；而强类型的 `is` / `as` / `GetType`
要求装箱值**保留精确的基元类型**——`object x = 5; x is long` 必须为 **false**，`object l = 9L;
l is long` 必须为 **true**。裸 `Value::I64` 两者无从区分（未过 object 边界的裸整数走
`prim_isa` 松匹配，那是另一条路）。所以装箱一定要落到一个带 `type_desc` 的堆对象上，而不是
「codegen no-op / 直接把裸值塞进 object 槽」。

代价被限制在装箱点：算术与方法体永远拿拆箱后的标量，热路径零影响。收益是基元 wrapper 本身就是
真 struct（`struct Int32 : IComparable<int>`），带 type_desc 的盒经对象路径**免费获得**
is-a / `GetType` / vcall。

### 编译期：谁装箱、在哪装箱

装箱由 `TypeChecker.BoxIfNeeded(value, target)`（`TypeChecker.z42`）在每个协变点判定，命中则
包 `BoundBox`，codegen 由 `TypeOpEmitter._emitBox` 降成
`const.str "Std.Int64"; builtin __box_prim %dst,%val,%cls`（`_emitBoxPrim`）。
**复用既有 Builtin opcode，不新增 IR 指令 ⇒ 不 bump 格式。** 拆箱复用 `AsCast`：`BoxedStruct` →
基元时 is-a 校验后返还标量。

**哪些源类型真的装箱**（`BoxIfNeeded` 的分支序即判据）：

| 源静态类型 | 目标是 `object` / 接口时 | 说明 |
|---|---|---|
| **整数族**（`int`/`long`/`byte`/`short`/`uint`/… ）| ✅ `__box_prim`，`class` = 精确 wrapper | 标量 LE 字节进盒的 `struct_bytes` |
| **`enum`** | ✅ `__box_prim`，`class` = **enum 自身**（非 `Std.Int32`）| enum 装箱带自己的 `TypeDesc` |
| `bool` / `char` / `float` / `double` | ❌ 不装箱 | 各有自己的 `Value` 变体，自带身份 |
| `string` | ❌ 不装箱 | 引用类型 |
| **值 struct**（含泛型实例化 struct）| ✅ `__box_struct`，目标还包括**泛型形参** | 非 blob（零字段 / 自引用空布局 / scalar wrapper）struct 的 `BoundBox` 在 codegen 退化为透传 |
| class / record / 数组 / 接口 | ❌ 恒等上转 | 本就是带 TypeDesc 的 GcRef |

> ⚠️ 「基元装箱」在 z42 里**只覆盖整数与 enum**。非整数标量不进盒这件事决定了：
> `((object)1.5).GetType()` 答 `Double` 不是靠盒，而是靠 `vcall_resolve` 阶梯第 3 级的
> `primitive_class_name`（见[对象协议派发](object-protocol-dispatch.md)）；
> 而 `ReferenceEquals` 式的盒身份只对整数 / enum / struct 成立。

**插入点**（协变点逐处插，缺一处就是一次静默丢类型）：

| 插入点 | 位置 |
|---|---|
| var-decl（`object o = 5L;`）| `StmtBinder.z42` |
| **再赋值**（`o = 5L;`，非声明）| `AssignTyper.z42` |
| return（返回类型 object/接口）| `StmtBinder.z42` |
| 数组字面量 `object[]` 的元素 | `ExprTyper._bindArrayInit`、集合字面量 `CollectionTyper.z42` |
| call-arg（形参 object/接口）| `TypeChecker.BoxArgs`，由 `OverloadBinder._withDefaults` 单点汇聚 |
| `params object[]` 尾包元素 | `OverloadBinder._withParamsExpansion`逐元素按**元素类型**装箱 |
| 索引器 set（`d[k] = v`）| `AssignTyper.z42` —— 手搭 `BoundCall`，**绕过 `BoxArgs`**，就地补装 |
| 泛型方法实参 | `ExprTyper.z42` —— 同上，手搭调用绕过 `BoxArgs`，逐位补装 |
| record 合成 `GetHashCode` 的字段 | `Lowering/RecordSynth.z42` 直接发 `__box_prim` |

> 最后三行是同一个教训：**任何手搭 `BoundCall` 而不经 `_withDefaults` 的路径都会漏装箱**；
> 普通再赋值也需要自己的装箱点（`AssignTyper.z42`）。漏装箱的症状是安静的：裸标量流进
> `object` 槽，`is` / `GetType` 答错，或者裸 `StructRef` 句柄逃出创建帧后 use-after-free。

**拆箱消歧**：`(int)x` 有两义——① `x` 是 object / 接口 → 拆箱（`AsCast`）；② `x` 是数值 → 数值窄化
（`Convert`）。按 `x.Type()` 分派，绝大多数既有 cast 属 ②，不受影响。分类器口径见
reference 的[类型转换](https://z42-lang.github.io/z42/reference/language/conversions.html)。

**call-arg 与基元 native 的交互**：call-arg 装箱会把整数实参装成 object（`Assert.Equal(object,object)`
这类），而基元 struct 的 native 方法按裸 long 读参 —— `arg_i64`（`corelib/convert.rs` 取参助手）
**透明拆箱**基元盒，一处修覆盖全部整数 native。

## JIT 值路径

JIT 遇 struct 值指令（`StructAlloc`/`StructCopy`/`StructFieldGetPrim`/`StructFieldSetPrim`）若 `bail!`→**整函数回退 interp**，
用到 struct 的函数就拿不到任何 JIT 收益（连周边算术/循环/调用一起退回）。故用 **helper 桥接**接通 JIT 值路径。

### 机制：helper 桥接（非原生内联）

每条 struct 指令 emit 成对一个 Rust helper 的 `call`（`jit_struct_alloc`/`_copy`/`_field_get_prim`/
`_field_set_prim`，`jit/helpers/struct_ops.rs`），helper 操作与 interp **同一个** per-context
`struct_arena`。**关键复用**：helper 只是薄封装读写 `JitFrame.regs`，真正的 arena 操作 + 字节编解码 +
base 三态分派（arena `StructRef` / 堆 `Object` 内联字段 / `StructRefHeap` 数组元素）全部调 interp
`exec_struct` 抽出的 frame 无关 `*_val` 核心（`struct_alloc_val`/`struct_copy_val`/`struct_field_get_val`/
`struct_field_set_val`）——interp 与 JIT **逐字节等价**，无逻辑分裂。

收益：**struct 指令本身 ≈interp 速度**（一次 native→Rust call + arena 锁），但**周边算术/控制流/调用为
native**——含 struct 的函数不再整体退回 interp。这是该方案的主要价值。**原生内联字节访问**（FieldGet/Set
直接 emit cranelift load/store 到 arena 字节，跳过 helper call）边际提速有限却引入裸指针 × 移动 GC ×
realloc 健全性风险，**待办**：待 benchmark 证明某热路径卡在 helper 边界再做。

### frame_id：惰性分配 + OSR 继承

`StructRef{idx, frame_id}` 的 `frame_id` 供共享 arena 的悬垂 guard（LIFO base 已由现有
`push_frame`/`pop_frame` stamp `struct_base` 管理）。`JitFrame` 加 `frame_id: u32`（默认 `0`），采用
**纯惰性**——只在**分配型** helper（`jit_struct_alloc` / `jit_as_cast` 拆箱 / `copy_array_elem_out`）里，
若 `frame_id==0` 则从 `next_frame_id()`（与 interp 帧共用的 id 来源；interp 帧同样惰性取号）取真值。deref（`FieldGet`/
`Copy`）用的是句柄里**内嵌**的 frame_id（非当前帧），故只有 alloc 路径需要——一处惰性覆盖入口 + 所有嵌套
callee，零 per-site 改动。**OSR 例外**：`from_interp_regs` 续接同一逻辑活动记录，须 eager **继承** interp
帧 frame_id（OSR 前已分配的 struct 局部交接后仍要能 deref）。

### 补齐：struct[] 数组 + 装箱拆箱

- **`jit_array_new`/`jit_array_new_lit`** 对 value-struct 元素造 `ArrayBacking::StructBytes` backing（复用
  interp `try_struct_backed`/`pack_struct_elem`），**`jit_array_get`** 对 StructBytes backing 产
  `StructRefHeap` 句柄（非 `get_boxed` BoxedStruct 快照）——接通 JIT 下 `new Point[]` + `arr[i].x` +
  `foreach(P p in arr)`（元素拷出走 `copy_array_elem_out`）。
- **`jit_as_cast`** 对 `BoxedStruct` 精确匹配**拆箱**到当前帧 arena `StructRef`（`(Point)o`），镜像 interp
  `unbox_struct`；`as object`·base·接口保持 boxed。

### 验证

golden `struct_jit.z42`（本地值语义 / 传参 sret / 嵌套 / string 叶子 / struct[] index+foreach / 装箱拆箱）
在 `--mode jit` EXIT=0，且与 interp 模式输出一致；既有 `struct*.z42` golden 在 `--mode jit` 全过（真走 JIT struct 路径）。**格式中立。**

## 跨包 struct 值语义

包 B `import` 包 A 定义的 struct 后按值语义工作（构造/字段/方法/传参 copy-in/返回/嵌套/值独立），与本地
struct 一致。

### 机制：跨包分类 imported struct（单点，复用既有 `HasBase` 编码，无格式 bump）

**wire 层面什么都不缺**：zpkg TYPE 段已携带 struct 标志（`Flags` bit2）+ 字段名/类型 + 完整字节布局
（`StructSize` + 引用位图），消费方 `ZbcReader` 也已解码进 `IrClassDesc`。要做的只是**编译器内部把「这是
struct」传过跨包这一跳**——`ImportedSymbolLoader` 造 imported `Z42ClassType` 时若不设 `IsStruct`（默认
false）→ imported struct 会被当引用类型。

**做法（单行）**：`nct.IsStruct = !cl.HasBase`。生产方 `ExportedTypeExtractor` 与消费方重建
`TsigReconcile._rebuildClass` 均把 struct-ness 编码进 `ExportedClassZ.HasBase`（`hasBase = !isStruct`——
非-struct class 恒 `HasBase=true`、struct 恒 `false`），故 `!cl.HasBase` **精确等价** isStruct（读同一份已
编码的权威 struct-ness，非启发式）。

**为何不加显式 `IsStruct` 字段**（bootstrap 约束）：`ExportedClassZ` 在 z42.package（stdlib 库），
z42c.semantics 依赖它作跨包 API。给它加新 `IsStruct` 字段并在 z42c 源立即用 → 上一 nightly 种子的 z42.package 无
此字段 → `xtask test compiler bootstrap` 编当前 z42c 源报 `E0401: no field IsStruct`（stdlib API 面越界）。
复用既有 `HasBase` 零越界；若未来去 `HasBase` 重载，需两-nightly 迁移到显式 `IsStruct`。

分类正确后 `StructLayout.BuildFromSymbols` 从字段名/类型**重算**布局（`_compute` 确定性，与生产方持久化的
`StructSize`/引用位图**逐字节一致**）→ 发 `StructAlloc`/`StructFieldGetPrim/SetPrim`（正确字节 offset）。

**「逐字节一致」的第二个前提：字段类型拼写同口径**。
`_kindOf` 按符号表**裸名键**判「字段是不是 struct」。本地字段拼写由 `MemberCollector` 取
`SurfaceTypeName(已解析类型)`（短名形式 `Point3`）；导入字段若**照搬导出元数据的 FQ 串**
（`Demo.NestLayoutTarget.Point3`）→ 查不到 → 嵌套 struct 字段被判成 8B 引用叶子 → 消费方布局与生产方错位，
读写静默错值。故 `ImportedSymbolLoader._fillClass` 登记 `OwnField` 时同样用 `SurfaceTypeName(fsym.FieldType)`
（解析失败才回落原串，与本地回落对称）。`Point{int,int}` 恰为 8B = 引用叶子
大小，偏移碰巧重合，所以 `struct_cross_pkg` 守不住这条；golden `cross-zpkg/struct_nested_layout_cross_pkg` 用 12B `Point3` + transitive
`Frame` 守住（阴性对照：撤掉处理后 `Error: struct ref leaf at byte offset 8 not in type layout`）。

### 不分类的后果

若 `ImportedSymbolLoader` 不设 `IsStruct` → imported struct 当**引用类型**（消费方不发 struct 指令、构造为
0 长 blob 的引用对象），而生产方 A 的构造函数按 struct 编译（发 `StructFieldSetPrim off=0`）→ 两包对「值
类型否」不一致 → 运行期 `struct field write out of blob bounds (off=0, w=4, len=0)`。golden
`cross-zpkg/struct_cross_pkg` 守住（interp+jit 输出一致）。

## 装箱引用身份 + struct 字段反射

两件相扣的事：**给装箱 struct 引用身份**（对齐 C#）+ **反射按字段名读写 struct 字段**。

### 装箱引用身份（路 B2：装箱进 `ScriptObject`）

若 `Value::BoxedStruct` 载荷是**值**（`Box` 独占，`.cloned()` 深拷贝）→ `object b = a`
是两份独立盒、反射 `SetValue` 改盒调用方看不见，与 C#（box 是共享堆引用）不一致。故载荷是
**`GcRef<ScriptObject>`**（共享堆句柄）：

- 装箱 = 分配一个 **struct 类型的 `ScriptObject`**（`type_desc.is_struct()`，struct blob 存进对象已有的
  `struct_bytes`/`struct_refs`，`slots` 空）。`inline_region_sizes()` 对 `is_struct()` 类型改读该类型自己的
  `struct_layout`（size + ref_count），使 `alloc_object` 为盒分配正确大小的 blob 区。装箱经
  `corelib::convert::box_struct_blob`（`__box_struct` 复用它），拆箱 `unbox_struct` 读对象 blob → 当前帧 arena。
- **复用 `region_object` + 全部 GC 机制，零 GC 核心改动**：GC 的 mark / gen-age / trace / scan_object_refs /
  size / 跨代写屏障（`maybe_mark_cross_gen_card` 的 owner+new 两侧）的 `BoxedStruct` 臂与 `Value::Object` **同路**
  （底层同为 `GcRef<ScriptObject>`）。`is/as/GetType/vcall/Equals` 保持 boxed 值类型特判，只改「读盒」经对象
  `type_desc.name`/`struct_bytes`/`struct_refs`。
- **收益**：`object b = a` 别名同盒、传参改盒可见、反射 `SetValue` 写穿——达成 C# 引用身份。
- **不给 struct 加 base/vtable**——只是把值装进已有对象容器、用 struct 自己的 TypeDesc。

### struct 字段反射（`FieldInfo.GetValue/SetValue`）

反射按**字段名**读值需 per-field 字节 offset + tag，但 runtime `StructTypeLayout` 只有 size + 无名引用位图。
解 = **Rust 复刻编译器 `StructLayout._compute`**（`corelib/struct_reflect.rs`，方案 B——格式中立、warm 本地可验，
非格式 bump 写偏移表）：

- 用 `TypeDesc.fields` 的 `(name, type_tag)` 逐字段自然对齐累积 offset，映射 `字段名 → (byte_off, tag, is_ref,
  is_struct, type_name)`。`canon`/`size_of`/`align_of`/`leaf_kind` 镜像 `Z42Type.Canon`/`_sizeOf`/`_alignOf`/
  `_kindOf`；`tag_from_name` 忠实镜像 `Tag.FromName`（decode signedness 与 codegen encode 一致）。嵌套 struct
  字段短名按声明类型的命名空间解析到 FQ（`resolve_named`）再递归。
- **三层校验**（`validate_against`，抓复刻漂移，不符即可 catch 的 `bail!`）：① `computed.size == 交付 size`；
  ② 计算的引用叶子 offset 集（含嵌套展平）逐一等交付 `ref_offsets`；③ 逐叶子 ref/prim 分类与交付位图交叉核对。
- **GetValue**：基元 → `decode_prim(struct_bytes)`；引用叶子 → `struct_refs[ref_index]`；嵌套 struct → boxed 快照
  （值语义，改返回盒不动父）。**SetValue**：共享盒**就地写穿**（引用身份→调用方可见）+ 引用叶子写屏障；基元实参
  透明拆箱（`object` 参数装箱的基元先 unbox 再 `encode_prim`）。
- 端到端 golden `reflection/struct_field`（GetValue 基元/string/嵌套 + SetValue 写穿+别名可见 + 值语义独立，
  interp+jit 双模式匹配 expected）+ `struct_reflect` 单元测试（布局/校验/tag signedness 护栏）。

### `field_get` 接受装箱 struct

值 struct 经**擦除的返回位**流出泛型函数时（`T id<T>(T a)` 的 `id(v)`），运行期的值就是上面那个
装箱 `ScriptObject`。`field_get` 必须认它，否则同一个接收者上
`id(v).Sum()` 正常、`id(v).X` 会抛 `FieldGet: expected object, got BoxedStruct(…整屏堆转储…)`。

- **为什么落到通用 `field_get`**：调用点的静态类型是裸 `T`（`--dump-bound` 实测
  `(call id … :T)`、成员 `:<unknown>`）⇒ `AccessEmitter._emitMember` 的
  `_isBlobStruct(m.Target.Type())` 判假 ⇒ 不走 `struct_fget_prim`（那条本就认盒）。
- **做法 = 复用反射那条已验证的按名取叶子路径**（`accessors::boxed_struct_field_get`，
  含 `validate_against` 布局对账），interp（`exec_object.rs`）+ JIT（`jit_field_get`）**两条臂对称**
  —— 只做一侧的话小用例全绿而热代码崩。
- **`field_set` 刻意不跟着加**：写进「从擦除返回位流出的临时盒」必然被丢弃，正解是编译期拒绝
  （C# 同）⇒ 待办：编译期拒绝对擦除调用结果赋值。
- 端到端 golden `generics/erased_return_blob_field.z42`（四种叶子 + 显式/推断型参 + 静态方法承载
  + 200k 次热循环逼出 OSR 走 JIT 臂）。⭐ **两条臂各自有阴性对照**：撤 interp 臂 → interp
  措辞红；撤 JIT 臂 → **JIT 措辞红**（证明热循环真的进了 `jit_field_get`，而不是全程解释执行）。
- ⚠️ 验这类改动必须 `xtask build runtime` **再** `package dev-sdk`：`package dev-sdk` 只装配、**不重编 Rust**，
  只跑后者会拿到上一轮的 `z42vm`，得到一字未变的假阴性。
- 📉 **已知代价**：`struct_reflect::compute` + `validate_against`
  **每次访问都重算**（反射那条是冷路径，从来没人给它加缓存）。实测 interp 下 500 万次
  `id(v).X` = 2.29s，同规模普通对象字段读 = 0.78s ⇒ 每次访问约 300ns 的布局重算。
  待办：按类型名缓存 `ComputedLayout`（布局按类型不变、缓存天然安全，
  反射侧同样受益）；缓存要挂在 `VmContext` 或 `TypeDesc` 上，属独立取舍。

### 对象内联 struct 字段反射（`class C { Point pt; }`）

**堆对象上的内联 struct 字段**（`class C { Point pt; }`）若走普通 slot 反射，`GetValue(fi_pt, c)` 读的是
**dead slot → `Null`**。故读写路径**复用同一套字节解码基础设施**：

- **类级内联布局复刻**（`struct_reflect::compute_class_inline`，镜像编译器 `StructLayout._computeInlineLayout`）：
  与 struct `_compute` 不同——类**只把 struct 字段**按声明序打包进对象 `struct_bytes`（自然对齐、引用叶子展平进
  `struct_refs`），**非 struct 字段仍在 slots**。产出「struct 字段名 → 对象相对 (byte_off, size, 引用叶子)」+
  对象相对引用位图，用 `validate_against` 对交付的 `TypeDesc.inline_layout`（已 wire 的合成布局）做同款三层
  校验抓漂移。
- **GetValue**：`struct_field_fq` 判定字段是否内联 struct——是则从 `struct_bytes`+`struct_refs` 物化 **boxed 快照**
  （值语义，改快照不动对象）；否则回落普通 slot 读。嵌套 struct 叶子（`Frame{Line edge}`）递归展平。读取逻辑与
  装箱 struct 的嵌套字段共用 `snapshot_struct_leaf`。
- **SetValue**：内联 struct 字段把传入的 boxed struct 字节+引用叶子**就地写穿对象共享字节区**（对象是堆节点 →
  引用身份可见，别名/后续读都见新值）+ 引用叶子写屏障。写入逻辑与装箱 struct 的嵌套字段共用 `write_struct_leaf`
  （该 helper 含嵌套引用叶子的写屏障，装箱路径亦受益）。
- golden `reflection/struct_field` 扩：普通 slot 字段仍走 slot（`id`/`label`）+ 内联 struct 字段 GetValue 快照 +
  SetValue 写穿（对象直读验证）+ 快照值语义独立 + 嵌套内联（`Frame{Line edge}` 展平引用叶子）；`struct_reflect`
  有类级布局单测。**纯 runtime、格式中立。**

> **待办**：反射 invoke boxed struct 合成方法（Equals/GetHashCode/ToString）、static struct 字段反射。

## 与逃逸分析 / packed 数组的关系

- struct 恒内联，**不走** `ObjNew`→堆/`StackObject` arena；逃逸 arena 是**引用类型**的
  分配优化，struct 内联是**值类型**的语言语义——两套机制。
- 字节 blob 地基与 packed 基元数组的字节 `ArrayBacking` 收敛（`struct[]` 字节 backing）。

## 泛型实例化与 struct 布局

### 泛型擦除槽的值复制

**按实例化算布局 + 部分具体化**。若 struct 值存进声明类型为 `T` 的字段（泛型 struct **与泛型 class** 都是）时按句柄存、**不复制**，
复制外层也只浅拷句柄 ⇒ 与源变量/副本共享同一块 blob。三形态都会错，其中泛型 class 那条还是
**use-after-free**（栈帧作用域的 arena `StructRef` 存进 GC 堆对象字段槽，ctor 帧一弹 arena 截断 ⇒ 悬垂）。

**根因不是「存入时忘了复制」，是那个槽物理上放不下字节**：`_kindOf("A")`（型参名）落
`StructLeafKind.GcRef` 8 字节句柄，因为布局**按定义**算。故正解是让实例化拿到**自己的布局**
（型参字段变真内联字节），并**按该布局特化其成员**——布局特化了，把偏移烘焙进指令的代码就必须跟着特化。

**闸门** = `InstDiffersFromDef`（实例化布局确实不同于定义布局）。相同则共享定义那份体，
特化是 no-op ⇒ 产出逐字节不变。**零格式 bump**（TYPE 段只是追加描述符条目）。

**两个正交概念**（别混）：**身份名**管描述符 / `StructAlloc` / 数组元素名；**特化名**管布局查询 /
成员派发。布局相同的实例化有独立身份但共享定义的成员体——共享体按定义偏移烘焙，布局既相同则对得上。

> ⚠️ **「实例化是独立类型」的两个硬后果**：① 合成 `Equals` 的 `other is <类型>` 必须用实例化名，
> 否则装箱值类型对不上 ⇒ **值相等静默误返 false**；② **VCall 按运行期类型名派发**，身份一变
> `MyList<int>.Add` 就找不到 ⇒ 必须配**擦除名回落**（miss 后剥实参重试，只在 miss 路径跑）。

**待办（仍未覆盖）**：① **普通泛型 class 不取独立身份**——给它独立身份要合成完整类描述符
（基类链/接口/静态字段；**vtable 不需要** —— 运行期从 `own_methods` + 基链 merge）；
② **跨包实例化不特化**（消费方编译只读依赖签名，拿不到生产方方法体无法重发）⇒
**`Std.ValueTuple` 在 z42.core，用户写 `(P2,int)` 是跨包，尚未覆盖**；
③ 容器 backing 仍靠泛型装箱（`List<T>` 内部 `new T[n]` 只编一份、元素名是字面 `"T"`）。

### 🔴 单调化必须是**闭包**

只特化实例化**类型**、不特化**操作它的泛型代码**是不够的。泛型体只编一份、按
**擦除布局**烘焙字节偏移，而调用方按实例化布局造值 ⇒ 同一批字节两种理解 ⇒ **静默错值**。

```z42
int ReadSecond<T>(Loc<T,int> p) { return p.Item2; }   // 体内：读 Item2，按擦除布局解成 @8
Loc<P2,int> t = new Loc<P2,int>(a, 7);                // 实例化布局里 Item2 在 @16，@8 是 P2.Y
t.Item2           → 7   ✅
ReadSecond<P2>(t) → 2   ❌   （若只特化类型：虚派发形态读出 102 而非 107）
```

> **不变式**：凡是碰到 `G<A,B>` 的值的代码，都必须对它的布局达成一致。
> 部分单调化**按构造**违反它——这不是覆盖率问题，是正确性问题。

**根因一句话**：z42 的 IR 把**字节偏移烘焙进指令**，而泛型体只编一份。「一份体」与「多种布局」
必然对不上，除非每种布局各编一份体。

特化覆盖：泛型自由函数 / 静态泛型方法 / 实例泛型方法（含虚与 override），与实例化类型的
特化**共用同一个不动点**——特化一个体会发现新实例化，特化一个实例化的成员又会调用新的泛型体。

两条只有实测才会知道的约束：

- **特化名不沿基类链继承**（`vcall_resolve`）。一份特化按**某一个声明**所在类型的布局烘焙了偏移；
  虚派发从接收者的**运行期**类型起走链，落到基类的特化体就是**静默调错实现**。
- **有错误时不进代码生成**（自然也不做特化）。递归泛型（`Rec<T>` 调 `Rec<G<T>>`）让类型实参逐层加深 ⇒
  工作项无限增长 ⇒ **编译器不返回**；良型却展不完的情形由工作表上限报 E0503。

**跨编译单元**：泛型声明与实例化在不同文件时，`SemanticModel` 由
`TypeChecker.Infer(cu, …)` **按 CU** 建，用当前 CU 的 `HasBody` 恒假 ⇒ 静默不发 ⇒
`MissingSymbolException: undefined function Demo.Loc<P2,int>.Loc`。故包级登记表
`GenericBodies` / `GenericTypeDecls` 里带上**声明所属 CU 的 model**（`GenericBodySrc.Model`）。

**待办（仍未覆盖）**：**跨包**实例化——消费方编译只读依赖的签名，拿不到生产方的方法体。

### 🔴 特化改变调用约定：sret 必须两侧同时翻

泛型 **class** 的方法返回**类级型参**、而该型参被实例化成 blob 值 struct 时：

```
fn @Demo.G<P2>.Get(1) -> T {
  %1 = struct_alloc Demo.P2 [16B]   ← 在**自己帧**的 arena 里造
  …逐字段拷贝…
  ret %1                            ← 帧一弹即悬垂
}
```

⇒ `struct-value handle used after its creating frame exited — value-struct lifetime unsound`。

根因是**两侧都在看未代换的返回类型 `T`**：callee 的 `FunctionEmitter._blobStructNameT(T)` 判否
（`T` 既非 ClassType 也非 InstantiatedType），caller 的 `_isBlobStruct(c.Type())` 也判否
（装箱模型下 `c.Type()` 是 `Unknown`，外层包 `BoundConvert` 拆箱）。两边都判否 ⇒ **表面自洽**，
代价是返回一个已死帧的句柄。

> 🔬 **只修一侧＝换个地方错**：原型中只让 callee 走 sret，症状立刻变成
> `takes 2 physical argument(s), the call passes 1`。「lifetime unsound」与「签名对不上」
> 是同一条 bug 的两副面孔，取决于哪一侧先判出具体类型。

**做法（两侧 + 一道共同闸门）：**

| 侧 | 谁提供信息 | 做什么 |
|---|---|---|
| callee | `IrGenTypeEmitter.EmitInstantiation` 铺**类级** `SpecTypeArgs`（`T→P2`） | `_blobStructNameT` 认出代换后的 blob ⇒ 置 `METHOD_FLAG_SRET` |
| caller | typer 在 `MemberResolver` GS6 分支写 `BoundCall.InstRetType`（代换后的返回类型） | `CallEmitter._specSretName` 判定后预留返回槽、作末尾隐藏实参传入 |

⭐ **闸门必须是同一个**（判据单一出口）：`_instLayoutName(receiver) != ""` ——
即「该实例化的成员**确实被重发过**」。它同时决定**成员派发名**与**调用约定**，不可能漂移成两把尺子。

⚠️ **调用约定是两侧协议，一侧单方面改就是 ABI 撕裂**：callee 侧的代换被
`SpecInstName != ""` 限定在**类实例化通道**。若去掉这道闸门，泛型**自由函数**的特化体
（`T makeValue<T>() where T : struct`）也跟着走 sret，而它的调用侧无从得知 ⇒
`Demo.makeValue:Pair … takes 1 physical argument(s), the call passes 0`（实测）。
泛型体那一侧两边都不代换 ⇒ 自洽（装箱模型），要改得连调用约定一起改。

> 为什么 caller 不自己算代换后的返回类型：那要**重做一遍重载决议**才能拿到被调方的声明返回
> 类型，查错就是错发 —— 同 `BoundCall.RetIsNullable` 的理由。typer 手里已有解析好的签名，
> 代换一次记下来即可；发射端只做它自己独有的那半判断（布局知识只在发射端）。

用例：`src/tests/generics/generic_class_returns_blob.z42`（六形态，含两条阴性对照）。

### 🔴 实例化是运行期真正的类型

实例化有**布局**、操作它的代码跟着**特化**之后，**声明形状**也不能是擦除的：
若普通泛型 class 的实例化不发描述符（闸门要求「有内联 struct 字段」），则

| 形态 | 擦除时 | 应为 |
|---|---|---|
| `class DInt : GBox<int> {}` 的继承字段 | `null` | `0` |
| `o as GBox<string>`（o 是 `GBox<int>`） | 放行 → `VCall: expected object, got I64` | `null` |

两者**互相咬死**，不能只做一格：只给身份不改 `is`/`as`，`x is GBox<int>` 会从 true **静默**变 false。

**编译期（四处，共用一个出口）**

- `_instIdentityName` 无闸门 ⇒ 每个具体的**本包**实例化都有身份。
- `_instClassDesc` 发**完整**描述符：基类链 / 接口按实参代换，字段类型名代换。
  ⭐ **字段的集合与顺序必须从定义那条描述符 `_classDesc` 派生**，不能从 `StructLayout` 另起一份
  ——后者不含属性后备字段（`__prop_X`）等合成条目，两份对不上就是运行期字段槽错位，
  实测 `MulticastException<bool>.Results` 读出 `Null`。
- 剥名只对**开放**泛型基保留（`class Sub<T> : Bag<T>` 不是具体实例化，
  永远不会有描述符，原理由对它依然成立）；**闭合**基写实例化名并登记它。
- `is`/`as` 的目标名走 `_instIdentityName`（`BoundIsExpr.TargetType` / `BoundCast.Type()`），
  而不是丢实参的 `NamedType.Name`。

⚠️ **实例化名的基名必须 arity-mangled**（`Pair$2<int,string>`）。泛型类与同名非泛型类共存时
，不 mangle 的话擦除前缀是 `Demo.Pair` —— 那是**非泛型**的那个类，
运行期擦除回落会调到它身上（实测 `p2.Describe()` 返回 "non-generic Pair"，**静默**错值）。

⚠️ 描述符投送必须**走到不动点**：造一条描述符会发现新的实例化（基表上的 `Bag<int>` 只有在造
`Sub<int>` 的描述符时才被登记）。快照一次 `Keys()` 就漏，实测
`base type Demo…Bag<int> of Demo…SubBag<int> could not be resolved`。

**运行期（三处，全是「擦除名是回落、不是身份」的贯彻）**

- `vcall_resolve`：擦除名回落要在基链的**每一层**做，不只接收者自己那层 ——
  `class DInt : GBox<int> {}` 的基是实例化，而成员体只以擦除名存在
  （实测 `VCall: function Demo.DInt.Tag not found`）。顺序：接收者自己的擦除定义
  先于**基类**的同名方法。
- `is_subclass_or_eq_td`：`x is GBox`（不带实参，C# 写不出来）意为「任何 GBox 的实例化」，
  故每层都比一次擦除前缀。两种拼写都要认：裸名 `Demo.GBox`，以及 arity-mangled 的
  `Demo.GBox$1`（**导入**泛型在元数据里的拼写，`StmtEmitter` 的 catch_type 就是它）——
  漏掉后者时 `catch (MulticastException<bool>)` 抓不住 `Std.MulticastException<bool>`。
- `build_type_registry`：**实例化的名字就是它的类型实参**，在这里解析成 `type_args`。
  编译期的 `ObjNew` 一旦用身份名就不再另发一份实参列表（发两份会渲染成 `Demo.Box<int><int>`），
  反射 `Type.GetGenericArguments()` 与泛型字段零初始化都读 `type_args` ⇒ 名字成为唯一真相。
  interp 与 JIT 的 `ObjNew` 必须**同时**回落到它（这一对若一边倒，就会出现
  `GBox<int>().V == 0` 两个引擎答案不一致）。

用例：`src/tests/generics/generic_class_identity.z42`（B/D 两格 + **五条阴性对照**：
擦除名 `is GBox`、接口代换、派生类两个方向、开放泛型基、反射仍视其为构造泛型类型）。

### 🔴 类型测试的目标名只能有一个出口

`is` / `as` / cast / **模式匹配** 问的是同一个问题：「这个值是不是那个类型」。
它们的目标名若有**两份**实现（`TypeOpEmitter` 与 `PatternEmitter` 各写一遍
「`QualifyTypeName` + Array/Object 归一」），只改一份就会出现：

```z42
object o = new Box<int>(1);
o is Box<string>                      // → false   ✅
switch (o) { case Box<string> b: … }  // → **匹配上**，b 拿到装着 int 的 Box<string>
```

**同一个问题两个答案**，而且后者是静默的、还把错类型漏给下游。三种模式形态
（类型模式 / 位置模式 / is-结构化模式）与属性模式实测全中。

> ⚠️ 两侧都用擦除名是「都错但一致」；只改一侧 ⇒ **引入不一致** ——
> 不一致比一致的错误更难查，这是「同一判据散在多处」最贵的一种形态。

唯一出口是 `ExprEmitter._typeTestName(resolved, astName)`：
优先 `_instIdentityName`（与描述符 / `ObjNew` / 数组元素名同源），算不出来才回落 AST 名
（那条回落是给内建类 / 跨包泛型 / 开放泛型的，`ResolveType` 对它们得 Unknown）。
模式节点本就带着 binder 解析好的类型（`BoundTypePattern.BoundType` /
`BoundPositionalPattern.Type` / `BoundPropertyPattern.Type` / `BoundAtPattern.Type`），
**不需要新字段**。

⚠️ 用例 `pattern_generic.z42` 只测「正确实例化能匹配」，判别力全在**阴性**那半。
`pattern_generic_identity.z42` 六格全是阴性形态，且撤回处理会变红。

### 🔴 静态成员按闭合类型各一份

C# 里 `GBox<int>.Count` 与 `GBox<string>.Count` 是**两个槽**。若两侧都按擦除名拼键，
自洽但语义错：各构造 2 / 3 次，两边都读出 5。

**三件事必须同时成立，缺一格就是另一种错：**

| | 做什么 | 缺了会怎样 |
|---|---|---|
| ① 键 | `AccessEmitter._staticKey` 是**唯一出口**（读 / 写 / 属性后备三条路都调它） | 同一个槽因走哪条路而拼出两个名字 |
| ② 体 | 成员按实例化各发一份 | **一份共享的体只能写一个键** —— 键改了也没用 |
| ③ 初始化 | 类型初始化器也各一份，描述符挂各自的 `$Cctor` | `static int Seed = 7;` 恒读出 0（定义那份 cctor 写擦除键、读方查实例化键） |

⭐ **②的闸门是 `IrGen.InstNeedsOwnBody`，发射侧与派发侧必须共用它**：

```
InstNeedsOwnBody(inst) = Layouts.InstDiffersFromDef(inst)      // ① 布局不同
                       || DefHasStaticState(基名)               // ② 定义有静态状态
```

两边错开的后果是确定的：只放宽**发射** ⇒ 特化体发出来但没人调（死代码，行为一字不变）；
只放宽**派发** ⇒ 调用点指向一个没发射的名字（运行期 MissingSymbol）。

⚠️ **`EmitStaticInit` 自己拼键、不经 `_staticKey`**，所以要在那里也认一次 `SpecInstName` ——
漏掉时 `Demo.GBox<int>.$cctor` 的体里写的还是 `Demo.GBox.Seed`（实测，症状就是 ③）。
这又是「同一判据散在多处」，只不过这次是**同一个键的两个拼法**。

**解析器**：`GBox<int>.Count` 须可解析（否则 `E0202`）。`<类型列表>` 后紧跟 `.` ⇒ 左边是**类型引用**，
无歧义（二元 `<` 的 `>` 之后不可能紧跟 `.`，那样缺操作数）。实参挂到左边的 `IdentExpr`
（`IdentExpr.TypeArgs`）而**不新造 AST 节点** —— 每加一个节点类型，每个 walker 都要补分支，
漏一个就是静默跳过。

用例：`src/tests/generics/generic_static_per_instantiation.z42`（三格 + **三条阴性对照**：
非泛型静态字段、`a < b` 仍是比较、实例字段仍每对象一份）。

## 待办

**单标量叶子 struct 塌缩**（`GCHandle`）、**JIT 原生内联字节访问**（现 helper 桥接=interp 速度）、**反射合成方法可见**、**static struct 字段反射**、**ToString 字段 dump**、**E0438 自引用诊断**（现 `Size==0` 兜底防崩）、**`(P)o` 拆箱失败报错**（对 blob struct 目标发 `AsCast`，类型不符 / `o` 为 null 时得 `Null`，不抛 `InvalidCastException` / `NullReferenceException`；基元拆箱已抛，见 [object-abi](object-abi.md)）尚未实施。
