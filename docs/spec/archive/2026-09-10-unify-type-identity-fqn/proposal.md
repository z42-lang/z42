# DRAFT: unify-type-identity-fqn —— 持久化的类型身份改用全限定名

> 状态：🚧 IMPL 中（User 已批准方案 A：修根缺陷）| 创建：2026-09-10 | 类型：**lang/ir**（改 zpkg 已定义 section 字段语义 → 格式 bump）
> 取代：`fix-qualified-type-name-unknown`（B3-产出端的窄修方案，因埋坑被否，见 §1.3）
> 出处：[[restore-emit-zbc-diagnostics-program]] 欠债表 **B3-产出端**，追根因后升级为本 change。

---

## 1. 问题

### 1.1 根缺陷：持久化的类型身份是**短名**

zpkg 的 SIGS（方法 ret/param 类型）与 TYPE（字段/属性/接口签名类型）两段，存的类型名是**短名**
（`FieldInfo`、`List<int>`）。**短名不是跨命名空间的唯一键**——`A.Foo` 与 `B.Foo` 写进去都是 `Foo`。

消费端 `ImportedSymbolLoader._resolve`（`ImportedSymbolLoader.z42:496-580`）按短名查
`r.Classes` / `r.Interfaces` / `r.EnumTypeNames`，谁赢了短名竞争就绑谁。这是**静默错绑**。

### 1.2 退化形态：限定名直接写成 `"unknown"`（= 欠债表 B3）

两个发射端在**调用解析器之前**短路，对含点的限定名返回哨兵：

| 发射点 | 代码 | 写进哪 |
|---|---|---|
| `FunctionEmitter._sigTypeName` | `FunctionEmitter.z42:222` | SIGS |
| `ClassDescBuilder._typeSourceName` | `ClassDescBuilder.z42:26` | TYPE 段字段类型（`:150`）、属性（`IrGenMemberEmitter.z42:92`）、接口方法签名（`:381/:390`） |

两处注释都写「镜像 C# `MemberType` → `_` → `"unknown"`」——**在复刻一个 2026-06-26 已删除的编译器的缺陷**。
`SymbolTable.ResolveTypeP`（`SymbolTable.z42:335-400`）**完全有能力**解析限定名，只是没被调用到。

**受害面已实测**：全仓「限定名在类型位」的声明 14 处，**全部在 `z42.core/src/Reflection/`**
（`MethodInfo` / `FieldInfo` / `PropertyInfo` / `ParameterInfo` 的 `Std.Attribute[] __attrCache`、
`GetAttribute(Std.Type)`、`Std.Type FieldType`，加 `Activator.CreateInstance(Std.Type)`）。
⇒ 「反射六大成员跨包全坏」得到独立印证。后果两条：类型检查被 Absorb 吞掉；`FieldType.Name == "unknown"`。

### 1.3 为什么不能只窄修 B3（否掉的方案）

窄修 = 删那两处短路、写**短名**。它把这 14 处从「没信息」变成「短名竞争的赢家」——
**在最需要精确类型的反射 API 上，新增一次静默错绑的可能**。这是把 §1.1 的根缺陷又固化一次。
而 producer 侧**无法自行判定歧义**（包 P 发射时看不见未来的消费者 Q 会引入哪个同名类）
⇒「不歧义写短名、歧义写 unknown」也不成立。**唯一无坑的持久化形式是 FQN。**

---

## 2. 为什么 FQN 是对的（三条硬证据，不是审美）

### 2.1 运行期本来就是 FQN 键，且它的短名回退是防歧义的

`make_type_from_name`（`src/runtime/src/corelib/reflection/type_object.rs:55`）顺序：
剥 `[]` → 解析 `<>` → **`m.type_registry.get(name)` / `ctx.try_lookup_type(name)`（FQ 查找）**
→ 仅当名字**不含点**才退到 `resolve_dotless_simple`，而那条路的注释明写：

> Resolve by unique simple-name match against loaded types (**never mis-binds: zero/ambiguous → synthetic**)

