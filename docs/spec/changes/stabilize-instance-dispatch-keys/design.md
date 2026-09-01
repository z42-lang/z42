# Design: 实例派发键稳定化 —— overload 键与多态槽解耦（.NET methoddef/vtable-slot 模型）

## 核心洞察：一个字符串键在扛两件互斥的活

今天 z42 用**同一个字符串键**同时承担两个语义完全不同的职责：

1. **overload 决议 / 注册标识**（编译期）——要能区分同名不同签名的重载，且**稳定于兄弟增删**
   （加一个重载不该改动别的方法的键）。这要求键 = **方法自身完整签名的纯函数**。
2. **多态派发槽**（运行期）——虚 / 接口 / 协议（`INumber.op_Add`、`IComparable.CompareTo`）/ foreach /
   委托调用，要一个 **base / 派生 / 实例化三方都同意的稳定槽**。这要求槽**对类型参数替换不变**——
   base `op_Add(T,T)` 与原始类型 override `op_Add(i32,i32)` 必须落同一槽。

这两个要求在泛型下**直接冲突**：把「替换后的完整签名」`op_Add$2$i32$i32` 当派发槽，就和 base 的
`op_Add$2$T$T` 对不上（VCall 落空 `expected object, got I64`）。上次全 mangle 尝试就是把两职责压在一个
键上、又只改了「注册键」没改运行期两条无-vtable 派发路径，于是挂 19 个 e2e golden。

**运行期实测（generics 排查确认）——两条互不相通的派发机制**：

- **对象接收者**：走 **vtable，槽键 = `$`-剥离后的简单名**（`types.rs:942 derive_simple_method_name`）；
  VCall 指令携带的是**裸源方法名**（`CallEmitter.z42:221,225` emit `c.MethodName`，**不是** RegKey）。
  → 全 mangle「注册键」对对象虚派发**透明**（简单名仍匹配）。
- **原始 / 装箱-struct 接收者**：**无 vtable**，靠**重建候选字符串**探 `module.func_index`——只试
  **裸 `Class.method`** 与 **`Class.method$arity`** 两种（`exec_vcall.rs:174-179 / 229-234 / 327-334`）。
  **从不试全类型-mangle 候选** → 注册成 `Std.Int32.op_Add$2$i32$i32` 的原始类型方法**直接不可达**。

## 设计（精炼版 2026-09-02）：primary 保基线键 / 非-primary 取全键 —— additive、最小辐射

> 借鉴 .NET（methoddef 做 overload 标识、vtable slot 做多态派发，二者独立）。但**关键洞察**：rekey 破坏
> 只发生在 **unique→overloaded 的跃迁**。故不必把每个方法都全 mangle（那正是上次回退把**唯一**的
> CompareTo/GetEnumerator/接口方法也 rekey → 5 子系统全炸的原因）。只需让**跃迁时既有那个不动**。

```
每个 (owner, name) 的重载集里：
  primary（声明序第一个）  → key = 「若它无同名兄弟时的基线键」（唯一方法 = 裸名；即今天的裸键）
                              —— primary 的裸名 = 多态/协议「规范槽」+ 旧调用方（seed）锚点
  非-primary（后续兄弟）    → key = MangleKey(全签名)  —— 新增/去除只动自己，不碰 primary
```

**关键性质：additive（对既有唯一方法零字节漂移）。**

- 一个原本唯一的 `IndexOf(string)` 保持裸 `IndexOf`（primary，= 今天）。新增 `IndexOf(char)` 只**新增**
  非-primary 全键 `IndexOf$1$char` → **primary 的裸 `IndexOf` 原样不动** → seed 的裸调用仍命中 → 零 rekey。
- **所有唯一方法**（含接口/泛型虚/协议 op_*·CompareTo/foreach 的 GetEnumerator·MoveNext·Current/委托目标——
  它们几乎都是唯一方法）**保持裸键、逐字节不变** → 上次挂的 **5 子系统天然不受影响**（不再是「先全 rekey 再
  逐个救」，而是「本就不动」）。
- 只有**真正的同名多重载**（少见，且多为非多态，如 `Substring` 系 arity 重载）的**非-primary 成员**取全键
  ——这是一次性迁移（格式 bump + 两代自举），此后加/删重载永久 additive。
- 这正是 String 补齐要的解锁；也把 E0436/E0433/first-wins 别名那类「键随兄弟集漂移」的补丁根因一次消除。

> **与上次「全 mangle」的本质差别**：上次 = 每个方法（含唯一多态方法）都 rekey → 5 子系统全断、冷 CI 打
> 地鼠。本方案 = 唯一方法（= 绝大多数多态方法）**不动**，只有非-primary 重载取全键 → 辐射面收敛到「非
> primary 重载的可达性」（prim 路径探全键 H1 + 非-primary 虚重载 vtable 槽 H4），是有界、可枚举的。

