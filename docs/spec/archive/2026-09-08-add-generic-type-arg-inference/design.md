# Design: 方法级类型实参推断 + 形参位类型实参代换

> 配套 [proposal.md](proposal.md) ｜ 创建：2026-09-08

## Architecture

### 今天的数据流（缺口所在）

```
CallExpr
  ↓ _resolveOverload（裸 IsAssignableTo，泛型候选被静默淘汰；na==1 时不做任何类型匹配）
MethodSymbol ms
  ↓ _withDefaults(ms, …)         ← 用 ms.Signature 原样：BindWithTarget / BoxArgs /
  │                                 ConvertIfNeeded / params 打包 / 默认值
  ↓ CheckArgTypes(…, ms.Signature, …)
  │     形参是裸 T ⇒ Conversion 分支 B「恰一侧含型参 → GenericErase」⇒ 一律放行 ★缺口
  ↓ BoundCall
  ↓ _applyMethodTypeArgs(bc, call, ms, env)
        call.TypeArgCount == 0 → 第一行早退 ★推断缺口
        否则：写 bc.MethodTypeArgs + 校 arity(E0445) + where(ConstraintChecker.CheckMethod)
```

### 本 change 后的数据流

```
MethodSymbol ms（决议不变，推断一律在其后）
  ↓ _withDefaults(ms, …)          ← 输入**保持 ms.Signature 原样**（不变式 I1）
  ↓ CheckArgTypes(…, ms.Signature, …)   ← 保持不变（今天的放行行为原样保留）
  ↓ BoundCall bc
  ↓ ── 新增：诊断专用旁路 ────────────────────────────────
  │   绑定来源三选一：
  │     A. 受者是 Z42InstantiatedType → 取 recv.TypeArgs（类级型参）
  │     B. call.TypeArgCount > 0      → 取已解析的 targs（方法级型参）
  │     C. 否则                        → TypeArgInference.Infer(ms.Signature, args)
  │   ↓ 有绑定？
  │     是 → substSig = subst(ms.Signature, bindings)
  │          CheckArgTypes(args, rawArgs, argCount, substSig, env)   ← **只产生诊断**
  │          （C 分支额外：ConstraintChecker.CheckMethod(…, inferred targs, …)）
  │     否 → 什么都不做 = 今天行为（不变式 I2）
  └────────────────────────────────────────────────────
  ↓ _applyMethodTypeArgs（bc.MethodTypeArgs 的写入条件**不变**：仅显式）
```

### 两条不变式（本设计的全部安全性来源）

- **I1 —— 代换结果绝不回灌执行路径**。`_withDefaults` / `BoxArgs` / `ConvertIfNeeded` /
  `_withParamsExpansion` / `BindArgsToSignature` / 重载决议的输入一律是**原始** `ms.Signature`。
- **I2 —— 无绑定即无行为**。推断失败、非泛型、含 lambda 实参位 ⇒ 旁路整体跳过，逐字节等于今天。

⇒ **发射端零改动 ⇒ 零字节漂移**，由构造保证，而非靠测试兜。

## Decisions

### D1：为什么不按 roadmap 字面收紧 `Conversion` 分支 B

**问题**：Deferred 条目要求收紧「目标含型参、来源具体 → 擦除放行」。

- **选项 A（照做）**：分支 B 判 `None`。
  - 优点：一行改动。
  - 缺点：**实测崩**——`build stdlib` 头 3 个包就 33 条 E0402，逐条核对**全是合法代码**，真欠债 0。
- **选项 B（先补代换）**：让两侧都变具体类型，分支 B 自然不触发。
  - 优点：命中真根因；欠债实测 0；不动通用规则。
  - 缺点：工作量大得多（含推断）。

**决定：B。** 分支 B 的松弛**不是** bug，它是「形参类型没被代换」这个缺口的**兜底**——先把缺口补上，
剩下的裸型参目标（推断失败 / 型参收者）再单独评估是否收紧。roadmap 条目按此改写。

### D2：代换结果为什么必须走「诊断专用旁路」而不是回灌 `_withDefaults`

**问题**：最自然的写法是把代换后的签名直接传给 `_withDefaults`，一次到位。

**实测：那会打开四条独立的字节漂移通道**（调研证据，逐条 file:line）：