⇒ 给 FQN，运行期直接命中，**一行都不用改**。

### 2.2 运行期正在为「只有短名」付性能代价

同函数的短名回退里：

> a class-like name … may live in a not-yet-loaded package whose FQN we can't derive
> (the loader indexes namespaces, not type names — no simple→FQN map).
> **Force-load all remaining packages ONCE**, then retry.

⇒ FQN 化后这条路对这些 tag 变成死代码。**这是收益，不是成本。**

### 2.3 消歧数据今天就在手上

- 本地类：`StubCollector.z42:245` 已设 `ct.Namespace = ns`。
- 导入类：`em.Namespace` 在注册点就有（`ImportedSymbolLoader.z42:133/168/190/209`，
  已存进 `ClassNamespaces` 与 `ClassNsAll`）。

> 🔴 **但 `Z42ClassType.Namespace` 在导入侧从未被设置**：`ImportedSymbolLoader.z42:138` 是
> `new Z42ClassType(cl.Name, hasBase, baseName)`，全编译器给该字段赋值的点**只有** `StubCollector.z42:245`。
> 而 `Z42Type.z42:38-39` 的注释声称「导入由 ImportedSymbolLoader 从 em.Namespace 设置」——**该注释是假的**。
> 又一条「没有东西盯着的断言变成谎言」。本 change 必须把它补上**并加门**（§6）。

---

## 3. 目标形态：持久化类型名文法（新 SoT）

```
typeform := prim_keyword                                   // int / long / string / bool / char / object / void / byte …
          | fqn                                            // 用户声明的 class / interface / enum；ns=="" → 裸名
          | fqn "<" typeform ("," typeform)* ">"           // 构造泛型
          | typeform "[]"                                  // 数组
          | typeparam_name                                 // T（裸名，不变）
          | "unknown"                                      // 哨兵：**真**解析不出来
```

**裁决点（本 DRAFT 请 User 确认这三条）**

- **D1 — 基元保留关键字拼写**（`int` 不写成 `Std.Int32`）。理由：基元无 ns、本就无歧义；
  运行期 `primitive_fqn` 已把两套词汇表（`int` / `i32`）都映到 FQN；且 SIGS 对
  `byte/sbyte/short/ushort/uint/ulong` 刻意保源拼写（G18g）——动它是无关的额外漂移。
- **D2 — 只 FQN 化「类名成分」**，泛型实参递归套用同一文法，**不统一 SIGS 与 TYPE 现有的
  实参拼写差异**（今天 SIGS 走 `Z42InstantiatedType.Name()`、TYPE 走 `_typeSourceName` 源拼写）。
  统一那两套词汇表是**另一个** change，混进来会让本次漂移面无法对账。
- **D3 — `"unknown"` 哨兵保留**，语义收窄为「真解析不出来」。消费端
  `ImportedSymbolLoader.z42:508` 还原成 `Z42UnknownType` 的逻辑（#523 修的）**不动**。

---

## 4. 落地

### Phase 0 — 实证清点（**先做，不许靠推理**）

从当前已建全部 zpkg 里 dump SIGS/TYPE 的**全部去重类型名串**，得到今天真实的词汇表；
据此写死 old→new 映射表并作为对账基线。

> ⭐ 依据项目既有教训：**别再手写近似语料枚举**（[[restore-emit-zbc-diagnostics-program]] 坑 1）。
> 我在本 DRAFT 的第一轮扫描就因正则要求标识符小写开头而漏掉 `public Std.Type FieldType;`（10 → 14）。

### Phase 1 — 产出端

1. `ImportedSymbolLoader.z42:138` 起，导入类/接口/enum 建型时设 `.Namespace = em.Namespace`。
   ⭐ **一行改动**：`:168` 紧邻处已在写 `r.ClassNamespaces.Put(clKey, new StrBox(em.Namespace))`，
   同处加 `nct.Namespace = em.Namespace` 即可。且该处注释写明的种子 ABI 约束是
   「新字段不得进 ctor 签名、须构造后赋值」——`Z42ClassType.Namespace` 正是这形态（ctor 内默认 `""`），
   **无自举越界**（[bootstrap-seed.md](../../../.claude/rules/bootstrap-seed.md) 轴 ②）。
