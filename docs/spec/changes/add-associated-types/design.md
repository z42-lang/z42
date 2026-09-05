# Design: 泛型表达力 —— 跨包约束持久化 + `Self` + 关联类型

> 对齐：2026-09-06 ｜ 前置：`complete-where-constraints`（已归档）

## Architecture

约束信息在链路上**三层递减**（实测，非推测）：语义侧 8 类 → IR 侧 2 个承载位 → writer 实写 1 个 flag 位。

```
                    ┌─ 本包路径（今天唯一通的路）────────────────────────────┐
  ClassDecl.Wheres ─┤                                                        │
        │           └→ ConstraintChecker.Resolve → SymbolTable.ClassConstraints → Check ✅
        │                                          （键=裸名，唯一写入点）
        │
        └→ ClassDescBuilder ──→ IrConstraintDesc ──→ ZbcWriter ──→ zbc TYPE 段
           ⚠️ special 丢弃      ⚠️ 只有 2 承载位     ⚠️ 只写 bit3    （bit0-6 已规约）
           ⚠️ base/iface 混同                                            │
                                                                          │
  ┌───────────────────────────────────────────────────────────────────────┘
  │  跨包路径（今天断在这里）
  ▼
 ZbcReader._readConstraintBundle ──→ IrClassDesc.TypeParamConstraints
   ⚠️ bit2 读而不存                            │
                                               ✂️ TsigReconcile 完全不读 ← 断点
                                               │
                            ExportedClassZ ────┘  ⚠️ 无约束字段
                                  │
                            ImportedSymbolLoader  ⚠️ 无约束容器
                                  │
                            SymbolTable.ClassConstraints ← 永远收不到导入类型
                                  │
                            Check 第一行 HasConstraints → return  ⇒ 跨包 100% 不校验
```

PR-1 把 ✂️ 与所有 ⚠️ 逐一接通。PR-2 / PR-3 在这条打通的通道上加表达力。

---

## Decisions

### D1: `ExportedClassZ` 可以加字段 —— 过时禁忌的推翻（本设计的地基）

**问题**：[`ImportedSymbolLoader.z42:92-94`](../../../../src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42) 明文禁止
「新增 `ExportedClassZ` 字段」，理由是「会让 z42c 源用一个上一 nightly 种子 z42.ir 尚无的字段」
（bootstrap-seed 轴③，stdlib API 面）。若该禁忌成立，PR-1 的跨包搬运无路可走。

**核实结论：禁忌已过时**，三条独立证据（已亲自抽验，非仅采信调研）：

1. **机制早于注释 13 天**：去 warm-skip 的 `07596b57` 是 **2026-07-30**，禁忌注释 `ea3f0c73` 是
   **2026-08-12**。[`xtask_compiler.z42:113-120`](../../../../scripts/build/xtask_compiler.z42) 的注释白纸黑字：
   「**总是**用当前 driver 把当前源 z42.ir 建进 build-libs……**破了 z42c⇄z42.ir 的「新 API 需晚一
   nightly」轴④约束**」。注释写下时它已失效。
2. **实证先例 ×5，4 次在注释之后**：`IsSealed`(08-07) / `Visibility`(08-13) / `IsDeprecated`(08-23) /
   `ExportedMethodZ.TypeParamCount`(09-03) / `StrMap.Find`(09-05)，全部「z42.ir 加成员 + z42c 源
   **同 commit** 消费」，全部 CI 绿。其中 `a71278b5`（#398）与禁忌禁止的操作**结构上完全同型**
   （同文件、姊妹类 `ExportedMethodZ`、同 TSIG 构造后赋值路径），零格式 bump。
3. **注释声称守门的那道门不在守**：`verify-selfhost` 走的正是 `_ensureBootstrapSelfDepLibs`
   预建路径（`xtask_compiler.z42:86`），按构造看不见这个轴。

**决定**：`ExportedClassZ` 加约束字段，**遵守唯一残余的真约束**——新字段不进构造函数签名、
ctor 内给默认值、由 `TsigReconcile` 构造后赋值（种子 ABI 兼容，五次先例均遵守）。

**连带修正**（同 PR 内先落一个独立 commit，因它们直接阻碍/误导本次实施）：