### primary 选取的确定性
primary = **声明序第一个**同名成员（跨 partial 碎片按碎片加载序，本就确定）。规则要求：**新增重载一律
追加在既有之后**（不得插到 primary 前）——否则 primary 易主 → 既有 primary rekey。此纪律写入 spec 场景
+ 由「本变更不改 z42c 源既有重载声明序」保证；stdlib 加重载（如 String 补齐）也追加式写。

### 旧「全签名 overload 键 + 裸规范槽别名」表述（保留追溯，已被上「primary/非-primary」精炼取代）
早先设想每个方法都登记全键 + 规范槽裸别名。精炼后：**primary 不必登记全键**（它就用裸键；全键是
非-primary 专属），省掉「每方法双登记」的字节膨胀与 primary 调用点 emit 漂移。规范槽 = primary 裸名。

### D1 调用点 emit：resolved 走全键，多态走裸槽

- **静态可决议的调用**（编译期知道实参类型、选定了具体重载）：emit **全键**
  `IndexOf$1$string` / `IndexOf$1$char`。含 prim-wrapper 实例方法（`String.IndexOf`）——调用点知道
  `"x"` 是 string、`'x'` 是 char，故能选定。
- **多态派发**（虚 / 接口 / 协议 / 泛型约束 `where T: INumber<T>` 里 `a.op_Add(b)` / `a.CompareTo(b)`，
  接收者是类型参数 `T`、编译期不知具体类型）：emit **裸规范槽** `op_Add` / `CompareTo`。base/接口只
  认规范槽；运行期按接收者真实类型落到该类型登记在规范槽别名下的实现。
- 由 `MemberResolver` / `CallEmitter` 依「调用是否 devirtualized / 接收者是否泛型参数 / 是否协议约束
  调用」二选一。今天 VCall 已发裸名（`CallEmitter.z42:221`）——多态路径**不变**；新增的是「已决议具体
  重载」路径改发全键。

### D2 运行期 method table + 两条无-vtable 路径（H1）

- **method table** 同时含：全键 `Std.String.IndexOf$1$string` → fn，裸规范槽别名 `Std.String.IndexOf`
  → 同一 fn（加载期按「规范槽」标志登记；别名 first-wins 需按稳定序，见 common-pitfalls §1）。
- **原始 / 装箱-struct 派发**（`exec_vcall.rs:159-198 / 204-252 / 321-349`）候选列表**增加全键形态**：
  今天只探 `{T}.{m}` / `{T}.{m}${arity}`；改为**优先探 VCall 携带的已决议全键**（若调用点发的是全键），
  再回落裸规范槽。→ H1 解除（原始类型的具体重载 + 协议方法都可达）。interp 与 **JIT
  （`jit/helpers/vcall.rs:126-132 / 205-211`）两处候选列表必须镜像同改**。

### D3 VM vtable 槽（对象接收者，H4）

- **规范槽**仍按裸简单名建（`derive_simple_method_name` 对规范槽方法保持裸名）→ 虚 / 接口 / 协议派发
  base·派生·实例化同槽，**替换不变**（H2 由「槽=裸声明名」天然解决）。
- **非规范的虚重载**（同名多个虚方法、各自独立多态槽——罕见，如 C# 允许 `virtual F(int)`+`virtual F(string)`）
  需**全键槽**：`derive_simple_method_name` 对这些保 `$`，`merge_with_base`（`type_registry.rs:213-254`）
  按全键 override 匹配。→ H4：非规范虚重载各占独立槽，不再塌。多数类型无此情况（每名一个虚方法）→
  常见路径零变化。

### D4 泛型专项（H2/H3/H5/H6）

- **generic arity 进键（H3）**：`MangleKey` 增编方法泛型元数 + **归一 type-param 名**（源 `T`/`U` → 规范
  `T0,T1…`，消除 alpha-rename 敏感 + `Bar<T>()` 与 `Bar()` 塌成 `Bar$0` 的碰撞）。`MemberCollector` 的
  arity-dup 预扫描同步按扩展签名。
- **composite 用 FQN（H5）**：`_compositeKeyName`（`OverloadResolver.z42:55-77`）的泛型构造子从短名
  `List` 改 `Fqn()`（`Std.Collections.List`），闭合与 `fix-type-ref-ns-collision` 同类的短名跨-ns 碰撞；
  import 侧（`ImportedSymbolLoader`/`ExportedTypeExtractor`/`DependencyIndex`）产出**逐字节一致**的
  composite 键。⚠️ 保留既有「裸内建 `int`→`i32` vs composite 内叶子 keyword 拼写」的非对称（否则跨包漂）。