2. `FunctionEmitter._sigTypeName`：删 `:222` 短路；对解析出的 class/interface/enum 用 `Fqn()`。
3. `ClassDescBuilder._typeSourceName`：`:26` 的 `_hasDot` 分支改为经 `this._g._symbols` 解析后取 `Fqn()`。
   ⚠️ **不能用「截末段」**：嵌套类型注册键是 `Outer+Inner`，截末段得 `Inner` = 错的。必须走解析器。
   ⚠️ prim 别名的源拼写策略（G18g）不动。

> 🔴 **IMPL 期发现的范围增量（2026-09-10，写代码时才浮现，DRAFT 初稿低估了）**：
> 「设 `.Namespace` + 用 `Fqn()`」只对 `Z42ClassType` 现成可用，另外三类载体还缺能力：
>
> | 载体 | 缺什么 | 要做 |
> |---|---|---|
> | `Z42InterfaceType` | **完全没有 `Namespace` 字段**（`Z42Type.z42` 该类里无此槽） | 补 `Namespace` + `Fqn()`，本地 `StubCollector` / 导入 `ImportedSymbolLoader:186` 两侧都要设 |
> | `Z42InstantiatedType` | 没有 `Fqn()`；`Name()` 用 `Def.Name()`（短名）拼 | 补 FQN 组合：`Def.Fqn() + "<" + args…Fqn() + ">"` |
> | imported enum | `Z42ClassType.Enum(name)` 在 `_resolve:536` 现场造，不带 ns | 建型处补 ns（数据在 `r.ClassNamespaces`） |
>
> ⇒ Phase 1 拆成三个 commit（class / interface+instantiated / enum），**class 先行**——
> B3 的实际受害者（`Std.Attribute` / `Std.Type`）全是 class，先落地就能拿到可验证的差分。

### Phase 2 — 消费端

`ImportedSymbols` 增 FQN 键视图（对称 `SymbolTable.ClassesByFqn`），`_resolve` 改 **FQN 优先**。
> ⚠️ **不留短名兜底当兼容层**（philosophy「不做兼容」）——格式 strict-pin，产物每次从源码全建，
> 不存在读旧 zpkg 的需求。短名路径只保留给**本就无 ns 的东西**（型参、prim）。

### Phase 3 — 格式 bump ❌ **User 裁决：不做**（2026-09-10）

`zpkg` minor **0.43 → 0.44**（判据：[version-bumping.md](../../../.claude/rules/version-bumping.md)
「已定义 section 字段语义变化」；先例 `stabilize-instance-dispatch-keys` 同为「wire 布局不变、仅键字符串」也 bump）。
按该文件 checklist 同步 4 处版本常量 + changelog + 二进制 fixture，走两代自举
（[[two-gen-bootstrap-regressed-blocks-format-bumps]] 已修，实测 5+ 次真实 bump 全绿）。

---

## 5. 风险

| 风险 | 判断 |
|---|---|
| **原生 ABI 受影响** | ❌ **不成立（已核实）**。`Z42FieldDesc.type_name` / `Z42TypeDescriptor_v1` 是 **native → VM 注册**方向（`z42-macros/src/methods_attr.rs:239` 由宏从 Rust 生成），不是 VM 把编译器发射的名字交给 native。 |
| 运行期需要改 | ❌ 不需要（§2.1）。**若发现需要改，说明文法定错了，回来重定** |
| 全量 golden / zbc fixture 漂移 | ✅ 预期且必然。按 [[worktree-green-false-signals-stale-xtask-and-zbc-regen]] 只 regen 真受影响的 |
| 自举不动点 | 两代都用新编译器 ⇒ gen1==gen2 仍应成立。**必验 3/3** |
| 反射行为变化 | 正是目标。若有测试**断言**了 `"unknown"` 或短名 → 那是把缺陷写进了断言，改测试 |
| 泛型/嵌套下 `Fqn()` 产串异常 | ✅ **已排除（已核实）**：`needsMangle = TypeParams.Count > 0 && multiArityMap.ContainsKey(Name)`（`StubCollector.z42:193`、`ImportedSymbolLoader.z42:128/231`）—— **只有同名多 arity 并存才 mangle**。普通泛型 `List<T>` → `IrName()=="List"` → `Fqn()=="Std.List"`；真 mangle 时注册键本来就是 `Foo$1`，`Fqn()` 产的正是注册键本身。⇒ **`Fqn()` ≡ `Ns.` + 注册键**，与运行期 FQ 查找形态严格一致 |

