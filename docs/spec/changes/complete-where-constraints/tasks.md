# tasks — complete-where-constraints

> 状态：🟡 IMPL 进行中（gate 已过 2026-09-05）| 创建：2026-09-05 | 类型：lang（完整流程）
> 图例：⚪ 未开始 · 🟡 进行中 · 🟢 完成 · ⛔ 阻塞
>
> **分支/worktree**：`complete-where-constraints` @ `../z42-whereconstraints`（基于 origin/main）
>
> **落地形态**：本 change 分 **4** 个 PR（PR-0 已撤销并进 PR-1）。PR-A 与 where 语义无关、
> 有独立价值，可先行合并；PR-1/2/4 是主体。PR-3 / PR-5 **不在本轮**（见 design §6）。

---

## Phase 0 — 测试机制（PR-0）🟢 已撤销

> **撤销理由（2026-09-05 IMPL 期实测）**：机制已存在且已在 GREEN gate 内——
> `SemanticDump.FirstErrorCode/FirstErrorMessage/ErrorCount` + `[Test]` units，由
> `xtask test compiler`（stage 5）驱动，活例 `undefined_type_tests.z42`。且初稿选的落点
> `xtask_test_dist.z42` **不在** `xtask test` 的 stage 表里（`test dist` 是发行版专用子命令）。
> 详见 design §4 PR-0 段。

- [x] 0.1–0.3 ~~sidecar 机制~~ **撤销**：不另造第二套，用既有 `SemanticDump` + `[Test]`
- [x] 0.4 `src/tests/README.md` 第 44 / 73 行指向已删的 `src/compiler/z42.Tests/Fixtures/`
      → 改为指向 `SemanticDump` 单测体系（由 1.11 一并落地）
- [x] 0.5 ~~自举一条现有 fixture~~ **撤销**：`undefined_type_tests.z42` 已是活例，无需再证
- [x] 0.6 跨包 / 多文件负例门（真正仍缺的那部分）登记 Deferred
      `where-constraint-future-crosspkg-negative-gate`（design §7）

## Phase A — 修 ZbcReader 漏读 bit2（PR-A，独立 bug fix）🟢

- [x] A.1 `ZbcReader.z42` TYPE 约束段：抽出 `_readConstraintBundle(c, pool)`（对称 Rust
      `read_constraint_bundle`），补读 `bit2 base u32` + `bit6 funcSig`，与
      `ZpkgReader._skipConstraintBundle` 的布局对齐。bit2/bit6 载荷**读而不存**——
      `IrConstraintDesc` 无承载位，加字段属 PR-3；本函数职责是对齐字节流
- [x] A.2 回归用例：`tests/zbcreader/constraint_bundle_tests.z42`（5 条）。
      **不走往返**——writer 今天只写 bit3，往返永远走不到 bit2/bit6 路径，正是它长期漏读
      无人发现的原因。改为按格式权威手拼 TYPE 段字节喂 `ZbcReader.ReadTypeAt`，并断言
      **bundle 之后**的字段（类级 Interfaces）作错位探针
- [x] A.3 **变异验证**：删掉 bit2 读取那一行 → 恰好 2 条 bit2 用例变红
      （`expected 1 but got 2` / `expected T but got Base`），其余 3 条仍绿 → 用例真能抓回归
      且无过度耦合；随后恢复
- **GREEN**：`xtask test compiler` 全绿（含自举字节不动点 gen1==gen2 3/3）
- **注**：这条是 PR-3 的硬前置；本轮即使不做 PR-3 也应先修（格式契约单侧退化）
- **踩坑记录**：手拼 TYPE 段最初漏了**对象全字段布局块**（`(Flags & 116) == 0` 时恒存在，
      非 gated），5 条用例全 OOB。写字节级 fixture 前必须把 `ReadTypeAt` 读到尾

## Phase 1 — 接口约束（PR-1，主体）🟢

### 1a 重构先行（单独 commit `05c00aeb`）
- [x] 1.1 合并 `_fillBundle` / `_fillBundleM` 为一份（两者此前逐字重复）
- [x] 1.2 合并 `_checkBundle` / `_checkBundleM`（差异仅 Span 与 owner 名 → 参数化）；
      顺带 `_isParam` 并入 `_indexOfName`、`_err`/`_errAt` 合一。265 → 215 行
- **GREEN**：✅ 纯重构，行为不变，字节不动点 gen1 == gen2 3/3