| 位置 | 改法 |
|---|---|
| `ImportedSymbolLoader.z42:91-98` | 保留「struct-ness 编码在 `HasBase`」的**事实描述**（读代码的人需要知道）；删除「不许加字段 ← bootstrap 越界」的**过时论证** |
| `self-hosting.md:219` | 删「warm 树直接跳过（幂等）」——该行为 2026-07-30 已删，且与同文件 :235-245「与 z42.ir 同款不 warm-skip」自相矛盾 |
| `bootstrap-seed.md:151` | 旧函数名 `_ensureBootstrapZ42Ir` → `_ensureBootstrapSelfDepLibs`；轴④判据补「**已有包的新 API** 由预建自动破环，无需等 nightly」 |

> **不改 struct-ness 的 `HasBase` 复用编码**：那是独立的技术债（`ExportedTypeExtractor` 写、
> `ImportedSymbolLoader` 读），与本 change 无关，顺手改会踩「不修 Scope 外问题」。登记为 Deferred。

### D2: `xtask test bootstrap` 的 A 路径接上破环预建

**问题**：该门的 A 路径（nightly VM + nightly z42c + **nightly stdlib** 编当前 z42c 源）
不经过 `_ensureBootstrapSelfDepLibs`，与真实构建路径口径不一致 ⇒ PR-1 加 `ExportedClassZ` 字段后
它会报红，但那是假阳性（次日 nightly 发布即自愈）。

**决定**：把 `_ensureBootstrapSelfDepLibs` 的目标 libs 目录参数化，A 路径用**隔离的 runlibs**
调用同一函数，与 `verify-selfhost` 同口径。

**改完之后它还守什么（必须写进注释，否则下一个人会以为它废了）**：

| 轴 | 改后是否仍守 | 为什么 |
|---|---|---|
| ① 语法越界 | ✅ 仍守 | 预建本身要用 nightly z42c 编当前源自依赖库；用了新语法就在预建阶段炸 |
| ② zbc/zpkg 格式 | ✅ 仍守 | 同上 |
| ③ **其余 stdlib 库**的 API 面（z42.collections / z42.threading / …） | ✅ 仍守 | 预建只覆盖 6 个自依赖库，其余仍用 nightly 版 |
| ③ **自依赖库**（z42.ir / z42.core / z42.project / z42.build / z42c.core / z42c.syntax）的 API 面 | ❌ 不再守 | **这正是要消的假阳性**——真实路径本就预建，守它等于守一个不存在的约束 |

**顺带修掉的既有缺陷**：`_compilerMembers` 因 #317 只剩 `driver/pipeline/semantics`，A 路径
**不重建当前源前端** → 对新增的 `z42c.syntax` 符号同样报假红。预建覆盖 `z42c.core` + `z42c.syntax`，
一并消除。

**明确不做**：不重设这道门「整体该守什么」、不把它接进 CI。那是独立议题（登记 Deferred）。

### D3: 约束键规则统一 —— 引入单一辅助函数

**问题**：三套键规则今天各说各话：

| 容器 | 键 | 位置 |
|---|---|---|
| `SymbolTable.Classes`（本包 + 导入） | **条件 arity-mangle**：同名多 arity → `Name$N`，否则裸名 | `StubCollector.z42:161`、`ImportedSymbolLoader.z42:80-81` |
| `SymbolTable.ClassConstraints` | **恒裸名** | `ConstraintChecker.z42:40`（唯一写入点） |
| 查询侧 | `inst.Def.Name()` = **恒裸名** | `ConstraintChecker.z42:122` |

写入与查询今天恰好一致（都裸名），所以本包能跑；但**同名多 arity 泛型类在 `ClassConstraints`
里互相覆盖（last-wins）**，是既存的键歧义 bug。导入侧一旦接入，`Name$N` 与裸名对不上则直接失效。

**选项**：
- A — 全部改用裸名：简单，但保留 last-wins 歧义，且 `Foo<T>` / `Foo<T,U>` 的约束会串味。
- B — 全部改用 `IrName()`（恒 `Name$N`）：无歧义，但与 `Classes` 的条件规则不一致，两套心智。
- C — **引入 `SymbolTable.ConstraintKey(Z42ClassType)`，写入 / 查询 / 导入三处统一调它**，
  内部实现 = `Classes` 的同款条件 mangle 规则。

