# Tasks: complete-generic-class-identity（让泛型实例化成为运行期真正的类型）

> 状态：🟢 实施中（P4 已完成并全绿）| 创建：2026-09-25
> 分支/worktree：`generic-class-identity` @ `wt-geninst` | 基于：#828 的 head
> 类型：`lang` + `ir`

**变更说明：** User 早已裁决「实例化是独立类型（对齐 C#）」。#774 为 blob struct 兑现了；
普通泛型 **class** 至今没有。代价已具体化为四条 soundness 缺口。

## 进度概览

- [x] **P4** 实例化成员的调用约定按实参代换（用例 A：`g.Get()` 返回 blob）— 全绿
- [x] **P1** 实例化描述符完整化（基类链 / 接口 / 代换后字段）— 全绿
- [x] **P2** `is` / `as` 携类型实参 — 全绿（**与 P1 同一刀**，它们互相咬死）
- [x] **P5-a** cast 接受闭合泛型（`(G<int>)o`）— 全绿
- [ ] **P3** 静态字段按实例化分槽（⚠️ 自举敏感，分阶段引入）
- [ ] **P5-b** 闭合泛型上的静态成员（`G<int>.X`）— 与 P3 同时落地（它是唯一消费方）
- [ ] **P4b** 泛型**体**（自由函数 / 泛型方法）的返回型参也走代换后的调用约定
- [ ] 模式匹配（`case GBox<int> b`）携实参 —— `is`/`as` 已做，模式那条路单独核实

⚠️ **P1–P3 互相咬死，不能只做一格**（见 design.md §D1 的实测）。P5 是 P2/P3 的**可测性前置**
——不修则那两格在源码层写不出来。**P4 与它们正交**，故先独立落地。

> 实测修正：**P3 可以单独留到后面**。P1+P2 一起落地后全量绿（含自举不动点）——
> 静态字段今天两侧都按**擦除名**（描述符不声明、发射端也不写实例化键）⇒ 自洽，
> 只是语义不对（两个实例化共享一个槽）。咬死的是 P1↔P2，不是 P1↔P3。

## P4 已落地（2026-09-25）

用例 A 的根因不是描述符，是**调用约定**：callee 与 caller 都在看未代换的返回类型 `T`，
双双判否 ⇒ 不走 sret ⇒ 特化体把返回值拷进自己帧的 arena 再交出句柄。

| 改点 | 文件 |
|---|---|
| 特化通道铺**类级**型参代换表 | `IrGenTypeEmitter.EmitInstantiation` |
| callee：代换后的型参算具体类型（限类实例化通道） | `FunctionEmitter._blobStructNameT` |
| typer 记下代换后的返回类型 | `BoundCall.InstRetType` ← `MemberResolver` GS6 分支 |
| caller：据此预留返回槽 | `CallEmitter._specSretName` |

⭐ **闸门单一出口**：两侧共用 `_instLayoutName(receiver) != ""`（该实例化的成员确实被重发过），
它同时决定成员派发名与调用约定 ⇒ 不可能漂移成两把尺子（design D5）。

🔴 **实测教训（已写进 internals）**：callee 侧的代换必须被 `SpecInstName != ""` 限定在**类实例化
通道**。放开后泛型**自由函数**的特化体也走 sret，而它的调用侧无从得知 ⇒
`Demo.makeValue:Pair … takes 1 physical argument(s), the call passes 0`。
**调用约定是两侧协议，一侧单方面改就是 ABI 撕裂** —— 这正是 P4b 存在的理由。

**验证**：e2e golden interp 351 / jit 347 全绿、cross-zpkg 81、multi-exe 3、stdlib 340、
自举字节不动点 3/3、`lines` / `docs` / `walkers` / `diagcodes` 全绿。

## P5-a 已落地（2026-09-25）