- **键取自声明签名、不取替换后（H2/H6）**：overload 键用**声明层**签名（类型参数原样，不代入实参）；
  多态槽用裸名（更彻底地 erase 类型）。→ 键不依赖 `_substGeneric` 的 seed-vintage 行为
  （`MemberResolver.z42:305-311` 的 Unknown/unchanged 回落），消除 H6 的跨种子非确定性。
- **static-virtual op_*（H2 的最尖锐点）**：INumber `static abstract op_Add(T,T)` + Int32 `static override
  op_Add(i32,i32)`。二者的**规范槽都是裸 `op_Add`**（静态无 vtable，走原始路径裸候选）→ base 声明与原始
  override 同槽、替换不变 → 保持今天可派发。`MemberCollector` 的 `staticVirtual` 基线 carve-out（`:193-196`）
  **改为「登记全键 + 裸规范槽别名」**，而非留纯裸键——这样既稳定（加 op 重载不 rekey）又可派发。

### D4a 泛型/非泛型重载的调用决议优先级（H3 —— 「怎么匹配」）

**前提（键层已保证不覆盖）**：泛型方法即使与非泛型同名同值-arity，键也不同（泛型编 `$$<genArity>`，如
非泛型 `Bar$0` vs 泛型 `Bar$$1$0`），故二者都登记进 `ct.Methods`、不互相静默覆盖。**决议层**负责在候选集里
按优先级选对那一个。**z42 泛型不展开/不单态化**（一个泛型方法=一份函数体一个键，实参走 `method_type_args`
运行期携带，见 generics 投研 item 5），故决议只在**声明层签名**上做，实参代入仅用于「适用性判断/推断」。

对齐 C# ECMA §12.6.4「better function member」，落到现有 `OverloadResolver.Resolve`（适用集→most-specific
→歧义报错）之上，**新增泛型维度**。调用 `recv.Name(args)` 或 `recv.Name<TA…>(args)`：

1. **候选收集**：源名 `Name` 全部方法（泛型 ∪ 非泛型，含 base 链），按有效签名去重（沿用 `_collectOverloads`）。
2. **显式类型实参门**（调用带 `<TA…>`）：仅留**泛型且 `Decl.TypeParams.Count == |TA|`** 的候选（非泛型 +
   arity 不符者剔除）；用 TA 代入得具体形参类型再判适用。（`MemberResolver._applyMethodTypeArgs` 已读 `<TA>`。）
3. **推断门**（泛型候选、无显式 TA）：由实参→形参推断绑 `T`；推断失败 → 该候选不适用。代入推断结果后判适用。
4. **适用性过滤**（`_applicable`）：arity 匹配 ∧ 每实参可赋值到（代入后）形参类型。**泛型候选用代入后签名判**。
5. **Better-function-member 排序**（在 `_betterThan` / `Resolve` 的 most-specific 之上，按序 tie-break）：
   1. normal form ≻ params-expanded（沿用）；
   2. 逐参更具体：exact ≻ 加宽/装箱（沿用 `_betterThan`）；
   3. **平手①：非泛型 ≻ 泛型**（C# §12.6.4.3：Mp 非泛型、Mq 泛型 → Mp 更优）。→ `Bar()` 选非泛型 `Bar()`；
      `Foo(5)` 在 `Foo(int)` 与 `Foo<T>(T)` 并存时选 `Foo(int)`。判据：`ms.Decl.TypeParams.Count == 0`。
   4. **平手②：形参类型更具体者胜**（都泛型或都非泛型仍平时）——如 `Foo<T>(T)` vs `Foo<T>(List<T>)` 对
      `List<int>` 实参选后者（更具体）。
   6. 仍无唯一极小 → **E-ambiguous**（现有 `OverloadResult` code 2 报错），**不再静默覆盖**（今天塌 `Bar$0` 的坑根治）。
6. **emit**：选中非泛型 → 其 primary-基线/非-primary-全键；选中泛型 → 其**声明签名键 + `method_type_args`**
   （显式或推断的 T 绑定）；若走多态（虚/接口/协议约束接收者）→ 裸规范槽（同 D1）。

**与 primary/非-primary 键方案的衔接**：`Bar()`（非泛型）与 `Bar<T>()`（泛型）声明序在先者为 primary、占裸
规范槽 `Bar`（承接 seed 旧裸调用 + 多态）；另一个取带 `$$`/`$` 的全键。**决议第 5 步的 tie-break 决定「调用
点选谁」，与「谁占裸槽」正交**——seed 的裸 `Bar` 仍解析到 primary（声明序在先的那个），新调用点按 tie-break
选具体候选、emit 其精确键。