---

## 6. 验证（每条都要"会变红"）

1. **退回对照**：撤 Phase 1 → 差分用例必红；不撤 → 绿。两侧必须同源（[[verify-conclusion-after-reseeding]]）。
2. **跨 ns 同短名真门**：两个包各声明一个同短名类，跨包引用后断言绑到正确那个。
   ⚠️ 门必须建在 `src/tests/cross-zpkg/`（走 `z42c build`，诊断可见），**不能放 golden**。
3. **反射门**：跨包读 `FieldInfo.FieldType.Name`，断言 ≠ `"unknown"` 且 = 真名。
4. **防腐门**：断言「每个 imported `Z42ClassType` 的 `Namespace` 非空」——
   §2.3 那条假注释正是因为**没有东西盯着**才烂掉的。
5. GREEN 全绿 + 自举不动点 3/3 + `xtask test stdlib --mode jit`（本地 `xtask test` 只跑 interp，
   见 [[local-green-misses-jit-and-lines]]）。

---

## 7. 不做

- 不统一 SIGS 与 TYPE 的实参拼写差异（D2，另开 change）。
- 不动 `StubEmitter._typeSpell`（extern 桩路径不经 `_resolve` 消费，今天没坏）。
  ⚠️ 记忆里称它是「正确样板」是**误导**——它是源拼写原样返回，不是解析后取名。
- 不动基元关键字拼写（D1）。


---

## 8. IMPL 实录（2026-09-10）——**与 DRAFT 初稿的出入，以此节为准**

### 8.1 Phase 0 的方法换了两次，前两次都不合格

| 方法 | 为什么废弃 |
|---|---|
| 源码侧正则扫描「限定名在类型位」 | **三次低估同一个数字**（10 → 14 → 仍漏 `Std.Reflection.FieldInfo[]` 这类多段限定名）。手写近似枚举不可靠，项目已有明文教训。 |
| zpkg STRS 字符串集合差分 | 对本改动**不敏感**：`Std.Attribute` / `Std.Type` 本就在池里（作类注册名等用途），槽位从短名变 FQN 时集合无变化。 |
| ✅ **运行期探针**（最终采用） | 直接观测反射面看到的字段类型名，既是基线也是回归门。实测 z42.core 四个反射类 **37 个字段中 9 个是 `unknown`**，修后 9 → 0、全差分恰好 9 行零回归。 |

### 8.2 根缺陷（DRAFT 初稿完全没预见到，User 裁决按方案 A 修）

**裸名类型引用不看引用方所在的命名空间。** `SymbolTable.Classes` 按裸名键、同短名跨 ns
first/last-wins ⇒ `namespace Alpha` 里写的 `Widget` 会绑到 `Beta.Widget`。
`fix-type-ref-ns-collision` 当年只根治了**限定**引用，裸名这半边留到今天 —— 以前没人把解析结果
持久化，所以只表现为运行期降级成无句柄合成类型；本 change 一旦写进元数据就变成**自信的错答案**。

修法：`SymbolTable.WithAliases` 本就是 per-file 视图（共享只读表 + 文件私有字段），在同层加
`ScopeNs`，并把散在 4 处的 `WithAliases(BuildAliases(cu))` 收敛成 `WithCu(cu)`。因
`model.Symbols` 就是该视图，**类型检查器与发射端从此共用同一份解析结果**。