### 1b 接口约束（功能 commit）
- [x] 1.3 `ConstraintBundle` 加 `InterfaceNames[] / InterfaceCount` + `AddInterface`
- [x] 1.4 **去掉 `nt.ArgCount == 0` 过滤**；按裸名分流：`HasInterface` → 接口约束，
      `HasClass` → base-class 约束（base 分支保留 ArgCount 过滤：泛型 base 不在本轮）
- [x] 1.5 `_checkBundle` 加接口分支 → `SymbolTable.Implements`；`_satisfiesInterface` 对
      `Z42GenericParamType` / `Z42ErrorType` / `Z42UnknownType` **按满足处理**（否则泛型类
      内部转发 `Outer<T>` → `new Box<T>()` 每处都误报、且与既有诊断叠加二次噪声）
- [x] 1.6 **接口继承闭包**：`Z42InterfaceType` 加 `BaseNames/BaseCount`（StubCollector 从
      `c.Bases` 填，裸名）+ `Implements` 改两层遍历（类 base 链收直接接口 → 再沿父接口链 BFS）
      + 新增 `InterfaceDerivesFrom`（接口自身作实参时用）。**代价远低于预期**，未走退路

### 1b′ 顺带修：实例调用路径的方法级校验整条静默（**Scope 扩展，User 已裁决**）
- [x] 1.12 实测发现 `this.m<T>()` 路径上**方法级 where 约束与既有 arity 校验 E0445 双双不发**：
      `_bindCall` 兜底对成员调用传 `ms = null`（`MemberResolver.z42:428`），
      `_applyMethodTypeArgs` 里 `ms != null && ms.HasDecl` 恒不成立。注释「无本地 Decl →
      仅解析、不校验」只对 imported / prim 成立，**同类实例方法明明已解析出 ms、只是没往外透**
- [x] 1.13 修：`_bindInstanceMemberCall` 增 `call` 参数，用户类 / 泛型实例化受者两个分支带真
      `ms` 调 `_applyMethodTypeArgs`（幂等，兜底自动早退）。**刻意不改重载决议**——把
      `_resolveOverload` 的 typeArgCount 从 0 改成 `call.TypeArgCount` 会改变重载选择、
      撼动 codegen 与自举不动点，属另一件事
- [x] 1.14 同步改掉 428 那条**已不成立**的兜底注释（它正是把缺口伪装成设计意图、躲过历次 review 的原因）

### 1c warning 探针 → 翻 error（User 裁决）
- [x] 1.7 以 **warning** 跑了**两轮**全仓探针：① 同包路径；② 加上 1b′ 的实例调用路径
- [x] 1.8 对账结果：**两轮新增诊断均为 0 条**，`E0445` arity 亦 0 条，`xtask test` 全绿
- [x] 1.9 ~~给 `struct P` / `struct Tagged` 补 `: IEquatable<>`~~ **本轮无需**——该预判建立在
      「`Dictionary<int,int>` 会被校验」之上，而**跨包约束本轮 100% 不校验**（见下）。
      留给 PR-5
- [x] 1.10 清单归零 → **翻成 error**
- [x] 1.11 负例单测 `tests/typecheck/constraint_tests.z42`（13 条，与 `typecheck_tests.z42`
      平级、扁平、单一职责）：接口满足/违反、**泛型接口**满足/违反（打 ArgCount 过滤）、
      继承闭包两层/三层、**闭包不过度放宽**、基类链接口、型参转发不误报、多接口 `+`
      满足/违反、方法级满足/违反
- **GREEN**：✅ `xtask test` 全绿（10 stages）+ 13/13 负例 + 字节不动点 3/3

> ### ⚠️ 「零误报」的真实覆盖面（必读，防下一轮误判）
> design §6 已定性**跨包约束今天 100% 不校验**：`symbols.ClassConstraints` 的唯一写入点是
> `ConstraintChecker.Resolve`，只遍历本包 CU 的 `ClassDecl`；导入类型走
> `ZpkgReader → TsigReconcile → ImportedSymbolLoader` 全程不碰它 → `Check` 第一行
> `HasConstraints` 即返回。
>
> ⇒ 本轮探针覆盖的**只有同包约束**。风险表那条 🔴「基元 wrapper 归一偏差 →
> `Dictionary<int,int>` 编不过 → 自举链断」**根本没被触及**，它要到 PR-5 跨包持久化才浮现。
> **切勿把本轮的零误报读作「wrapper 归一没问题」。**

## Phase 2 — `enum` + `new()`（PR-2）🟢