**决定：C**。理由：① 顺带消掉既存的 last-wins 歧义；② 与 `Classes` 同规则 ⇒ 只有一套心智；
③ 单一函数是唯一真相源，将来改规则只改一处（正是 `common-pitfalls` §1 那类「同一决策散落多份」
的预防）。本包写入侧 `Resolve` 需先经符号表拿到 `Z42ClassType` 才能应用该规则——这是 C 相对
A 的唯一额外成本，可接受。

### D4: PR-1 的落地强度 —— 先 warning 探针，零误报才翻 error

**沿用 `complete-where-constraints` 已由 User 裁决的做法**（勿重问）：新接通的跨包校验先以
**warning** 落地，跑完整 GREEN + 自举不动点，确认零误报后再在**同一 PR 内**翻成 error。

**这一轮为什么格外重要**：上一轮的两轮探针都是零误报，但**覆盖面只有同包**——`complete-where-constraints`
的 memory 明确记载「本轮探针零误报不构成证据」。真正的 🔴 风险（D7）只在跨包接通后才暴露。

### D5: `Self` 的语义模型 —— 复用既有型参机制，不新造类型

**关键发现：`Self` 的实现半径比 proposal 估计的还小**，三条原以为要做的事实际不需要做：

1. **约束位零改动**：`where T : IEquatable` 与 `where T : IEquatable<T>` 在约束模型里**已经
   归约成同一个裸名 `"IEquatable"`**（`ConstraintChecker._fillBundle:88-95`，design D1 裸名匹配）
   ⇒ 两种写法天然等价，不需要任何「等价展开」代码。
2. **成员签名匹配零改动**：`class Int32 : INumber` 时无需把接口的 `Self` 替换成 `Int32` 去比对——
   因为**今天根本没有「类实现接口时的成员签名齐备性校验」**（`InheritanceResolver.z42:14` 自陈
   「严格校验留待后续」，`DiagnosticCodes.z42` 里也没有任何 "does not implement" 诊断码）。
3. **派发键零改动**：`static abstract` 成员走**裸名键**（`MemberCollector.z42:177` 的 `staticVirtual`
   分支，注释明确解释接口 `op_Add$2$T$T` 与实现 `op_Add$2$i32$i32` 必须同键才能 VCall 派发）
   ⇒ `Self` 不进键，不影响派发。

**决定**：`Self` 在接口内解析成 `Z42GenericParamType("Self")`，与今天的 `T` 走**完全同一条路**。

- **本包**：`SymbolTable.ResolveTypeP` 在「当前所属类型是接口」时，把 `NamedType("Self")` 解析成
  `Z42GenericParamType("Self")`。上下文经 `TypeEnv`（已有 `ClassName` 字段，且 `TypeEnv.ResolveType:86-97`
  已有「用 `ClassName` 二次改写类型名」的先例）与 `MemberCollector._fillInterface`（已持 `c.Name`）透传。
- **跨包**：`Self` 作为**裸字符串**写进接口方法签名（与型参 `T` 的编码方式完全相同——研究确认
  `T` today 就是以字符串 `"T"` 存在 `ExportedMethodZ.ReturnType` / `ParamZ.TypeName` 里）。导入侧在
  `ImportedSymbolLoader._resolve` 特判 `"Self"` → `Z42GenericParamType("Self")`。
- **作用域**：**仅接口**（User 裁决）。在类里写 `Self` 落到 `ResolveTypeP` 兜底 → `Z42UnknownType` → **E0443**，
  与其它未定义类型同一诊断，无需新错误码。

**顺带发现的既有缺口（PR-2 必须一并修，否则 `Self` 跨包直接坏）**：导入接口方法走的是
`_resolve(r, name)` **单参重载**（`ImportedSymbolLoader.z42:195-196`），**无型参上下文** ⇒ 跨包接口
方法里的 `T` 今天就落到 `Z42ClassType.Builtin("T")` 兜底，而非 `Z42GenericParamType`。`Self` 会
踩同一个坑。修法：给接口方法解析传入型参上下文（接口的 `TypeParams` ∪ `{"Self"}`）。

