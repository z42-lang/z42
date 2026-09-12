# Design: 约束接口方法裸型参形参位真校验

> proposal 见同目录。本文件记实现原理与决策权衡（供接手者不读源码即懂「为什么这样」）。

## D1 洞的机理（为什么 `Self→T` 替换没堵住）

分支 B（[Conversion.z42:132](src/compiler/z42c.semantics/src/Conversion.z42#L132)）是一个**方向无关的对称 XOR**：
`_hasGenericParam(from) != _hasGenericParam(to)` → `GenericErase` 放行。它服务两个方向：

| 方向 | 例 | 今天必须 |
|---|---|---|
| 源含型参、目标不含 | `T → object` / `T → IEq` / `T → 基类` | **放行**（泛型体内把 T 当上界用） |
| 目标含型参、源不含 | `string → T`（实参→不透明 T 形参） | **本应拒**，今天却擦除放行 ← 洞 |
| 两侧都含型参 | `T → T` / `U → T` | 不命中 XOR，落分支 D（`Identity` / 名字不等报 E0402） |

`_substSelfSig(cms.Signature, rt)` 把 `Self` 换成型参 `rt`，形参从「裸 Self」变「裸 rt」——**仍是裸型参**，
于是「具体实参 → 裸 rt 形参」正好命中第二行、被擦除。替换把问题从「Self 擦除」搬成「rt 擦除」，没消除。

## D2 为什么不碰分支 B、也不给型参加 owner 槽

**不碰分支 B**：它的第一个方向（`T→object` 等）是泛型代码的命脉，全局收紧会全线误报（#536 阶段 A 字面
收紧实测 stdlib 头 3 包即 33 条假阳性）。

**不加 owner 槽**：一般情形下，要区分「作用域内不透明型参（该拒具体实参）」vs「待推断型参（该放，等推断）」
确实需要型参带 owner——但**本调用点不需要**：它是从 `rt is Z42GenericParamType` 分支进来的
（[MemberResolver.z42:190](src/compiler/z42c.semantics/src/MemberResolver.z42#L190)），`rt` 是 caller
**已固定**的不透明型参（约束收者），语义在此处已知，不必由类型自身携带。⇒ 把收紧**下沉到这个已知语义
的调用点**，比改全局类型系统便宜且零风险。通用收紧（其它裸型参目标位）仍留 Deferred。

## D3 判据：为什么 `!_hasGenericParam(at)` 一个条件就够

型参收者路径里，分支 B 能擦除的**唯一**情形是「目标（`Self→rt`）含型参、实参不含」。穷举实参形态：

- 实参也含型参（`T`-to-`T`，或 `List<T>`）：XOR 不命中 → 分支 D。`T`-to-`T` → `Identity` 放行（对）；
  `U`-to-`T` / `List<T>`-to-`T` → 名字/结构不等 → E0402 **今天已报**。⇒ 不需新检查。
- 实参是 error/unknown：`_hasGenericParam` 对它们返回 **false**（它们不是 GP/Array/Inst/Func）——
  故必须**显式排除**，否则会在已错实参上级联误报。
- 实参是具体类型（`string`/`int`/用户类）：`!_hasGenericParam` 为 true、非 error/unknown → **正是洞**，报。

所以判据 = `at != null && !(at is Z42ErrorType) && !(at is Z42UnknownType) && !Conversion._hasGenericParam(at)`。

## D4 门控用 `_containsSelf`（不是 `_substSelf` 的覆盖面）

守的是一条**新诊断**（本质同「禁令」侧），漏一种 Self 形态 = 漏一个洞。`_containsSelf`
（[MemberResolver.z42:477](src/compiler/z42c.semantics/src/MemberResolver.z42#L477)）比 `_substSelf` 多
下钻一层 `Z42FuncType`（`void Apply(Func<Self,int>)`）。用它作门 ⇒ 覆盖面不小于替换面，符合本线
「禁止侧比替换侧宽」原则（[MemberResolver.z42:473-476](src/compiler/z42c.semantics/src/MemberResolver.z42#L473) 记的）。
注：`_substSelfSig` 不下钻 Func ⇒ `Func<Self,int>` 形参位替换后 Self 仍在内层；此时 `at` 是个具体
`Func<...>`（`!_hasGenericParam` true）→ 按 D3 报。这是防御性正确方向，stdlib 零命中。

## D5 注入位置与方法形态

- **位置**：`MemberResolver.z42:218` 的 `BindArgsToSignature` 之后、`return` 之前。此处 `args` 已绑定、
  `rawArgs` 在手（span 可指到实参本身，同 `_checkOneArg`）。
- **新方法**：`OverloadBinder.CheckConstraintIfaceArgs(args, rawArgs, argCount, cms.Signature /*orig,含Self*/,
  env)`——镜像 `CheckSubstitutedArgs`（[OverloadBinder.z42:83](src/compiler/z42c.semantics/src/OverloadBinder.z42#L83)）
  的逐位门控形状，但门控换成 `_containsSelf(orig.ParamTypes[i])`、命中后走 D3 判据发 E0463（不调
  `CheckImplicitConvert`，那会再走 Classify 被分支 B 擦——直接按 D3 判并发码）。
- **最终落点 = OverloadBinder**（`CheckConstraintIfaceArgs` + `_checkOpaqueParamArg`），`_containsSelf`
  提 `internal static` 供其调用。初版放在 MemberResolver 贴着调用点，但 `lines` stage 硬限（886 行）
  把 MemberResolver 顶到 892 行 → 超限。OverloadBinder 本就是「重载决议子绑定器」、拥有 `CheckArgTypes`/
  `CheckSubstitutedArgs`/`CheckArg`，是这道实参检查的**正确语义归属**，且只 541 行有充足余量 ⇒ 搬过去
  一举解决行数 + 归属两件事。调用点 `MemberResolver:218` 改 `this._tc._overload.CheckConstraintIfaceArgs(...)`。

## D6 E0463 发码纪律

按 E0449–E0462 既定手法：**语义层用字面量 `"E0463"` 发码**，DiagnosticCodes 里加常量 + 注释说明，
但语义层**不引用该常量**（避 core→semantics 新跨成员符号撞 F2 冷启动 stale-cache，待随 nightly
载入后可切常量）。常量名拟 `ConstraintMethodArgTypeParam`。

## D7 零漂移论证

只新增诊断调用，不触碰：`BindArgsToSignature` / `_withDefaults` / 任何发射。stdlib + 编译器 build 源
对约束接口方法**零具体实参调用**（阶段 0 实测坐实）⇒ 新检查在 build 路径上触发 0 次 ⇒ zbc 逐字节不变
⇒ 四阶段不动点 3/3 gen1==gen2。无格式 bump（纯语义诊断）。
