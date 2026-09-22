# Design：自由函数取引用的 target-typed 重载消解

## 决策一览

| # | 决策 | 取舍 |
|---|---|---|
| D1 | 消解语义 = **精确全签名匹配**（`cand.Signature.IsAssignableTo(targetFuncType)`） | z42 委托无变型，精确相容是唯一合法目标判据；用 `OverloadResolver.Resolve`（可隐式转+最具体）会选中随后被相容检查拒绝的候选 |
| D2 | 单一咽喉 = `ExprTyper.BindWithTarget` 加 `IdentExpr + target is Z42FuncType` 分支 | 赋值/字段初始化已走此通道；变量声明/return/实参位小接线即可复用 |
| D3 | `BoundFuncRef` 增 `RegKey` 字段，发射改用 `QualOf(FuncNs, RegKey)` | 选中非-primary 重载必须发 mangle 键；primary 时 RegKey==FuncName ⇒ 存量字节不变 |
| D4 | 实参位靠**延迟谓词**扩展（仅延迟**重载** funcref 实参） | 复用 lambda 实参的延迟/回填机制（`BindArgsToSignature`）；单份 funcref 实参仍急切绑、字节中性 |
| D5 | 新诊断 **E0477**（目标存在但零重载匹配）；无目标/多匹配仍用 **E0425** | 「写了 `Func<string> f = Parse` 但无 `()->string` 重载」是独立且常见的用户情形，值一个清晰码 |

## 消解算法（D1）

给定裸标识符 `id` 与目标 `Z42FuncType target`：

```
若 env.LookupVar(id.Name) != null           → 不是 funcref（局部变量），回落普通绑定
若 env.Symbols.ResolveFuncNs(id.Name)==null → 不是自由函数，回落普通绑定（静态成员/枚举/undefined 各自处理）
ns   = ResolveFuncNs(id.Name)
cands = env.Symbols.GetFuncCandidates(ns, id.Name)     // MethodSymbol[]，基名 FQN 分组
若 cands.Length == 1:                                   // 无重载：现状即走此，补 RegKey
    return BoundFuncRef(name, ns, imported, cands[0].Signature, cands[0].RegKey)
// 多重载：精确全签名过滤
matches = [ c ∈ cands | c.Signature.IsAssignableTo(target) ]   // 逐位 params + ret 精确相等
若 matches.Length == 1: return BoundFuncRef(name, ns, imported, matches[0].Signature, matches[0].RegKey)
若 matches.Length == 0: E0477（列出全部候选签名 + target）; return BoundError
若 matches.Length >= 2: E0425（列出匹配候选；仅泛型基类替换后同签名可致）; return BoundError
```

> `IsAssignableTo` 在两个 `Z42FuncType` 间已是**逐位 `_partEq(ParamTypes)` + `_partEq(Ret)`
> 精确相等**（[Z42Type.z42:509-524](../../../../src/compiler/z42c.semantics/src/Z42Type.z42)），
> 无变型 ⇒ `IsAssignableTo` 对称 ⇒ 直接当「精确匹配」原语用，无需另写 `_sigMatches`。

## 接线点（D2）

### 咽喉：`ExprTyper.BindWithTarget`（[:837](../../../../src/compiler/z42c.semantics/src/ExprTyper.z42)）

```z42
// 新增分支（置于 lambda 分支旁）：
if ((e is IdentExpr) && (target is Z42FuncType)) {
    BoundExpr fr = this._bindFuncRefTargeted(e as IdentExpr, target as Z42FuncType, env);
    if (fr != null) { return fr; }        // 命中自由函数（消解成功 or 已报 E0477/E0425）
    // 否则（局部变量 / 非自由函数）回落
}
return this._bindExpr(e, env);
```

`_bindFuncRefTargeted` = 上节算法；返回 `null` 表示「这个 ident 不是自由函数名」，交回
`_bindExpr` 走普通路径（局部委托变量读、静态成员等）。

### 变量声明 `T f = Parse;`（[StmtBinder.z42:300-317](../../../../src/compiler/z42c.semantics/src/StmtBinder.z42)）

现状：`IdentExpr` RHS 不匹配 lambda/coll/new 任一谓词 → 落 `_bindExpr(v.Init)`（无目标）。
加：`declared is Z42FuncType` 且 `v.Init is IdentExpr` → `BindWithTarget(v.Init, declared, env)`
（与既有 lambda 分支 `declared is Z42FuncType → _bindLambda` 并列）。

### return `return Parse;`（[StmtBinder.z42:288](../../../../src/compiler/z42c.semantics/src/StmtBinder.z42)）