### 8.3 范围增量（初稿低估）

`Z42InterfaceType` **连 Namespace 槽都没有**；`Z42InstantiatedType` 没有 `Fqn()`；
imported enum 现场建型不带 ns。三者都补齐。

### 8.4 本 change 自己引入、被既有测试抓到的两个回归

| # | 现象 | 抓到它的测试 |
|---|---|---|
| 1 | enum 的 FQN 被当成**名字**传给 `Z42ClassType.Enum` ⇒ `Name()` 变全限定串，同一 enum 判成两个类型。**`Name()` 是全仓等值比较口径，FQN 只能进 Namespace** | `heap_retention` |
| 2 | `_sigArgTypeName` 误用 `PrimModel.SurfaceName`，把泛型**实参** `Int32` 改写成 `int` ⇒ **违反本 change 自己的裁决 D2** | `generic_struct_array_cross_pkg` |

两者都**只在 #550 打开 `--emit-zbc` 诊断门之后才可见**。

### 8.5 DRAFT 初稿里被推翻的判断（勿再引用）

- ❌「原生 ABI 受影响」—— `Z42FieldDesc` 是 **native → VM 注册**方向，不受影响。
- ❌「`StubEmitter._typeSpell` 是正确样板」（这条源自记忆）—— 它是**源拼写原样返回**，会写出 FQ 串，
  不是解析后取名，别照抄。
- ✅ 已排除：`Fqn()` ≡ `Ns.` + 注册键（`needsMangle` 只在同名多 arity 并存时为真）。

### 8.6 ✅ 已补：真歧义面不再写「选了赢家」的 FQN

**真歧义面（两个 `using` 都提供同一短名）目前仍会被写进「选了赢家」的 FQN。**
A1 只解决「外围 ns 能解析」那一类。E0456 的 11 个调用点全在语句/表达式位，
**声明位（字段/形参/返回类型）一个都没有** —— 判据在 `TypeChecker`（有 `_currentUsings`），
而声明位的类型检查在 collector 阶段（拿不到 usings）。
**已落地**：per-CU 视图补 `ScopeUsings`；判据抽成 `SymbolTable.IsBareNameAmbiguous`
（外围 ns 声明该名 → 不算歧义；否则 ≥2 个**可见** ns 声明 → 歧义）。两个发射端在写 FQN 前先过这道
判据，歧义时**退回短名** —— 诚实降级，与本 change 之前一致，绝不写自信的错答案。

⚠️ **判据只此一份**：`TypeChecker.ChkAmbiguousBareName`（E0456）改为复用同一方法，不再各判各的。
这一步有风险（若某调用点传的 `symbols` 不是 `WithCu` 视图，E0456 会**静默失效**），
但 #550 为 E0456 留的三条单测替这件事把关 —— GREEN 实测三条全 PASS，守卫仍会响。

> **A3（E0456 补到声明位）拆为独立 Deferred**：判据在 TypeChecker（有 usings），而声明位的类型
> 引用检查在 collector 阶段（拿不到 usings），且 collector 诊断可见性另有历史包袱。
> 本 change 的守卫已保证「不写错答案」；让歧义声明**报错**是诊断完备性问题，独立立项。


---

## 9. 格式 bump 的裁决（2026-09-10，User）

**结论：本 change 不 bump zbc/zpkg minor。**

DRAFT §4 Phase 3 原计划 bump（zbc 1.38→1.39 / zpkg 0.43→0.44），依据是
[version-bumping.md](../../../.claude/rules/version-bumping.md) 的「已定义 section 字段语义变化」
与 `stabilize-instance-dispatch-keys`（同为「wire 布局不变、仅键字符串」）的先例。IMPL 期两条新事实
翻转了这个判断：