两条 cast 前瞻都是**定长**的（`( Ident )` / `( Ident [ ] )`），装不下变长的 `<…>`。
修法 = 与泛型方法调用（`ExprParser.z42:115`）同款的**回溯**：游标 + 诊断 + `_pendingGt`
（`>>` 拆分状态）三件一起存档还原，**缺一不可**。预检 `( Ident <` 才试 ⇒ 非泛型形态
一条指令都不多走，既有产物字节不变（自举不动点 3/3 复验）。

⚠️ `_castOperandStart` 仍**刻意排除** `(`：`(f<int>)(x)` 保持泛型调用而非 cast。

**P5-b 为什么不在这里做**：`G<int>.Count` 解析出来需要一个**携类型实参的类型引用**节点，
其唯一消费方是 P3（静态字段按实例化分槽）。先造节点没人消费 = 造一个必然漂移的半成品。

## P1 + P2 已落地（2026-09-25，一刀）

从 11 条判红一路收到 0 条。**每一条都是真缺口**，按出现顺序记下来（这条线上第五次证明
「同一判据散在多处」是主要风险来源）：

| # | 症状 | 根因 |
|---|---|---|
| 1 | `base type Bag<int> of SubBag<int> could not be resolved` | 描述符投送循环只**快照一次** `Keys()`；造描述符本身会发现新实例化（基表上的） ⇒ 要走不动点 |
| 2 | `catch (MulticastException<bool>)` 抓不住 | 擦除名有**两种拼写**：裸名与 arity-mangled `$1`（导入泛型在元数据里的写法） |
| 3 | `MulticastException<bool>.Results` 读出 `Null` | 我的字段表另起一份（`ObjectLayoutOf`），与定义描述符的**集合与顺序**对不上（属性后备字段 `__prop_X`）⇒ 改为从 `_classDesc(定义)` 派生 |
| 4 | `GetType().IsGenericType` 变 false | 清空了实例化描述符的 `TypeParams`；反射判据正是它 |
| 5 | `GetGenericArguments()` 变空 | ObjNew 用身份名时不再另发实参列表 ⇒ 让**注册表从名字解析** `type_args`（名字成为唯一真相），interp / JIT 两侧同时回落 |
| 6 | `p2.Describe()` 返回 "non-generic Pair" | 实例化名的基名没 arity-mangle ⇒ 擦除前缀 `Demo.Pair` 撞上**同名非泛型类**（静默错值） |
| 7 | `VCall: function Demo.DInt.Tag not found` | 擦除名回落只在**接收者自己**那层做，基链上的实例化层级够不着 |
| 8 | 单测 `test_new_generic_multi` | ObjNew 类名从「裸名 + 另一份 TypeArgs 渲染」变成身份名 ⇒ `Pair<int, string>` → `Pair<int,string>`（无空格），golden 更新 |

⭐ **7 顺带修正了一处顺序错误**：擦除名回落原先写在链表遍历**之后**，于是「接收者自己的
擦除定义」排在「基类的同名方法」后面。改成逐层之后，override 正确地赢过基类。

**验证**：e2e golden interp 353 / jit 349、cross-zpkg 81、multi-exe 3、stdlib 340、
自举字节不动点 3/3、runtime `cargo test --lib` 1394、
`lines` / `docs` / `walkers` / `diagcodes` 全绿。
（`ClassDescBuilder.z42` 因此超 886 行，按既有先例 partial 拆出 `.GenericInst.z42`。）

## 顺带发现（独立缺口，不在本线）

- **嵌套型参在 ctor 形参位被代换成 `<unknown>`**：
  ```z42
  class G<T> { public G(T v) {…} }
  class Wrap<U> { public G<U> Inner; public Wrap(G<U> inner) {…} }
  new Wrap<P2>(new G<P2>(p));   // E0402: cannot assign G<P2> to G<<unknown>>  ← 假红
  ```
  纯 typer 侧（不涉布局/调用约定），`MemberResolver._substGeneric` 在
  `Z42InstantiatedType` 递归分支里对外层类的型参 `U` 解析失败。与型参重名无关（换名复现）。
  单独登记。

