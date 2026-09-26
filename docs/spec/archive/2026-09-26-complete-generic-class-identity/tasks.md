# Tasks: complete-generic-class-identity（让泛型实例化成为运行期真正的类型）

> 状态：✅ **已归档**（2026-09-26）。#829(P4) / #830(P5-a) / #831(P1+P2) / #834(P3+P5-b) /
> #835(顺带：ctor 形参在调用点解析) / 本刀(模式匹配携实参) 全部合入 main。| 创建：2026-09-25
> 分支/worktree：`wt-geninst`，按刀切分支（`-p1` / `-p5` / `-p3`）
> 类型：`lang` + `ir`

**变更说明：** User 早已裁决「实例化是独立类型（对齐 C#）」。#774 为 blob struct 兑现了；
普通泛型 **class** 至今没有。代价已具体化为四条 soundness 缺口。

## 进度概览

- [x] **P4** 实例化成员的调用约定按实参代换（用例 A：`g.Get()` 返回 blob）— 全绿
- [x] **P1** 实例化描述符完整化（基类链 / 接口 / 代换后字段）— 全绿
- [x] **P2** `is` / `as` 携类型实参 — 全绿（**与 P1 同一刀**，它们互相咬死）
- [x] **P5-a** cast 接受闭合泛型（`(G<int>)o`）— 全绿
- [x] **P3** 静态成员按闭合类型各一份 — 全绿
- [x] **P5-b** 闭合泛型上的静态成员（`G<int>.X`）— 全绿（与 P3 同刀）
- [x] **模式匹配携实参** — 全绿（核实后发现是**活的静默错值**，见下）
- [ ] **P4b** 泛型**体**（自由函数 / 泛型方法）的返回型参也走代换后的调用约定
      —— 今天两侧都不代换 ⇒ 自洽（装箱模型），**不是活 bug**，留作可选后续

> 🔴 **design.md §D1 的两条前提被实测推翻，以本节为准**：
> 1. 咬死的只有 **P1↔P2**，P3 **不在环里** —— 静态字段修前两侧都按擦除名 ⇒ 自洽（只是语义错），
>    所以 P1+P2 单独落地就能全量绿含自举不动点。
> 2. P3 **不需要分阶段引入** —— 全仓 139 个泛型类型声明里带静态字段的是 **0 个**，
>    「生产方与消费方必须同代编译器」那个危险没有任何实例能咬到。当时定「自举敏感」时没量过。
>
> P5-a 是独立的用户可见缺口；**P5-b 不是 P3 的测试前置**（读走实例方法就能测），
> 它只解决「能不能在源码里写出来」，故与 P3 同刀落。**P4 与全部这些正交**。

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

| 9 | CI `bench-regression` 判红：`编译器输出变了，但 CompilerFingerprint 没变` | 本档改的是**发射的元数据**（多出实例化描述符 / base 名 / `is`·`as` 目标名）⇒ 含泛型的源文件**哈希不变而发码变** ⇒ 必须 bump（18 → 19） |

⭐ **#9 是唯一一条本地跑不出来、只有 CI 能抓的**：`xtask test fingerprint` 要一棵 base 源码树
（CI 里是 bench-pr 已备好的 `base-src`），本地 15 个 stage 里没有它。
⭐ 这一档与前几档不同：**守门真的看见了输出变化**（z42.core），不是「stdlib 里没这种形状、
只会漏判」的那类。

⭐ **7 顺带修正了一处顺序错误**：擦除名回落原先写在链表遍历**之后**，于是「接收者自己的
擦除定义」排在「基类的同名方法」后面。改成逐层之后，override 正确地赢过基类。

**验证**：e2e golden interp 353 / jit 349、cross-zpkg 81、multi-exe 3、stdlib 340、
自举字节不动点 3/3、runtime `cargo test --lib` 1394、
`lines` / `docs` / `walkers` / `diagcodes` 全绿。
（`ClassDescBuilder.z42` 因此超 886 行，按既有先例 partial 拆出 `.GenericInst.z42`。）

## P3 + P5-b 已落地（2026-09-25）

