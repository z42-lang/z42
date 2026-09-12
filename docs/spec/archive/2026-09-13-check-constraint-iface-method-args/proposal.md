# Proposal: 约束接口方法的裸型参形参位真校验（收紧 `Self→T` 擦除放行）

> change: `check-constraint-iface-method-args` ｜ scope: `compiler` ｜ 无格式 bump
> 来源: [[add-associated-types-program]] 队首 Deferred `tighten-bare-type-param-target-erasure` 的**剩余情形**
> （`IdOf<string>(7)` 那半已由第五批 #536 阶段 B 补上；本 change 补**型参收者调约束接口方法**那半）

## Why（gap 已在今天的 main `ccee746b` 上重新核实，非照抄 Deferred）

> 这是本线第五次「Deferred 记的根因是当时的推断」——Deferred 原话写「动**通用**擦除规则（C# CS1503）、
> 爆炸半径未量」，听起来要碰 `Conversion` 分支 B 的对称 XOR、风险大。**实测不是**：洞只在**一个**已知
> 语义的调用点，修法是「单点补一道复用现成范式的检查」，**不碰分支 B、不给型参加 owner 槽**。

### 洞的链路（三处源码注释已自供）

型参收者（`where T : IEq` 的 `T`）调用约束接口方法，走
[MemberResolver.z42:215-219](src/compiler/z42c.semantics/src/MemberResolver.z42#L215)：

```
MethodSymbol cms = this._tc._expr._constraintIfaceMethod(env, rt.Name(), mem.Name);
if (cms != null && cms.Signature != null) {
    Z42FuncType csig = MemberResolver._substSelfSig(cms.Signature, rt);   // Self → rt（裸型参 T）
    args = this._tc._overload.BindArgsToSignature(args, rawArgs, argCount, csig, env);
    return new BoundCall(..., csig.Ret, sp);
}
```

`_substSelfSig` 把接口方法里的 `Self` 精确替换成型参 `rt`（注释 [MemberResolver.z42:496-499](src/compiler/z42c.semantics/src/MemberResolver.z42#L496) 明说「不换形参位则分支 B 照旧放行、等于白接」）。**但换成 `rt` 后形参是*裸型参***——`BindArgsToSignature → CheckArgTypes → Conversion.Classify` 命中分支 B
[Conversion.z42:132](src/compiler/z42c.semantics/src/Conversion.z42#L132) 的「恰一侧含型参 → `GenericErase` 放行」，传进去的**具体类型实参照旧被擦除、零诊断**。即：`Self→T` 精确替换**没有**恢复形参位的实参检查（Deferred `tighten-bare-type-param-target-erasure` 已诚实记过这条）。

现场（源码注释 [MemberResolver.z42:205-207](src/compiler/z42c.semantics/src/MemberResolver.z42#L205) 就写着）：

```z42
bool badGen<T>(T a) where T : IEq { return a.Same("nope"); }   // 编译期零诊断
```

运行期派发到 `T.Same`、`other.V` 从 `string` 上读出不存在的字段 → 静默错值 / 崩。同族：
`where T : IComparable` 的 `a.CompareTo("x")`、`INumber` 的 `op_*`。

### 这正是本线的核心形状：「binder 收紧了，却没堵住」

邻居 **E0454**（#530 Part B，[MemberResolver.z42:122](src/compiler/z42c.semantics/src/MemberResolver.z42#L122)）已堵住**接口静态类型**收者调形参含 `Self` 的方法（逆变、无安全上界 ⇒ 一刀切禁止）。本 change 补的是**姊妹路**：**型参静态类型**收者——这条路 `Self ≡ T` 精确（约束就是「运行期 T 即实现类型」的断言），**可以安全真查**（不像接口静态类型那条只能禁止）。两条路的语义边界对齐：接口收者禁止、型参收者真查。

### 受益点与爆炸半径（DRAFT 估算，IMPL 阶段 0 必实测坐实）

- **Explore 普查（origin/main）**：stdlib 的约束接口方法调用（PriorityQueue / SortedSet / LinkedList
  的 `CompareTo`/`Equals`）**全是 `T`-to-`T` 形态**（实参也是裸型参）⇒ 两侧都含型参 ⇒ 分支 B 的 XOR
  **不命中**、落分支 D 的 `Identity` 放行 ⇒ **本 change 不碰这些、零误伤**。
- **真欠债（现存）≈ 0**：没找到任何「泛型体内对约束接口方法传具体类型实参」的现存写法 ⇒ 新诊断几乎
  全由负例测试触发（`badGen`/`a.CompareTo("x")`）。**潜在**欠债（防将来有人这么写）是主要价值。
- **假阳性 ≈ 0 的关键 = 只收「目标裸型参 + 源无型参」单方向**：分支 B 的另一方向（`T→object`/`T→接口`/
  `T→基类`，源含型参、目标不含）**必须保留**，碰了会全线误报。本 change **不碰分支 B**，只在那一个调用点
  补一道方向敏感的严格检查，从构造上避开这个风险面。

> ⚠️ 上面是估算。**IMPL 阶段 0 先不接线、只加探针**：`build stdlib` + `build compiler` 全量，
> grep 新诊断计数，逐条分类真欠债 vs 假阳性。>0 假阳性则停下来重判 scope（呼应本线「动手前量清」纪律）。

## What Changes

在 [MemberResolver.z42:215-219](src/compiler/z42c.semantics/src/MemberResolver.z42#L215) 的约束接口方法路径，
`BindArgsToSignature` **之后**补一道方向敏感的严格检查（新 `OverloadBinder` 方法）：

- 遍历 `cms.Signature`（**原始**接口方法签名，Self 未替换）的形参位；
- **门控** = `MemberResolver._containsSelf(cms.Signature.ParamTypes[i])`（该位来自 `Self`）——
  用 `_containsSelf`（比 `_substSelf` 多下钻 `Z42FuncType`）守，遵循本线「禁止侧比替换侧宽」原则；
- 对命中位，取对应实参类型 `at = args[i].Type()`，当 `at` **完全不含型参**
  （`!Conversion._hasGenericParam(at)`）**且**不是 `Z42ErrorType`/`Z42UnknownType`（不在已错实参上级联）
  → 报 **E0463**。否则（实参也含型参、或 error/unknown）保持今天行为。
- params 尾位同款门控（防御性，stdlib 零命中 `params Self[]`）。

**为什么这一个判据就够**：分支 B 只在「恰一侧含型参」时擦除。型参收者路径里，会被擦的**唯一**情形就是
「目标含型参（`Self→T` 后）、实参是具体类型」。实参也含型参的情形（`T`-to-`T`）走分支 D 的 `Identity`
今天就对；实参是**不同**型参（`U`-to-`T`）今天已落分支 D → 名字不等 → E0402（已报）。所以只剩「具体实参」
这一种是被静默擦除的，精确对应 `!_hasGenericParam(at)`。

**不碰的东西**（零漂移由构造保证）：
- 分支 B（`Conversion._classifyBuiltin`）**一字不改** ⇒ 所有推断路径 / `T→object` 逐位不变。
- `Z42GenericParamType` **不加 owner/scope 槽** ⇒ 因为这个调用点的 `rt` 已知是 caller 固定的不透明型参
  （它就是从 `rt is Z42GenericParamType` 分支进来的），语义已知、无需类型自身携带。
- 只**产诊断、不改绑定/发射**（不回灌任何代换结果）⇒ 自举字节不动点 3/3。无格式 bump。

## 需 User 裁决的语义点

1. **新 E-code E0463 vs 复用 E0402**（推荐 **E0463**）：本质是「具体类型无法赋给不透明型参 T」的实参
   不匹配，复用 E0402 也说得通。但 **(a)** 本线 #530/#536 每个「此前被擦除的泛型位」的修复都配**专码**
   （E0454/E0455）+ 指路消息，是既定房规；**(b)** 专码让负例门判据精确（退回对照对精确条数）；
   **(c)** 消息能给出正解写法。E0463 空号已确认。消息拟：
   > `cannot pass `string` to parameter of type parameter `T` — method `Same` comes from constraint
   > `where T : IEq`; an opaque type parameter accepts only values of that same type parameter`
2. **`null` 实参**（`a.Same(null)`）：`null` 字面量的 `Type()` 今天是什么决定它会不会被新判据误伤。
   **裁决方向**：保持今天行为（不新报），IMPL 用 fixture 钉死；若今天 null 绑成 Unknown/Error 则天然
   被 absorb 排除，无需特判。
3. **scope 只收「型参收者」这一路**（不动 E0454 的接口收者路、不动分支 B 通用规则）——即本 change
   **不等于**把 Deferred `tighten-bare-type-param-target-erasure` 整条关掉。剩余的「通用擦除收紧」
   （需区分作用域内不透明型参 vs 待推断型参、要给 `Z42GenericParamType` 加 owner）仍留 Deferred。

## 验证要点

- **编译期门**（`z42c.semantics` 单测，`SemanticDump` harness —— 注意 `--emit-zbc` / golden **吞诊断**，
  编译期断言只能走单测，见本线 #530 教训）：`badGen`/`a.CompareTo("x")` 从「0 诊断」变「1×E0463」；
  `T`-to-`T` 正确写法仍 **0**（无误伤对照）。**退回对照**坐实新门修前 FAIL（精确条数）。
  ⚠️ 负例 lambda/表达式体不能自带同码错误（本线反复踩的「断言变谎言」）——用恒等/常量形态。
- **运行期真门**（e2e fixture）：一个 `where T : IComparable` 的泛型函数真跑，证明正确路径不被误伤、
  能编能跑（interp + jit 双绿）。
- **阶段 0 爆炸半径实测**：先探针、量 `build stdlib`+`build compiler` 新诊断数并逐条分类（见 Why）。
- **零漂移**：`xtask test` 全 stage + 自举不动点 3/3 gen1==gen2；`test stdlib --mode jit` + `test e2e
  --dir cross-zpkg --mode jit`（本地 GREEN 只跑 interp，派发面改动须补）；`test bootstrap`（无格式 bump）。

🤖 Generated with [Claude Code](https://claude.com/claude-code)