现状 return 已以函数返回类型为 target 走 target-typed `new`。补：返回类型 `is Z42FuncType`
且被返回表达式 `is IdentExpr` → 经 `BindWithTarget`。

### 赋值 / 字段初始化：**已在通道内**

`AssignTyper.z42:153` `BindWithTarget(a.Value, target.Type(), env)`；
`BindInitValue`（[:180](../../../../src/compiler/z42c.semantics/src/AssignTyper.z42)）同。分支加进
`BindWithTarget` 即自动生效，**这两处零改动**。

### 调用实参 `xs.ForEach(Parse)`（D4）

`BindArgsToSignature`（[OverloadBinder.z42:36](../../../../src/compiler/z42c.semantics/src/OverloadBinder.z42)）
对延迟位（`args[i]==null`）调 `BindWithTarget(rawArgs[i], pt, env)`——只要实参被**延迟**，
即自动走 target-typed funcref。故只需让**重载 funcref 实参**进入延迟位：

- 延迟点 [MemberResolver.z42:476-477](../../../../src/compiler/z42c.semantics/src/MemberResolver.z42)：
  `IsTargetTypedNew || IsLambdaArg || IsNamedArg` → 加 `|| IsOverloadedFuncRefArg(arg, env)`。
- 新谓词 `IsOverloadedFuncRefArg(e, env)`：
  `e is IdentExpr && env.LookupVar(name)==null && ResolveFuncNs(name)!=null && GetFuncCandidates(ns,name).Length>1`。
  **只延迟重载的**（单份 funcref 实参保持急切绑定 → 字节中性）。`IsNamedArg` 已吃 env，故
  env-aware 谓词有先例。
- arg-shape 构建 [OverloadBinder.z42:467](../../../../src/compiler/z42c.semantics/src/OverloadBinder.z42)
  `_typeOfArgExpr` 同样加该谓词 → 返回 `null`（延迟位无类型），避免急切绑 → 误报 E0425。

**已知限制（同 lambda 实参）**：外层调用若在**委托形参**上重载，延迟位无类型无法参与外层
决议 → 可能 E0437/歧义。可接受，spec 记录。

## 表示层 & 发射（D3）

### `BoundFuncRef`（[BoundExpr.z42:100](../../../../src/compiler/z42c.semantics/src/BoundExpr.z42)）

增 `public string RegKey;`，构造器加参。既有 5 处（若有）构造点补传 `mfs.RegKey`。
`_bindIdent` 无目标单份路径（[:113](../../../../src/compiler/z42c.semantics/src/ExprTyper.z42)）
也改传 `mfs.RegKey`（primary ⇒ ==FuncName，字节中性）。

### 发射（[ExprEmitter.z42:194-201](../../../../src/compiler/z42c.semantics/src/ExprEmitter.z42)）

```z42
this._ctx.Emit(new LoadFnInstr(frdst, SymbolTable.QualOf(fr.FuncNs, fr.RegKey)));  // 原为 fr.FuncName
```

`fr.FuncImported` 仍触发 `TrackDepNamespace(fr.FuncNs)`。跨包非-primary 重载已由 #731 按 RegKey
精确导出/导入 ⇒ 跨包 LoadFn @RegKey 与同包同形。

## 诊断（D5）

| 码 | 场景 | 消息（要点） |
|---|---|---|
| **E0477（新）** | 目标委托类型在场，但**无**重载签名与之精确匹配 | ``no overload of free function `f` matches target delegate type `Func<...>`;`` 后列全部候选签名 |
| E0425（沿用，更新消息） | ①无目标位重载取引用（`var`/表达式语句/非委托目标）②目标在场但**多个**候选精确匹配（仅泛型基类替换后同签名可致） | ①原消息补一句「assign to a variable of the target delegate type to disambiguate」 ②`ambiguous ...`（列匹配候选） |

E0477 = 现有最高 E0474 之后的下一个空位。诊断下划线落在**标识符本身**（`id.Span`），与
methodof 一致。

## 为什么不复用 `OverloadResolver.Resolve`（D1 展开）

`Resolve` 吃 `Z42Type[] argTypes`、按**适用性（可隐式转）+ 最具体**选。若把委托 `ParamTypes`
当 argTypes 喂进去：对 `Func<int,int> f = g;`、候选 `int g(int)` 与 `int g(long)`，两者都「适用」
（int 可隐式转 long），最具体选 `g(int)`——本例结果碰巧对。但 `Func<long,long> f = g;` 时
`g(int)` 也会被判「适用」（方向搞反），而它随后过不了委托精确相容 → 选中即死。精确匹配
`IsAssignableTo` 从根上只保留**真能当此委托的**候选，无此类陷阱，且与 z42「委托逐位精确」
的既定语义一致。