## 实测取证（main `ef88897ac`，全部实跑）

| # | 用例 | 实测 | C# |
|---|---|---|---|
| A | `class G<T>{T V; T Get(){return this.V;}}` → `g.Get().X` | `struct-value handle used after its creating frame exited — value-struct lifetime unsound` | `42` |
| B | `class DInt : GBox<int> {}` → `d.V` | `null` | `0` |
| C | `GBox<int>` / `GBox<string>` 静态计数 | `4 / 4` | `2 / 2` |
| D | `o as GBox<string>`（o 是 `GBox<int>`） | 放行 → `VCall: expected object, got I64(42)` | `null` |
| — | `(GBox<string>)o` / `GBox<int>.Count` | `E0202` 解析失败 | 合法 |

### 🔬 A 的原型实证（值得照抄的取证手法）

只让 **callee** 认出 `T→P2`（走 sret），症状立刻从
`value-struct lifetime unsound` 变成 `takes 2 physical argument(s), the call passes 1`。
⇒ **「lifetime unsound」与「签名解析不到」是同一条 bug 的两副面孔**，取决于哪一侧先判出具体
类型。**这是「必须整体做」的直接证据**，不是论证。

## 关键已知事实（省掉重新发现）

- ⭐ **vtable 不需要合成**：运行期 `build_type_registry` 从 `own_methods` + 基链 merge 出来，
  **不在 TYPE 段**。#774 把它算进「完整描述符」的负担里，那一条是**高估**。
- ⭐ **`_substGenericSig` 已存在**（`MemberResolver.Subst.z42:48`，形参位 + 返回位按 receiver
  的类级实参递归代换），今天只用在 `MemberResolver.z42:211`（接口成员）与
  `ConstructTyper.z42:282`（构造器）。P4 是**铺满**它，不是发明它。
- 🔴 基类名被**显式剥成裸名**在 `ClassDescBuilder.z42:151-162`（`fix-generic-base-name`），
  注释写明理由。身份成立后该理由消失，但必须与 P2 同时落地。
- 🔴 `is`/`as` 丢实参在 `TypeOpTyper.z42:46`（as）与 `:334`（is）；运行期按**名字符串**比
  （`dispatch.rs` 的 `is_subclass_or_eq_td` 首行 `derived == target`）。
- 🔴 静态字段键在 `AccessEmitter.z42`（`QualifyClass(裸名) + "." + 字段`）；运行期按 FQN
  字符串索引（`vm_context/types.rs` 的 `static_field_index`）。
- 解析器两处前瞻：cast 在 `ExprParser.z42:365`（定长 `( Ident )`），泛型出口在 `:114-130`
  （`<…>` 后须紧跟 `(`）。`_parseType()` 本身**早已支持**闭合泛型（`as` 走的就是它）。

## Out of Scope（已实测定性，别混进来）

- **单字段 struct 无值语义**：`struct S1 { int F; }` 的数组元素读
  `FieldGet: expected object, got Null`。**非泛型同样崩** ⇒ 与泛型无关，根因是
  `IsBlobStruct` 硬性要求 `FieldCount >= 2`。单独登记。
- 跨包模板投送（`complete-generic-instantiation` S2）、容器密集化。

## 验证纪律（照抄，别重新踩）

- ⚠️ **本地 `xtask test all` ≠ CI**：本地只含 e2e + stdlib + compiler，**不含 `test lines` /
  `walkers` / `docs` / `diagcodes`**。要么逐个点名，要么承认只有 CI 权威。
- ⚠️ **全绿 ≠ JIT 验过**：golden 只跑 interp，必须显式 `xtask test e2e --mode jit`。
- ⚠️ **单文件用例对「按 CU / 按包」的机制没有判别力**。
- ⚠️ **对照实验的两棵树只能差「我的改动」一个变量**；基线不同就不是对照。
- ⚠️ `git checkout <别的分支> -- src` 会**留下**该分支独有的文件（`checkout HEAD -- src` 不删）。