- [x] 2.1 `TypeParser._parseConstraint` 补 `enum` special。**根因在 parser 而非语义层**：
      `enum` 此前落进 `_parseType()` 兜底 → `NamedType("enum")` → 语义层查不到同名类/接口
      → 静默丢弃。与接口约束同一种死法，只是死在更早的阶段
- [x] 2.2 `WhereConstraint.Special` 注释同步取值（`new()` / `class` / `struct` / `enum`）
- [x] 2.3 `ConstraintBundle` 加 `RequiresEnum` / `RequiresCtor`（含 `IsEmpty` 同步）
- [x] 2.4 `enum` 判定 → `symbols.EnumTypes.ContainsKey`；未解析型参 / error / unknown
      按满足处理（同 `_satisfiesInterface` 的口径）
- [x] 2.5 `new()` 判定**照抄运行期 `generics.rs::type_has_no_arg_ctor`**（design §1 铁律：
      运行期是 SoT，不另立规则）：基元满足；类须非 abstract；**完全没有显式 ctor = 默认构造
      = 满足**，只有「声明了 ctor 却无一能零实参调用」才不满足。「能零实参调用」的判据**复用
      `ConstructTyper.z42:186-193` 已有规则**（params 变长 / 尾参全带默认值都算），不另写一套
- [x] 2.6 负例用例 **9 条**：enum 满足 / 违反(class) / 违反(基元)；new() 无显式 ctor 满足 /
      显式无参 ctor 满足 / 只有带参 ctor 违反 / 形参全默认满足 / 基元满足；`struct + new()` 组合
- **GREEN**：✅ `xtask test` 全绿 + 22/22 负例 + 字节不动点 3/3

> **自举纪律**（[bootstrap-seed.md](../../../../.claude/rules/bootstrap-seed.md)）：本阶段只加
> **support**（z42c 能解析 `where T : enum`），z42c 自身源码与 stdlib **不使用**它 → 上一版
> nightly 仍能编当前源码，自举链不断。「use」要等下一个 nightly 发布后才可以。

## Phase 4 — 诊断质量（PR-4）⚪

- [ ] 4.1 ConstraintChecker 全程改用真 Span（`WhereClause.Span` / `WhereConstraint.Span` 本来就有）
- [ ] 4.2 删 `_noSpan()`
- [ ] 4.3 未知约束名（拼写错误）从静默改报 **E0443 UndefinedType**
- [ ] 4.4 负例用例：`where T : IFooo` → E0443
- **GREEN**：`xtask test` 全绿；确认既有诊断的 Span 变化不打翻 golden

## Phase 9 — 文档 + 归档 ⚪

- [ ] 9.1 `docs/book/src/language/` 新建约束页（或并入 generics 页）：七项约束语义 +
      **裸名匹配的已知限制**（D1）+ 跨包尚不校验（诚实标注）
- [ ] 9.2 `docs/design/language/generics.md` 的「约束体系」表按实况更新
      （当前标 ✅ 但实际未校验的行必须改）
- [ ] 9.3 `GenericConstraint.z42` 那条**已过时**的「z42c 缺对应类型/信息」注释删除/改写
- [ ] 9.4 Deferred 六条登记进 roadmap 未完成项索引（见 design §7）
- [ ] 9.5 `src/tests/README.md` 已在 0.4 更新——复核
- [ ] 9.6 归档 `changes/` → `archive/2026-XX-XX-complete-where-constraints/`，**随 PR 一起提交**

---

## 风险登记

| 风险 | 等级 | 处置 |
|---|---|---|
| 基元 → wrapper 归一有偏差 → `Dictionary<int,int>` 编不过 → **自举链断** | 🔴 **未解除** | 探针零误报**不构成证据**——跨包约束本轮 100% 不校验，该风险要到 PR-5 才真正暴露。原定的「给 struct P 补 IEquatable」处置一并顺延 |
| 接口继承闭包代价超预期 | 🟢 已解除 | 实际代价远低于预期（`Z42InterfaceType` 加 base 列表 + `Implements` 两层 BFS），未走退路 |
| 去 `ArgCount` 过滤让 21 条泛型接口约束一次性生效 | 🟢 已解除 | 两轮探针新增诊断 0 条 |
| 实例调用路径此前整条不校验（1b′ 新发现） | 🟢 已解除 | 修后两轮探针 `E0445` arity 亦 0 条；修法克制在「接真 ms」，不动重载决议 |
| Span 变更打翻既有诊断 golden | 🟡 | Phase 4 独立 PR，golden 变化单独对账 |
| 跨包 100% 不校验（本轮不修）→ 用户以为已保护 | 🟡 | 文档**诚实标注**（9.1）；PR-5 下一轮 |
