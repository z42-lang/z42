# Design: 型参收者上的成员可用性

## Architecture

型参收者（`rt is Z42GenericParamType`）今天有**两条互不知情的路**，各有一个兜底：

```
        a.M(args)  ──► MemberResolver.z42:229   （方法路）
                        ① Object 成员        → BoundCall.Marked ✅
                        ② 约束接口方法        → BoundCall.Marked ✅（Part A 补过）
                        ③ 兜底 sig = null    → BoundCall(Unknown)   ← B 改这里

        a.M       ──► MemberResolver.z42:322   （属性 / 字段路 = GS5）
                        ①' 无                 ← A 在这里补 Object / 约束查找
                        ③' 兜底 直接 Unknown  → BoundMember(Unknown) ← B 改这里
```

**不对称就是 A 的根因**：`bind-self-param-and-constraint-members` Part A 只补了方法路；
属性路从 2A-2（GS5）起一直是「直接松绑 Unknown」，运行期按名字当字段读 ⇒ 读不到 ⇒ `null`。

## Decisions

### Decision 1: A 与 B 同一个 change、A 先落

**问题：** 能不能只做 B（补诊断）？

**决定：不能，且顺序是 A→B。** B 的判据是「不是 `Object` 成员、也不是约束提供的」。
而今天属性路**压根没查过约束** ⇒ 先做 B 会把 `where T : IHasName` 的 `a.Name`
判成「不可用」而报错——把一个静默错值换成一个**误报**，比现状更坏。
A 让「约束提供」这一格真的可判，B 才有正确的判据。

### Decision 2（🔄 **实测后改判**）：B 收窄到「成员名全仓不存在」，不做两个报错点

**初版设计**：按字面全面执行成文规则（不由 `Object`/约束提供 ⇒ 报错），并用 `BoundCall` 上的
`RetIsErasedTypeParam` 标记区分两种修法（写显式类型实参 / 加 `where`）。

🔴 **被存量 e2e 证伪**（`generic_constraints` + `generic_baseclass` 两条本来全绿的用例判红）：

- `T Max<T>(T a,T b) where T : IComparable` + `var m = Max(a,b); m.value` —— `Num` 是 class，
  **今天正常打 7**。引用类型经擦除返回位流出来时运行期派发良好 ⇒ 全面判红 = **误报惯用写法**。
- `where T : Animal` 的 `pet.legs` —— **基类**约束提供的成员。初版只查了接口约束
  （`_constraintIfaceMethod`），**漏了基类约束** ⇒ 误报。

**真正会坏的只有 blob struct 经擦除返回位**（`id(v).X`），而那一格的根因是泛型**特化**
（实测两份 IR 只差 callee 名），不是成员解析 —— 报错修不好它。

**决定（User 2026-09-25 裁决）：收窄到「成员名在任何已知类型上都不存在」。**
零误报、零存量破坏，抓住「拼错名字 / 调不存在的方法」这类必然错。

📌 **连带简化**：成员压根不存在时，「写显式类型实参」与「加 `where`」**两种修法都不对**，
正确的话是「这个名字不存在」⇒ 两个报错点的区分失去意义，`RetIsErasedTypeParam` 标记成为
死机制，**已撤**（不留半用的机制）。下面这条决策记录保留，因为它的判据分析在将来
真要做「全面执行」那一刀时仍然成立。

### Decision 2b（保留备查）：若将来要全面执行，区分两个报错点得用「结果标记」而非 env 判据

**问题：** 两个报错点的修法不同（写显式类型实参 vs 加 `where`），得分辨收者是
「从 callee 返回位漏出来的型参」还是「当前作用域内的不透明型参」。而
`Z42GenericParamType` **只有名字**（`Z42Type.z42:580-588`），既无 owner 也无约束。

**选项：**

- **A. 给 `Z42GenericParamType` 加 owner** —— 归档 Deferred
  `tighten-bare-type-param-target-erasure` 预估的做法。缺：动类型模型，
  该 Deferred 自己写着「爆炸半径另算」。