### D6: 关联类型 —— 地基先行，绑定显式声明，需要格式 bump

**结构性前置（proposal 初稿低估的部分）**：`Z42InstantiatedType.Def` 的静态类型是 `Z42ClassType`
（`Z42Type.z42:324`）⇒ **只能承载泛型 class/struct 实例化**；`Z42InterfaceType` 连型参名字段都没有
（`Z42ClassType` 才有 `GenericParamNames`）。`where T : IEnumerable<Item = U>` 今天**无处落脚**。

**决定**：PR-3 的第一阶段先补地基——给 `Z42InterfaceType` 加型参名槽，并让 `Z42InstantiatedType`
能承载接口（`Def` 提升为可承载 `Z42ClassType` 或 `Z42InterfaceType`）。这块地基本身独立有价值
（今天 `IEquatable<string>` 与 `IEquatable<int>` 在类型模型里无法区分，正是 Deferred
`where-constraint-future-type-arg-matching` 的根）。

**绑定：显式声明，不推断**（`type Item = int;` 而非从方法签名反推）。理由：① 推断需要跨成员的
统一算法（要处理 F-bounded 递归），代价与收益不成比例；② 显式声明与 Rust 一致，可读；
③ 今天连成员签名齐备性校验都没有（D5），推断没有可依赖的基准。

**`type Item;` 的解析：上下文关键字，不进 lexer**。`type` 今天不是关键字，贸然加会破坏所有把
`type` 当标识符用的既有源码。改在 `MemberParser._parseMemberBody` 的「`_parseType()` 之前拦截区」
（`:107-133`，与 `class`/`enum`/`implicit` 同款前瞻）按 `Identifier("type") + Identifier + ;/=` 三 token
前瞻拦截。注释 `:106` 已明确此拦截区的存在理由。

**格式**：约束 bundle 的 bit0–bit6 已用满（bit6 = funcSig），**bit7 空闲** ⇒ 绑定用
`bit7 = has_assoc_bindings` + `count u8` + `(name_idx u32, type_idx u32) × n`。新 wire 内容 ⇒
**PR-3 需要 zbc minor + zpkg minor 双 bump**（`version-bumping.md` 规则：改 wire layout 必 bump；
zbc bump 必同步 zpkg bump），并同步三方 reader（Rust `type_reader.rs` / `ZbcReader` / `ZpkgReader`）
+ 10 个 committed fixture regen。

> 格式 bump 本身**不再是障碍**：`fix-bootstrap-format-bump-deadlock` 的两代自举已让 bump 的
> CI 路径自动通过（实测 0.25→0.30 连续 5+ 次真实 bump 全绿）。但**别与删 cold 兜底同周期**。

### D7: 🔴 跨包接通会暴露的自举链风险 —— 处置预案

`complete-where-constraints` 登记的这条风险，**PR-1 是它第一次真正暴露的时刻**：

> 基元 wrapper 归一偏差 → `Dictionary<int,int>` 编不过 → 自举链断。

机理：`Dictionary<TKey,TValue> where TKey : IEquatable<TKey>`，实例化 `Dictionary<int,int>` 时
跨包校验会去问「`int` 是否实现 `IEquatable`」。若基元 `int` 与其 wrapper（`Int32`）在
`SymbolTable.Implements` 里归一不一致，答案是「否」→ 整个 stdlib 编不过 → 自举断链。

**处置**（按顺序，任一步骤失败即停下报 User）：

1. D4 的 warning 探针**先跑**，不翻 error。
2. 观察探针输出：若出现 `Dictionary<int,int>` 一类误报 → **不绕过、不加特例**，回到根因：
   查 `_satisfiesInterface`（`ConstraintChecker.z42:219-236`）对基元实参的归一路径，与运行期
   `generics.rs` 的 `constraint_satisfied_by` 对账（运行期是判定 SoT，编译期照抄，勿另立规则）。
3. 原定「给 `src/tests/types/struct_generic_container.z42` 的 `struct P` / `struct Tagged` 补
   `: IEquatable<>`」的处置，在本 PR 内执行（上一轮已顺延到这一轮）。