📐 **先量后做，推翻了本线 design 的一条前提**：全仓 **139 个泛型类型声明，带静态字段的是 0 个**
（两种独立扫法，覆盖 `src/libraries` / `src/compiler` / `src/toolchain` / `src/tests` /
`examples` / `docs`）。⇒ **分阶段引入不需要**：「生产方与消费方必须同代编译器，否则 cctor 写一个槽、
读方查另一个槽」这个危险**没有任何实例**能咬到。当时定「自举敏感」时没量过。
⇒ 同一条实测也意味着 **CI 的 fingerprint 守门对这一档是瞎的**（stdlib 产物一字不变）⇒ 手动 bump 到 20。

三件事必须同时成立，缺一格就是另一种错：

| | 做什么 | 缺了会怎样 |
|---|---|---|
| ① 键 | `AccessEmitter._staticKey` 单一出口（读 / 写 / 属性后备三条路都调） | 同一个槽因走哪条路拼出两个名字 |
| ② 体 | 成员按实例化各发一份（闸门 `IrGen.InstNeedsOwnBody`） | **一份共享的体只能写一个键** —— 键改了也没用 |
| ③ 初始化 | 类型初始化器各一份 + 描述符挂各自的 `$Cctor` | `static int Seed = 7;` 恒读出 0 |

⭐ **②的闸门发射侧与派发侧必须共用**：只放宽发射 ⇒ 特化体没人调（死代码）；只放宽派发 ⇒
运行期 MissingSymbol。
🔴 **实测撞到的**：`FunctionEmitter.EmitStaticInit` **自己拼键、不经 `_staticKey`** ⇒
`Demo.GBox<int>.$cctor` 的体里写的还是 `Demo.GBox.Seed`（症状就是 ③）。又一次「同一个键两个拼法」。
🔴 另一条：P1 期我把实例化描述符的 `StaticFields` **清空**了（那时发射端还按擦除名，声明了也没人写）。
P3 两侧同时换键后必须加回来（按实参代换类型名），否则运行期
`static field Demo.GBox<int>.Count is not declared on type Demo.GBox<int>`。

**P5-b**：`<类型列表>` 后紧跟 `.` ⇒ 类型引用，无歧义。实参挂到左边的 `IdentExpr.TypeArgs`
而**不新造 AST 节点**（每加一个节点类型，每个 walker 都要补分支，漏一个即静默跳过）。

**验证**：e2e golden interp 354 / jit 350、cross-zpkg 81、multi-exe 3、stdlib 340、
自举字节不动点 3/3、runtime `cargo test --lib` 1394、`lines` / `docs` / `walkers` / `diagcodes` 全绿。
**零回归**（与「全仓零使用」的预测一致）。

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

## 模式匹配携实参（2026-09-26，收尾刀）

核实的结论比「未核实」严重：**这是活的静默错值，而且 #831 让它变成了自相矛盾。**

```z42
object o = new Box<int>(1);
o is Box<string>                      // → false  ✅（#831 修的）
switch (o) { case Box<string> b: … }  // → **匹配上**，b 拿到装着 int 的 Box<string>
```

三种模式形态（类型模式 / 位置模式 / is-结构化模式）实测全部误匹配，属性模式同理。

**修前两侧是「都错但一致」**（`_emitIs` 与 `PatternEmitter._qual` 都用擦除名）；
#831 只改了前者 ⇒ 变成**同一个问题两个答案**。这条要写清楚：不是我引入了错误，
但我**引入了不一致**，而不一致比一致的错误更难查。

根因还是**两把尺子**：`TypeOpEmitter` 与 `PatternEmitter` 各写了一份
「`QualifyTypeName` + Array/Object 归一」。修法是收敛到 `ExprEmitter._typeTestName`
这**一个出口** —— 模式节点本就带着 binder 解析好的类型
（`BoundTypePattern.BoundType` / `BoundPositionalPattern.Type` / `BoundPropertyPattern.Type` /
`BoundAtPattern.Type`），**不需要新字段**。

⚠️ **既有用例 `pattern_generic.z42` 一直是绿的，因为它只测「正确实例化能匹配」。**
判别力全在阴性那半。新用例 `pattern_generic_identity.z42` 六格全是阴性形态，
并且**撤回修复本身验过它会变红**（不是只看它现在绿）。

指纹 21 → 22：改的是**发码**（模式路径上 `is_instance` 的目标名），含泛型模式的源文件
哈希不变而发码变。stdlib 里没有这种形状 ⇒ CI 守门只会漏判，手动 bump。