- **B. 用 env 判据**（`MethodParamIndexOf(name) >= 0`）。缺：🔴 **错的**。
  `void g<T>(Vec2 v) { id(v).X }` 里 caller 的 `T` 与 callee 的 `T` **同名不同物**，
  env 会说「在作用域内」⇒ 给出错的修法。
- **C. 在调用结果上打标记**：`BoundCall` 增 `RetIsErasedTypeParam`，在
  `BoundCall.Marked(bc, sig)` 里按 `sig.Ret is Z42GenericParamType` 置位；成员访问时看
  **收者表达式**是不是带该标记的 `BoundCall`。

**决定：选 C。** 判据落在「这个值是从哪来的」而不是「这个名字长什么样」，天然免疫同名问题；
且 `Marked` 的接线**已覆盖 20 余个调用点**（含自由函数 `MemberResolver.z42:585`），
与 `RetIsNullable` 逐字同款——抬头已写明该范式的取舍（「显式接线漏掉一个点只会漏报，
方向安全得多」）。**类型模型零改动 ⇒ Deferred 预估的那份成本不发生。**

⚠️ 沿用该范式也继承其性质：**漏接线 = 漏报**（退化成今天的行为），不会误报。方向安全。

### Decision 3: 两个诊断码都复用，不取新号

| 报错点 | 码 | 为什么复用 |
|---|---|---|
| 调用点 | **E0455** `GenericTypeArgRequired` | 现有措辞就是「write the type argument explicitly」——**修法逐字相同**，只是触发面从「callee 体内消费型参」扩到「调用方消费返回位型参」 |
| 体内 | **E0401** `UndefinedSymbol` | 镜像 #801 的基元收者路径（「no member `X` on …」），**同一类问题同一个码**，措辞对称 |

⭐ 先问「能不能复用现成的码」——①⑥ 原本都记着「需新诊断码」，实测两条都不用；③ 一个都没取。
本变更同样不取号 ⇒ 整套取号流程（`DiagnosticCodes.z42` 登记 / 字面量纪律 /
`diag-literal-emitters.txt` 双向棘轮 / `error-codes.md` 词条）一步都不用走。

### Decision 4: A 的实现镜像方法路，不另写一份查找

**决定：** GS5 里调**同一个** `_constraintIfaceMethod(env, rt.Name(), "get_" + m.Name)`，
返回类型过**同一个** `_substSelfSig`，产**同一种** `BoundCall`（`get_<Name>` 实例调用）。

理由：约束查找要覆盖「父接口闭包 + 方法级与类级两个来源 + `Object` 优先」三件事，
手抄第二份必漂（同 `IfaceBridgeSynth` 抬头那条「名字复用 `_blobStructName`、不另拼一份」的教训）。
⚠️ **`Object` 优先于约束接口的顺序不能反**（`MemberResolver.z42:246-248` 明写：反过来会改派发键
→ 撼动自举字节）。A 在属性路建立同一顺序。

### Decision 5: 发射与 VM 零改动

A 产出的是既有的 `BoundCall(get_X)` 形态 —— 与非泛型属性读**同一条发射路径**；
B 只报错、不发射。⇒ **无 IR 新指令、无 zbc/zpkg 格式 bump、无 VM 改动、无 fingerprint bump。**

## Implementation Notes

- **A 的落点**：`MemberResolver.z42:322` 的 GS5 分支，在 `return BoundMember(…Unknown)` 之前插入
  ① `Object` 成员（属性形态：`get_<Name>`）② 约束接口 `get_<Name>`。
- **B 的落点**：方法路的 `③ sig = null` 兜底（`:263-264`）与 GS5 的兜底（改造后的尾部）。
- **标记传播**：只需 `BoundCall`；`a.Name` 的收者是 `BoundIdent` / `BoundMember` 等，
  不带标记 ⇒ 落 E0401 分支，正确。
