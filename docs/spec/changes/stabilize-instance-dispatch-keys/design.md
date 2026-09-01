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

## 设计：解耦为「全签名 overload 键」+「裸规范槽」（additive，不 rekey）

借鉴 .NET（methoddef token 做 overload 标识；vtable slot 做多态派发；二者独立）。z42 用字符串表近似：

```
每个方法登记两个标识：
  ① overloadKey(m) = MangleKey(m.name, genericArity, paramTypes)   —— 全签名纯函数、稳定、区分重载
  ② slotName(m)    = m.name（裸）                                    —— 仅「规范槽」方法额外登记此别名

规范槽 = 每个 (owner, name) 里承担多态/协议派发的那一个方法：
  · 虚方法链的 virtual origin（首次声明 virtual/abstract 的那层）
  · 接口方法 / 协议方法（INumber.op_*、IComparable.CompareTo、IEnumerable.GetEnumerator …）
  · 协议豁免名（ToString/Equals/GetHashCode/GetType/get_Item/set_Item）
  · 非虚且无同名兄弟时：该唯一方法自身（= 今天的裸键，保持不变）
```

**关键性质：additive（增量、不 rekey）。** 一个原本唯一的方法 `IndexOf(string)` 今天登记裸 `IndexOf`；
本方案下它登记**全键 `IndexOf$1$string` + 裸规范槽别名 `IndexOf`**。新增 `IndexOf(char)` 只**新增**全键
`IndexOf$1$char`（非规范槽、无裸别名）。→ **现有方法的裸规范槽 `IndexOf` 原样保留** → 上一 nightly seed
的裸 `IndexOf` 调用仍命中 → **零 rekey、零 bootstrap 破坏**。这正是 String 补齐要的解锁；也把 E0436/
E0433/first-wins 别名那一整类补丁的根因（键随兄弟集漂移）一次性消除。

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