**实现落点**：`OverloadResolver`：`Resolve`/`_betterThan` 加 tie-break③④（读 `Decl.TypeParams.Count`）+ 泛型
候选代入后判 `_applicable`；`MemberResolver`：显式-arity 门 + 推断门接入候选集（复用 `_applyMethodTypeArgs`）。
**边角性**：`Bar()`+`Bar<T>()` 同值-arity 共存是否真出现于代码库，开工前 grep 证实——不出现则键层 `$$` 编码
可先作**守卫**（防未来），决议优先级规则仍按上定义（对 `Foo(int)`+`Foo<T>(T)` 这类**更常见**的泛型/非泛型
并存有用，属 C#-parity）。

### D5 协议名单统一

合并今天分歧的两份：`SymbolCollector.IsProtocolExempt`（6 名）与 `DependencyIndex._isProtocol`（4 名，
缺 `get_Item`/`set_Item`）→ 单一 SoT。这些名恒为规范槽裸别名（VM 硬查锚点 `dispatch.rs:146` /
`exec_vcall.rs` / `jit value.rs:145` 不变）。承接上次 Deferred「三处协议名单合并」。

### D6 迁移：additive → 减字节漂移 + 两代自举

- **注册侧 additive**：现有裸规范槽**保留**（不删、不改）→ seed 的裸调用跨代仍命中。**新增**的是各方法的
  全键 + 「已决议重载」调用点改发全键 → **emit 的 zbc 字节变**（调用点操作数从裸→全键），METHOD TABLE
  仍认裸别名 → 跨代 seed→gen1 解析不断。
- **格式 bump**：zbc + zpkg minor 双 bump（wire 布局不变，仅键字符串内容/新增别名条目）→ ci-bootstrap
  版本差 gate 触发**两代自举**整树重键（`design.md`[上次]§两代自举吸收重键、`fix-bootstrap-format-bump-deadlock`
  D7）。上次已证：全 mangle 在两代自举 + gen1==gen2 **通过**——迁移不是风险点，**运行期派发才是**。

### D7 上次回退的 5 子系统 —— 本方案逐一覆盖

| 子系统 | 上次挂法 | 本方案如何不挂 |
|--------|---------|----------------|
| 接口派发 | `Demo.Dog.Name not found`（裸 emit vs mangle 注册） | 接口方法=规范槽裸别名；对象 vtable 裸槽命中（D3） |
| 泛型虚 | `Num.CompareTo not found` | CompareTo=规范槽裸别名；对象走裸 vtable 槽、原始走裸候选（D2/D3） |
| 泛型-over-原始 | `expected object, got I64` | op_*/CompareTo 规范槽裸别名 + 原始路径探全键+裸回落（D2/D4） |
| 委托/事件 | `FuncRef` 不匹配 | 委托目标按规范槽/全键一致 emit（待 tasks 定位具体 emit 点） |
| foreach | `ArrayLen: expected array, got Object(Ring)` | GetEnumerator/MoveNext/Current 列为协议规范槽裸别名 |

### D8 反射

`MethodInfo.Name` 继续反向 demangle 全键 → 源级名展示（`reflection/methods.rs` 已 `split('$')`）；规范槽裸
别名对反射为 no-op。派发键与反射显示名正交（同上次 D-反射）。

## 风险与实现纪律

- **最高风险 = 运行期实例派发**（上次 19 挂点全在此、**冷 CI 才可见**）。tasks.md 把接口/泛型虚/泛型-over-
  原始/委托/foreach 的现有测试（generics 排查 item 7 已列全）列为**硬回归门**；interp 与 JIT 候选列表必须
  同批改（`jit/helpers/vcall.rs`）。
- **分阶段引入纪律**（bootstrap-seed.md）：本变更是 support（编译器认全键+裸别名、运行期认全键候选）；
  z42c 源码/stdlib **改用**新形态（如 String 补齐加 char 重载发全键）留**晚一个 nightly** 的独立下游 change。
  故本变更内 z42c 源不新增会 rekey 的重载。
- **本地不可验部分以 CI 为准**（proposal §验证现实）。

## Deferred（明确不在本变更）

- Layer 2（zpkg 体积/读性能：varint/窄索引、零拷贝 `&str`+mmap、跨包共享池）——独立 backlog，与本
  identity 层正交（investigation 已证 interning 现已部署，真杠杆是索引宽度+字节码+加载分配）。
- String 补齐（下游 change，本变更进 nightly 后落）。
- 非规范虚重载全键槽若辐射面证明过大，可退为「不支持同名多虚重载」并诊断报错（tasks 里作 fallback）。
