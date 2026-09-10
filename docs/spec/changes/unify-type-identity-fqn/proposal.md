# DRAFT: unify-type-identity-fqn —— 持久化的类型身份改用全限定名

> 状态：📝 DRAFT，待 User 裁决 | 创建：2026-09-10 | 类型：**lang/ir**（改 zpkg 已定义 section 字段语义 → 格式 bump）
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

### Phase 3 — 格式 bump

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