| 通道 | 机制 | 位置 |
|---|---|---|
| **params normal/expanded 翻转** | `args[pf].Type().IsAssignableTo(T[])` 今天恒 false（`"i32" != "T"`）⇒ 恒走 expanded 合成 `BoundArrayLit`；代换成 `int[]` 后变 true ⇒ 直接透传，**实参形状完全不同** | `OverloadBinder.z42:178` |
| **值 struct 去装箱** | `erasesS = … \|\| (target is Z42GenericParamType)` ⇒ 裸 `T` **恒装箱**；代换成具体 struct 后装箱消失 | `TypeChecker.z42:136-137` |
| **ConvertInstr / `op_Implicit` 凭空插入** | 裸 `T` 今天在 `Conversion.z42:127` 就返回，走不到 `_classifyUser`；代换后可能返回 `UserImplicit` ⇒ 多一条 Call | `TypeChecker.z42:180-189` |
| **lambda 重绑** | 裸 `T` 不是 `Z42FuncType` ⇒ lambda 落 `Z42UnknownType`；代换成 `Func<int,int>` 后走 `_bindLambda(expected)`，**body 每条指令都可能变** | `ExprTyper.z42:582` |

前两条会在 z42c 自举源里被**大量**触发（stdlib 容器 + `params object[]` 遍地）。

**决定：旁路。** 代换后的签名只作为 `CheckArgTypes` 的入参存在，不进入任何 emit 决策。
代价是同一批实参被检查两遍（第一遍用原签名，全部被擦除放行；第二遍用代换签名产生真诊断）——
纯诊断路径，无副作用，可接受。

### D3：推断插在重载决议之前还是之后

- **选项 A（之前）**：用推断后的具体形参类型参与 `_applicable`。
  - 缺点：**会把今天能编的代码变歧义**。`OverloadResolver._assignable:222-228` 用裸
    `IsAssignableTo`，裸 `T` 对任何具体实参都判「不可赋」⇒ 泛型候选今天被**静默淘汰**。
    一旦代换，`void F(int)` 与 `void F<T>(T)` 在 `_betterAtPos` 里都成精确匹配 ⇒ 歧义 ⇒ 编不过。
- **选项 B（之后）**：决议选定唯一 `ms` 后再推断。
  - 缺点：`OverloadBinder.z42:330` 的 `na == 1` 在类型匹配之前直接返回 ⇒ 泛型方法只要 arity 唯一
    就无条件选中 ⇒ **可能出现「决议先选、代换后报假错」**。但这正是 `IdOf<string>(7)` 该报的错，
    不是假错；真正的歧义场景（多候选）走 `OverloadResolver`，行为不变。

**决定：B（之后）。** 并在 tasks 里留一条验证：多重载 + 泛型混合的用例编译结果与基线逐字节一致。

### D4：推断出的类型实参是否回灌 `bc.MethodTypeArgs`

**问题**：回灌决定 opcode（`Op.Call` ↔ `Op.CallGeneric` 0xB4）、zbc 字符串池、以及是否走 native 快路径。

**两条设计原则在这里对撞**：
1. **推断是纯语法糖** ⇒ `Foo(x)` 必须严格等价于 `Foo<int>(x)` ⇒ 无条件回灌（显式路径今天就是无条件发）。
2. **不为用不到的东西付费** ⇒ 运行期只有 callee 体内出现 `typeof(T)`/`new T()`/`default(T)`/`new T[n]`
   时才需要类型实参（`exec_address.rs:91-106`、`exec_array.rs:80-90`）⇒ 条件回灌。

**数据裁决**：全仓普查（stdlib 25 库 + compiler 自建，`REAL_EXIT=0`）——隐式泛型调用**共 112 处，
全部是 `Array.Copy<T>` 一个方法**，而它的 `T` 纯粹是编译期类型安全装置：函数体只做参数校验，
搬运落到非泛型 native 原语 `CopyRange`（`Array.z42:52-58, 76-81`）。

⇒ **原则 1 在本仓语料上没有反例**（唯一的推断点位根本不消费型参）；而无条件回灌会把这 112 处
热点 bulk 拷贝全部推出 native 快路径（`exec_call.rs:135` 以 `method_type_args.is_empty()` 为门）
+ 重排整个 zbc 串池，换来**零**语义收益。

**决定：不回灌**（`bc.MethodTypeArgs` 的写入条件保持「仅显式」不变），
**并用阶段 D 把不回灌留下的洞变成编译错误**（见 D5）——这样等价性只在
「编译器明确拒绝的写法」上被打破，而不是静默产生不同语义。

