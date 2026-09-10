# Design: lambda 实参驱动方法级泛型类型实参推断（Scope 2 + 纯 A）

> 配套 [proposal.md](proposal.md) ｜ 创建：2026-09-11

## 今天的数据流（gap 所在）

```
_bindCall:
  非-lambda 实参 → 急切 _bindExpr；lambda / target-typed-new → null 占位
  ↓ (泛型自由/静态函数路径)
  _withDefaults(ms, args) → BindArgsToSignature(sig=ms.Signature 原样)
        对 null lambda 位: BindWithTarget(rawArg, sig.ParamTypes[i]=Func<T,…> 裸 T)
        ⇒ lambda 无标注形参拿到裸 T ⇒ 体内运算 E0402 ★gap（编不过）
  ↓ _applyMethodTypeArgs(bc, call, ms):
        TypeArgCount==0 → Infer(ms.Signature, bc.Args)   # #536 阶段C，诊断专用
              lambda 位 args[i].Type() = finalSig（形参仍是裸 T）⇒ 自绑 T→T 或与他位冲突 ★gap
```

## 本 change 后的数据流

```
_bindCall:
  非-lambda 实参 → 急切 _bindExpr；lambda → null 占位（不变）
  ↓ (泛型自由/静态函数路径，call.TypeArgCount==0 且 ms 泛型)
  ── 新增：绑定前推断 ────────────────────────────────
  part = TypeArgInference.InferPreBinding(sig, args, call.Args, names, env)
        源① 非-lambda 已绑位: unify(sig.ParamTypes[i], args[i].Type())
        源② lambda 延迟位 + Func 形参: unify(formal.ParamTypes[j], ResolveType(lam.Params[j].Type)) 逐标注位
        → 部分绑定（不要求全绑）
  bindSig = SubstituteFuncParams(ms.Signature, names, part)   # 只换 Func 形参位里已绑的型参名
  bindMs  = ms.WithSignature(bindSig)                          # 非-Func 位保持裸 T（零漂移）
  ──────────────────────────────────────────────────
  ↓ _withDefaults(bindMs, args) → BindArgsToSignature(bindSig)
        lambda 位 BindWithTarget(rawArg, Func<int,…> 具体) ⇒ 形参具体、体内定型、发具体 opcode ✓
  ↓ _applyMethodTypeArgs(bc, call, ms):   # 传**原** ms，诊断路径不变
        Infer(ms.Signature, bc.Args)   # 此时 lambda 已绑具体 ⇒ 其 Type() 亦具体 ⇒ 诊断推断照跑
        CheckMethod / CheckSubstMethodArgs / E0455 全不变
```

## 不变式（安全性来源）

- **I1'（非-lambda 位零漂移）**：`SubstituteFuncParams` **只**改 `Z42FuncType` 形参位；一切非-Func 形参位
  保持 `ms.Signature` 原样 ⇒ 非-lambda 实参的绑定 / 装箱 / params 展开 / 默认值 / 重载决议**逐字节不变**。
  `Array.Copy<T>`（无 Func 位）等 112 处隐式泛型调用完全不受影响。
- **I2（无绑定即无行为）**：`InferPreBinding` 返回空（两源都推不出）⇒ `bindMs == ms` ⇒ 逐字节等于今天。
- **I3（不回灌）**：`bc.MethodTypeArgs` 写入条件不变（仅显式）。emit 变化**只**发生在 lambda 体，
  且仅当推断真的代换了该 lambda 的形参位；build 源零命中该形态 ⇒ 自举/stdlib 不动点 3/3。

## Decisions

### D1：为什么「只代换 Func 形参位」而非整条签名代换（像 ForExplicitTypeArgs）