- 🔴 **`Z42UnknownType` 不可顺手收紧**：GS5 的 Unknown 兜底还服务别的形态
  （注释写着「调用方再按 DepIndex 真实解析」）。**只在「收者确为 `Z42GenericParamType`」
  这一格报错**，别把 Unknown 兜底整条拆了。

## Testing Strategy

- **诊断单测**（`z42c.semantics/tests/typecheck/`）：E0455 两形态（`.X` / `.Bogus` / `.NoSuch()`）+
  E0401 体内形态 + **同名不同物那条钉子**（必须报 E0455 而非 E0401）+ 「一个错误一条诊断」。
  🔴 `SemanticDump` 只跑 `SymbolCollector`、**不加载 stdlib** ⇒ 用例里要自己声明用到的类型
  （#801 踩过：否则分支根本进不去 = 空测试）。
- **e2e**（`src/tests/generics/`）：A 的四条正面场景 + C 的放行面逐条（`Object` 四成员 /
  约束方法 / 赋局部 / 类级约束来源）。
- **阴性对照（必做，取最强的那种）**：**撤回修复本身**（而不是改期望值）——
  A 撤回后属性用例必须判红；B 撤回后诊断用例必须判红。逐条确认红在哪一行。
- **全仓摸底（先做，再动手）**：`xtask test` 全量跑一遍，把 stdlib / 自举里**依赖松绑**的
  写法找出来。**「零命中」必须解释**，否则交付的可能是恒不响的门。命中的逐处改成约束或
  显式类型实参 —— **不得为了让门禁过而放宽判据**。
- **完整 GREEN**：改了编译器 ⇒ 先 `xtask build sdk`（产物在 `artifacts/.z42/`）；
  外加**不带过滤的 `cargo test --lib`**（[[z42-xtask-test-misses-cargo-lib]]：`xtask test`
  的 15 个 stage 不含它）。本变更不加 builtin，但棘轮面广，照跑。
- **自举字节不动点**：A 改了属性路的派发（Unknown → 真 `get_X` 调用）⇒ **z42c 自身若有
  这种写法，字节会漂**。`xtask test compiler` 必须实跑确认，不靠推理。

## Deferred

- 🆕 **`enforce-bare-type-param-member-rule`**：按字面全面执行成文规则（不由 `Object`/约束
  提供即报错）。需先解决两件事：① 也查**基类**约束（本刀初版漏了，被 e2e 抓下）；
  ② 裁决 `var m = genericCall(); m.X` 这类**今天正常工作的惯用写法**是否接受判红
  （它对引用类型完全正常，只有 blob struct 真坏）。Decision 2b 的标记法可直接复用。
- 🆕 **`export-method-level-wheres`（User 2026-09-25 裁决另开一刀）**：**TSIG 不导出方法级
  `where`** ⇒ 所有方法级约束**跨包一律不校验**。实测：跨包 `Array.Sort<Opaque>(a)` 无诊断
  （这是**既存**行为，不是本变更引入）；类级约束跨包正常（`add-associated-types PR-1` 给它
  加过通道，可照抄该范式）。修它要动 zbc/TSIG 写读两端 + `ImportedSymbolLoader`，属格式变更。
  ⇒ 本变更的 C **只在同包生效**；`List<Opaque>().Sort()` 在用户代码里仍静默。
  ⚠️ 探索期我一度把「方法自己的型参 → ✅ E0402」当成无条件成立 —— 那是只造同包探针的结论。
  **凡是「是否被检查」的结论，探针必须同包 + 跨包各造一个。**
- **让 `id(v).X` 真的工作**（而非报错）：需泛型特化（实测 IR 证明显式形态靠 `@id<Vec2>`
  特化函数），属 [[z42-generic-instantiation-layout]] 线。
- `tighten-bare-type-param-target-erasure` 的**目标位**那半仍开着（本变更只关「收者位」）。