**登记 Deferred**：「callee 需要才发类型实参」应当成为**统一规则**（显式路径也归它管），
届时既保住等价性，又能救回今天显式泛型调用白掉的 JIT 快路径。前置是可传递、跨包可见的
「方法体消费型参」分析（`$mta:` 转发要传递闭包，跨包要进元数据）⇒ 独立 change。

### D5：不回灌留下的洞怎么处置

- **选项 A（不管）**：文档标注「省略尖括号时 `new T()` 不工作」。
  - 缺点：**静默错值**（运行期读空 `method_type_args`），与 `generic-new-array-value-type-null-tail`
    同款形状；这正是本条线一直在清的那种假保障。
- **选项 B（报错要求显式）**：能判定 callee 消费型参时报编译错误。
  - 优点：静默错值 → 明确诊断，并直接给出修法（写出 `<T>`）。
  - 缺点：需要「方法体是否消费方法级型参」的 AST 分析；导入方法无 `Decl` 时判定不到。

**决定：B，判定不到则放行**（= 今天行为，严格无回归）。欠债预计 0（唯一隐式泛型调用不消费型参）。
⚠️ 这是 Open Question 1，可裁到独立 change。

### D6：推断算法的形态与失败语义

**算法**：对每个 (形参类型 `Pi`, 实参类型 `Ai`) 做结构化 unify：

```
unify(P, A, bindings):
  P is Z42GenericParamType 且 P.Name ∈ 待求型参:
      已有绑定 → CanonName() 逐字相同则一致，否则**冲突 → 整体推断失败**
      无绑定   → bindings[P.Name] = A
  P is Z42ArrayType   且 A is Z42ArrayType        → unify(P.Elem, A.Elem)
  P is Z42Instantiated 且 A is Z42Instantiated 且同 Def 同 arity → 逐位 unify
  P is Z42FuncType    且 A is Z42FuncType 且同 arity → 逐形参 + 返回位 unify
  其它 → 不产生信息（不算失败）
```

**保守收口**（爆炸半径全部来自这里，故三条都取最保守）：
- **任一型参未被绑定 ⇒ 整体推断失败**（不做 partial 代换）。
- **同一型参绑到两个不同类型 ⇒ 整体失败**（不做 C# 式「最佳公共类型」）。
  ⇒ `Max(1, 2L)` 今天怎样、之后还怎样。
- **实参类型是 `Unknown`/`Error`（含 lambda、target-typed new 延迟位）⇒ 跳过该位**；
  若因此有型参未绑定 → 按上一条整体失败。

**失败 = 完全按今天行为，不发任何诊断**（不变式 I2）。理由：warning 在本项目是空操作
（`driver-hides-warnings`），而报错会把大量今天能编的代码判红。

### D7：规范冲突处置（User 已裁决）

`docs/design/language/generics.md:380` 声称「自由函数调用时 T 从实参推断」——与 book SoT
`generic-methods.md:109` 及实现（`MemberResolver.z42:475` 早退）三方冲突，且 `:382 ### 限制（本阶段）`
**漏列**「不支持推断」。

**决定（User 裁决）**：整段迁进 `docs/book/src/language/generics.md`，顺带完成
`language/README.md:31` 标着「⬜ 待迁」的那次迁移；迁移时修正失效段落，`docs/design/` 侧删除。
按 `workflow.md:586-587`「旧 `docs/design/` 不再更新、知识一律落 book」。

## Implementation Notes

- `_substGenericSig` **镜像既有 `_substSelfSig`**（`MemberResolver.z42:449`）：
  `ParamsFrom` / `ParamDefaults` / `ParamCallers` 必须**原样搬运**——ctor 只设 `ParamsFrom = -1`，
  漏搬会让 params 变长形参退化成定长（`CheckArgTypes` 的 params 尾位分支读的就是它）。
- 代换半边**已经现成**：`_substGeneric`（`MemberResolver.z42:361-378`）处理型参→实参、数组、
  实例化递归。本 change 只需补 **unify 半边**。
- `TypeArgInference` 的递归面**必须与 `Conversion._hasGenericParam`（`Conversion.z42:213`）对齐**
  ——两者描述的是同一个「类型里哪里会出现型参」。不齐会出现「分支 B 认为含型参、推断却没覆盖到」的
  错配。⚠️ 注意 `_substGeneric` 今天**没有 `Z42FuncType` 分支**而 `_hasGenericParam` 有，
  实施时要么补齐要么在设计里显式说明为何可以不补。
