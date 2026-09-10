# Proposal: lambda 实参驱动方法级泛型类型实参推断

> change: `generic-inference-lambda-args` ｜ scope: `compiler` ｜ 无格式 bump
> 来源: [[add-associated-types-program]] 第五批 (#536) 的 Deferred `generic-inference-lambda-args`

## Why（gap 已在今天的 main 上重新核实，非照抄 Deferred）

#536 加的方法级类型实参推断在 `TypeArgInference.Infer` 里从**已绑定实参类型**反推型参，但对
**lambda 实参无效**。#536 的 proposal 把根因记成「lambda 绑成 `Z42UnknownType`」——**实测已过期**：
lambda 实参今天绑成 `BoundLambda`，其 `Type()` 是 `Z42FuncType`，但**形参类型复用了目标签名的裸
`T`**（`_bindLambda` 的 `finalSig = expected`，`StmtBinder.z42:463`），lambda 自己标注的类型另存在
`BoundLambda.ParamTypes` 字段、从没被推断消费。

真实世界的受益形态是**「无标注 lambda + 省略尖括号」**（`examples/generics.z42`：
`Filter(nums, n => n > 4)` / `Map(nums, n => n * n)` / `Reduce(nums, 0, (acc, n) => acc + n)`）。
探针实测（`z42c.semantics` 单测 harness，走完整 `TypeChecker.Infer`）：

| 形态 | 今天 | 说明 |
|---|---|---|
| `Srt(xs, (x,y) => y - x)` 无标注、省略 `<>` | **2×E0402** | 形参绑成裸 `T`，体内 `y - x` 无法定型 → 编不过 |
| `Srt(xs, (int x,int y) => y-x)` 有标注、省略 `<>` | 0（但两实参全被擦除放行，**零真实检查**） | |
| `Cvt(xs, x => x + 1)`（U 只能从 lambda 推） | **1×E0402** | 编不过 |

⇒ **这类调用今天全部编不过或零检查**。要让它们能编，必须**先从其它实参推出型参、再用具体类型重新
绑定 lambda**——正是 Deferred 原话「要动延迟绑定通道」的含义。

**顺带坐实一个既存缺陷（lambda 毒化）**：`f<T>(T[] a, Func<T,int> g)` 调 `f(arr, lam)` 时，即使
`arr` 本可绑 `T`，lambda 位的裸 `T` 会与之冲突 → 整条推断**失败**（`TypeArgInference` 边界②）→
连 where 约束都不校验。本 change 一并解除。

### 受益点量化（Explore 全仓扫描 origin/main）

- **纯 A（型参只在 lambda 里、无其它来源）**：4 个，全在 `examples/generics.z42`（Compose/Repeat/
  Option.Map/FlatMap）；stdlib 0、编译器 0。
- **B（型参既在普通形参又在 lambda 形参）**：15 个 = stdlib `Array.z42` ×12（Sort/Find/FindIndex/
  FindAll/Exists/TrueForAll/ConvertAll/ForEach/BinarySearch/…）+ examples ×3；编译器 0。
- **省略 `<>` + lambda 的真实调用点**：4 个，全在 examples，lambda **全部无标注**；**编译器源码内 0、
  build 源码（stdlib/编译器）内 0**。stdlib 的 `Array.*` 调用今天一律写显式 `<int>` 来绕开。

⇒ 有用版本对**自举 + stdlib 构建零字节漂移**（build 源零命中该形态，且不动点门兜底）；
只有 `examples/generics.z42`（无门）会新编过。

## What Changes（Scope 2 + 纯 A，User 2026-09-11 裁决）

让隐式泛型调用**用推断出的类型实参重新绑定 lambda 实参**，使无标注/有标注 lambda 体拿到具体形参类型
能编、能跑。复用显式 `<>` 路径已有的代换机器（`MethodTypeArgSubst.ForExplicitTypeArgs` 的手法），但：

1. **两个推断源**，都在 lambda 绑定**之前**跑（新 `InferPreBinding`，返回**部分**绑定、不要求全绑）：
   - **源①（非-lambda 实参）**：`sig.ParamTypes[i]` vs 已绑非-lambda 实参的 `Type()`——覆盖 B 类
     （`Array.*` / Filter / Map / Reduce：型参从数组/source 推出）。
   - **源②（lambda 自身标注）**：lambda 延迟位（`rawArgs[i]` 是 `LambdaExpr` 且形参是 `Z42FuncType`）
     时，用 lambda **AST 里带标注的形参类型**（`env.ResolveType(lam.Params[j].Type)`）unify 形参位
     ——覆盖纯 A（`Compose<T>(Func<T,T>,Func<T,T>)` 调 `Compose((int x)=>…, (int y)=>…)`：T 从
     lambda 标注推出）。无标注位跳过（无信息）。
2. **只代换 func 类型的形参位**（新 `SubstituteFuncParams`，只换**已绑定**的型参名）：非-lambda 形参位
   **保持原裸型参** ⇒ 非-lambda 实参绑定与检查**逐字节等于今天**（`Array.Copy<T>` 等 112 处无 func 位
   者零触碰，避开 #536 D4 记录的四条漂移通道）。
3. **绑定**：`_withDefaults` / `BindArgsToSignature` 用「func 位已代换」的签名绑 lambda 延迟位 ⇒
   lambda 形参拿到具体类型、体内运算定型、发射具体 opcode。
4. **不回灌 `bc.MethodTypeArgs`**（沿 #536 D4）：opcode 仍 `Op.Call`、native 快路径门不动、
   zbc 串池不重排。callee 运行期消费型参的情形仍由 **E0455**（阶段 D）要求显式写 `<T>`。

## 边界（v1，呈 User 确认；纯 A 已纳入）

- **B1（唯一真正的收口 = 无标注纯 A）**：型参**只**出现在 lambda 里、**且该 lambda 形参无标注**
  （`Compose((x) => …)`）——鸡蛋依赖（不绑体拿不到型参、不知型参绑不了体），**无解**、留今天行为
  （静默）。这是 C# 也需要 target 才能破的形态，非本 change 可及。**有标注的纯 A 已支持**（源②）。
- **B2（只在 lambda 返回位的型参）**：如 `Map<T,U>(source, Func<T,U> f)` 的 `U`——`T` 由 `source`
  或 lambda 形参标注推出、代换进 `Func<T,U>` 形参位使 lambda 体能编，但 `U` **不回灌** ⇒ 调用方看到的
  返回类型 `List<U>` 里 `U` 仍是裸型参（同 #536 D4 不回灌的代价）。lambda **体**照常能编（形参具体）。
- **B3**：非-lambda 实参绑定、CheckArgTypes、重载决议**全部不变**（I1 对非-lambda 位保持）；
  emit 变化**只**发生在 lambda 体，且 build 源零命中 ⇒ 自举/stdlib 不动点 3/3 gen1==gen2。

## 验证要点

- **运行期真门**（e2e / cross-zpkg fixture）：真跑一个 `Map(nums, n => n*n)` 形态的调用，断言输出正确
  —— 证明 lambda 确实以具体类型绑定+执行（不只是编译期不报）。
- **编译期门**（`z42c.semantics` 单测，`bodyDiags` harness）：无标注 lambda + 省略 `<>` 从
  「2×E0402」变「0」；毒化解除（`f(arr, lam)` where 约束真校验）；纯 A 仍静默（B1 边界）。
- **零漂移**：`xtask test` 全 stage + 自举不动点 3/3；退回对照坐实新门修前 FAIL。
- **bootstrap**：无格式 bump；`test bootstrap` 绿。USE（`Array.*` 调用点去掉 `<int>`）属**下一
  nightly** 的独立 change（[[bootstrap-seed]] 轴①），本 change 不改 build 源调用点。

🤖 Generated with [Claude Code](https://claude.com/claude-code)