### D8: 发现的额外缺口 —— 接口的 `where` 从不被 Resolve

**实测**：`ConstraintChecker.Resolve:38` 的条件是 `Kind == "class" || Kind == "struct"`
⇒ **接口声明上的 `where` 子句根本不进约束模型**。`INumber<T> where T : INumber<T>` 的约束
今天完全不校验。`ClassDescBuilder._interfaceDesc:341-344` 同样只建全空 bundle。

**决定**：**纳入 PR-1**（一行条件 + 一处 bundle 填充）。理由：① 它与「跨包约束不校验」是同一
性质的静默失效，同属本 change 要偿的债；② PR-3 的关联类型声明在接口上，必然需要接口约束模型
先立起来，届时补等于返工。

> 这是实施前发现的 Scope 扩张，已如实计入上面的 Scope 表，请在 gate 时一并确认。

---

## Implementation Notes

- **判定 SoT 不变**：`src/runtime/src/corelib/reflection/generics.rs::validate_type_arg_constraint`
  + `type_has_no_arg_ctor` 是唯一真相源，编译期照抄，**不另立规则**（上一轮已裁决）。
- **bit 位权威**：`src/runtime/src/metadata/zbc_reader/type_reader.rs:296-313`
  （`0x01` class / `0x02` struct / `0x04` base / `0x08` tpRef / `0x10` ctor / `0x20` enum / `0x40` funcSig）。
  三方 reader 布局必须逐字节一致。
- **手拼 zbc TYPE 段做 fixture 的坑**（上一轮踩过）：必须把 `ReadTypeAt` 读到尾——**对象全字段
  布局块**（`(Flags & 116) == 0` 时**恒存在**、非 gated）最容易漏，漏了全部用例 OOB。
- **`ExportedClassZ` 新字段的硬约束**：不进 ctor 签名、ctor 内给默认值、`TsigReconcile` 构造后赋值。
- **PR-1 零格式 bump**：约束 bundle 的 bit0-6 早已规约、三方 reader 已按完整布局消费，写端置位
  不改 layout。**实施时必须实测确认**（regen 后 `cargo test --test format_fixture_versions` 应绿）。

## Testing Strategy

- **单元（本包）**：扩充 `src/compiler/z42c.semantics/tests/typecheck/constraint_tests.z42`
  （现 27 条负例）——每个 PR 各补正例 + 负例，覆盖 spec 的每条 Scenario。
- **跨包端到端**：`src/tests/cross-zpkg/` 下三个新用例（PR-1 / PR-2 / PR-3 各一），这是本 change
  的**核心验证面**——`complete-where-constraints` 的教训正是「同包全绿不构成跨包证据」。
- **格式防腐**：PR-3 的 bump 需 regen `src/tests/zbc-format` 6 个 + `src/tests/zpkg-format` 4 个
  committed fixture，并确认 `cargo test --test format_fixture_versions` 绿。
- **自举**：每个 PR 合并前跑完整 `xtask test`（10 stages）+ `xtask test bootstrap`（改 parser /
  codegen / 格式后必跑）。**cold 路径本地不可验，以 CI 为准**。
- **JIT**：本地 GREEN 只跑 interp；PR-1 改了运行期校验的活死分支 ⇒ 需补跑
  `xtask test stdlib --mode jit`（memory `local-green-misses-jit-and-lines` 的教训）。

## Deferred（登记到 roadmap，本 change 不做）

| 项 | 说明 |
|---|---|
| `exported-class-struct-ness-encoding` | struct-ness 复用 `HasBase` 编码（`ExportedTypeExtractor` 写 / `ImportedSymbolLoader` 读），D1 已确认可改为显式字段，但属独立技术债 |
| `bootstrap-check-door-purpose` | `xtask test bootstrap` 整体该守什么 + 是否接进 CI（D2 只做「接上预建」这一处） |
| `interface-member-completeness-check` | 类实现接口时的成员签名齐备性校验（`InheritanceResolver.z42:14` 自陈留待后续；D5 依赖它「不存在」这一事实） |
| `where-constraint-future-inferred-method-args` / `-toplevel-func` / `-func-constraint` | 上一轮已登记，本轮不动 |