- 阶段 D 的「消费型参」判定可复用现成入口：`TypeOpTyper.z42:61-64`（`typeof(T)` 经
  `env.MethodParamIndexOf`）、`CallEmitter.z42:355-364`（`new T()` 的 `IsMethodTypeParam`）、
  `ExprTyper.z42:546`（`default(T)`）。
- **阶段 A 天然不产生假红**：`_substGeneric` 对「名字不在 `inst.Def.GenericParamNames` 里」的型参
  （即**方法级**型参 `U`）返回 `Z42UnknownType`（`MemberResolver.z42:365`）⇒ 落 `Conversion` 分支 A
  的 Absorb ⇒ 放行。所以「类级 + 方法级混合泛型」的方法在阶段 A 下不会被误报，
  要等阶段 C 把方法级绑定也求出来才真检查。
- **`MethodSymbol.TypeParamCount` 对导入方法也已还原**（`Symbol.z42:26-28`，由
  `ImportedSymbolLoader` 从 `ExportedMethodZ.TypeParamCount` 填）⇒ 「这是不是泛型方法」跨包可判，
  不需要新元数据。

## Testing Strategy

### 单元（`z42c.semantics/tests/typecheck/generic_inference/`）

按 `argument_type_tests.z42:22-37` 的现行标准——**自建 `bodyDiags` + `countCode` helper**，
断言「码 + 条数」，不用 `FirstErrorCode`（它只看第一条，验不到「本该无诊断」与「多实参逐条报」）。

必须成对写正例（`Count() == 0`）与负例。

### 三道对照（每一道都必须真跑，缺一不可）

1. **阳性对照**：刻意违反能报 + 正确代码不报。
2. **真实构建面破坏性对照**：临时改坏一处**真实** stdlib/compiler 调用点，
   `xtask build stdlib` / `build compiler` 必须立刻红。**这道最容易漏，也最值钱**——
   证明检查活在真实构建路径上，而非只在单测里。
3. **退回对照（同源）**：在**同一份源码**上只回退消费端一处 → 单独 `build compiler`（~40s）→
   复现**逐字相同**的诊断。种子对照（nightly = main−N）**不够**，差一个 commit 就有混淆余地。

### 字节不动点（本 change 的核心风险闸门）

- 每阶段结束跑 `xtask test`，其中的 build wave 自带自举不动点校验。
- **额外**：阶段 A/B/C 各自结束时确认 `build compiler` 两轮收敛（gen1 == gen2）。
- ⚠️ **GREEN 跑测期间只改 `.md`**；中途若因退回对照重建过编译器，**最终态必须重跑一次完整 GREEN**。

### 改编译器/语法后必补（本地 GREEN 只跑 interp）

- `xtask test stdlib --mode jit`
- `xtask test e2e --dir cross-zpkg --mode jit`
- `xtask test bootstrap`（分支落后 main 时会红且报错极具误导性 —— **先 rebase 再看**）

## Deferred（登记到 roadmap，本 change 不做）

| key | 内容 |
|---|---|
| `tighten-bare-type-param-target-erasure` | **改写根因**：不是「擦除规则太松」，而是形参未代换。本 change 补代换后，真正剩下的裸型参目标（推断失败 / 型参收者 `a.Same("nope")`）是否收紧，需要区分「作用域内不透明型参」与「待推断型参」——`Z42GenericParamType` 今天只带名字不带 owner |
| `unify-method-type-arg-emission-gate` | 「callee 需要才发类型实参」统一到显式路径；能救回今天显式泛型调用白掉的 JIT 快路径。前置 = 可传递、跨包可见的「方法体消费型参」分析 |
| `generic-inference-best-common-type` | 同一型参绑到多个类型时取最佳公共类型（C# 口径）。v1 一律判失败 |
| `generic-inference-lambda-args` | lambda 实参参与推断（要动延迟绑定通道，牵扯字节漂移） |
| `generic-inference-in-overload-resolution` | 推断参与重载决议（今天会把可编代码变歧义，见 D3） |
| `ast-walker-completeness-gate` | 🔴 `MethodTypeParamUse` 的 AST 遍历完备性**没有自动门**——覆盖是 2026-09-08 从源码机械枚举建立的（Expr 36 / Stmt 16 / Pattern 10 / TypeExpr 6），将来给 AST 加新节点类**不会让任何测试变红**，漏了就是一个静默洞。需要一道「节点类全集 vs walker 已处理集」的门（源码级 grep 对账，或行为级逐节点用例） |