显式 `<>` 路径 `ForExplicitTypeArgs` 代换**整条**签名并绑全部实参——它能接受由此产生的字节变化，因为
`Sort<int>` 的签名本来就是具体的、用户显式要求了。但**隐式**路径若整条代换，会在非-lambda 位打开
#536 D4 记录的四条漂移通道（params normal/expanded 翻转、值 struct 去装箱、ConvertInstr 插入、
lambda 重绑），而这些在 `Array.Copy<T>` 等 112 处隐式调用上遍地触发 ⇒ 大面积字节漂移、零语义收益。
**决定：只代换 Func 形参位**——lambda 重绑正是我们**要**的那条通道，其余三条对非-lambda 位一律关闭。

### D2：为什么绑定前推断要单独走 `InferPreBinding`（不复用 #536 的 `Infer`）

- `Infer` 要求**全部型参绑定**才成功（诊断路径的保守边界①），且读**已绑** `bc.Args`。
- 绑定前推断需要**部分**绑定（`Map<T,U>` 只推出 T 也要能代换 Func 形参位使 lambda 能编）、且 lambda 位
  此时是 null（要从 **AST** 读标注，不是从 BoundLambda）。两者形态不同 ⇒ 单独一个方法，语义清晰。
- `Infer`（诊断路径）**保持不动**：绑定后它照跑，此时 lambda 已具体，能驱动 where 校验。

### D3：纯 A 的无标注形态为何无解（B1 边界）

`Compose<T>(Func<T,T> f, Func<T,T> g)` 调 `Compose((x) => x + 1, …)`：`x` 无标注 ⇒ 型参 T 无任何
非-lambda 来源、也无标注来源 ⇒ 绑体需要 T、得 T 需要绑体 ⇒ 鸡蛋。C# 靠 target-typing + 多阶段
推断部分破解，z42 v1 不引入。**有标注**（`(int x) => …`）经源②可解，已支持。

### D4：返回位型参不回灌（B2 边界）

`Map<T,U>(source, Func<T,U> f)`：T 由 source 推出、代换进 `Func<T,U>` 使 lambda 形参具体、体能编；
U 只在 lambda 返回位。可在绑定后从 `lambda.ExprBody.Type()` 推出 U 供**诊断**（where 校验），但**不
回灌** `bc.MethodTypeArgs`（#536 D4）⇒ 调用方看到的 `List<U>` 里 U 仍裸。v1 不额外做返回位诊断推断
（价值低、且 block-body lambda 无单一返回类型），留 Deferred。

## 新增/改动点

| 位置 | 改动 |
|---|---|
| `TypeArgInference.z42` | 新 `InferPreBinding`（两源、部分绑定、读 AST lambda 标注）+ 复用 `_unify` |
| `MethodTypeArgSubst.z42` | 新 `SubstituteFuncParams`（只换 Func 形参位的已绑型参名，复用 `ByName`） |
| `MemberResolver.z42` | 泛型自由/静态函数路径：`_withDefaults` 前插入「推断→代换 Func 位→用 bindMs 绑」 |

## 测试矩阵

- **运行期真门**（cross-zpkg 或 e2e fixture，`--mode interp`+`jit`）：真跑 `Map(nums, n => n*n)` /
  `Filter` / `Reduce` / `Array.Sort(xs, (a,b)=>b-a)`（无标注、省略 `<>`），断言输出正确
  ⇒ 证明 lambda 以具体类型绑定+执行。**这是本 change 唯一能证明「真能编能跑」的门**（编译期单测证不了 emit）。
- **编译期单测**（`z42c.semantics` typecheck / `bodyDiags`）：
  - 无标注 B（`f(arr, (x,y)=>…)`）：修前 E0402、修后 0（能编）+ where 违反可报（毒化解除）。
  - 有标注纯 A（`Compose((int x)=>…, …)`）：T 由标注推出、where 违反可报。
  - 无标注纯 A（`Compose((x)=>…)`）：仍静默（B1）——退回对照证明它不是被本 change 意外点亮。
  - 返回位 U（`Map`）：lambda 体能编、调用不报（B2）。
- **零漂移**：`xtask test` 全 stage + 自举不动点 3/3；`test stdlib --mode jit`；`test bootstrap`。
- **退回对照**：`git stash` 三个改动点分别退回，证明各新门修前 FAIL / 各毒化用例修前编不过。