1. **实测双向互操作都正确，不 bump 不会炸**：旧 VM 读新 zpkg 正确（`make_type_from_name` 本就
   FQ 优先）；新编译器读旧 zpkg 正确（短名路径保留）。bump 的理由是条文与「不许倚仗回退路径」，
   不是「不 bump 会坏」。
2. **带 bump 就无法本地全绿**：格式常量住在 `z42.ir`（stdlib 库），z42c 运行时用的是**已编译好的
   那份** ⇒ gen1 只能产出「旧格式外壳 + 新常量」、gen2 才写新格式，且两代都必须在能读旧格式的 VM
   下跑。本地实测两轮均失败，第二轮把 in-tree 弄成新旧混用（已清理）。
   [bootstrap-seed.md](../../../.claude/rules/bootstrap-seed.md) 记的正是这条：格式 bump 的两代自举
   由 CI 的 `ci-bootstrap` 版本差 gate 自动完成，**本地是环境墙**。

⇒ 与 workflow「全绿才能开 PR」直接冲突。User 裁决：**不 bump**，保住本地全绿这条硬约束。

**留下的代价（明写，不掩饰）**：新旧产物可静默混用（各自靠回退路径读对方）。
若将来要收紧，应作为独立 change 走 CI 两代自举，那时才 bump。

**顺带修的两处腐坏（与 bump 无关，保留在本 PR）**：
`zbc.md` 的 changelog 缺 **1.38**、`zpkg.md` 缺 **0.43**、且 zpkg.md「当前版本」写着 `minor=42`
（实际 43）—— 上一次 bump 漏写 changelog，与 version-bumping.md 自己记的「1.37→1.38 漏 fixture」
是同一次事故的另一半。已补录并标注。

---

## 10. 收尾：跨包限定名解析（本 change 的最后一个洞）

**症状**：源码写**限定的导入类型**（`Demo.FqnBeta.Widget x;`）时仍绑到短名竞争的赢家。
实测 `Both.fromBeta` 被写成 `Demo.FqnAlpha.Widget` —— 又是「自信的错答案」，
且我的歧义守卫管不到它（守卫只判**裸名**，含点即视为已消歧）。

**根因（比预想深一层）**：`ImportedSymbolLoader` 里**整个建型过程**都罩在裸名 first-wins 守卫
`if (!r.Classes.ContainsKey(clKey))` 内 ⇒ 同短名的第二份**压根不建型**，FQN 视图自然也拿不到它。
所以光给 `_mergeImports` 加 FQN 合并是不够的 —— 源头就没有第二份。

**修法**：把守卫**收窄到只裹裸名表**。每份都建型、按 FQN 各存一份；`r.Classes` /
`r.ClassNamespaces` / `r.Constraints`（都以裸名 `clKey` 为键）仍 first-wins。
⚠️ `r.Constraints` 那条容易漏 —— 守卫收窄后必须显式跟上，否则第二份会覆盖赢家的约束。
再加 `SymbolCollector._mergeImports` 把 `imported.ClassesByFqn` / `InterfacesByFqn` 并进符号表。

> ⭐ **这个洞此前修不了**：修它要算导入类的 FQN，而导入类的 `Namespace` 恒为空 —— 本 change
> 才补上。（并发会话在记忆索引里把「imported 类不进 ClassesByFqn」列为剩余项最高优先级，
> 随本 change 一并关闭。）

**门**：`type_identity_fqn` 加 `Both`（限定引用两个导入包的同短名类）。
判别力已验证 —— 修前该格实测输出 `Demo.FqnAlpha.Widget`（错），修后 `Demo.FqnBeta.Widget`（对）。

**过程中我自己犯的两个错**（留档，别再犯）：
1. 扩展门时引用了一个**根本不存在**的类（`Both` 只在 scratchpad 的 fixture 里有），
   `undefined type: Both` 是字面属实，还级联把另外两条也带红了。
2. **两次手工只编 main、不编依赖**去复现，得到无效结论。
   有效手段是**用 harness 做变量隔离**（只撤门的扩展、保留代码修复），一步分清是哪一半的问题。
