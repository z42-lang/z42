# Design: 未定义类型引用诊断（E0443）

## Architecture

类型引用校验的单一choke point：

```
所有类型注解位置（局部/字段/参数/返回/基类/cast/as/is/typeof/new/default/catch/属性/索引器）
   │ 各自 env.ResolveType(te) / table.ResolveTypeP(te, 形参)   ← 泛型形参在此被解析为 GenericParamType
   ▼
_chkTypeRef(resolved, …, span)   （TypeChecker & SymbolCollector 两个薄 overload）
   ▼
AccessChecker.CheckTypeRef(resolved, currentClass, symbols, diags, span)   ← 唯一实体
   ├─ Z42InstantiatedType → 递归 Def + 每个 TypeArg
   ├─ Z42ArrayType        → 递归 Elem            ← 【新增】捕获 C[]
   ├─ Z42ClassType/Interface → 可见性校验（既有）
   ├─ Z42UnknownType 且 UnresolvedName != "" → 报 E0443【新增】
   └─ 其余（prim/泛型形参/func/匿名 Unknown）→ 放行（既有）
```

## Decisions

### Decision 1: 诊断在哪报
**问题**：未定义类型名解析成 `Z42UnknownType` 后如何在正确位置、带类型名地报错。
**选项**：
- A. 在 ~15 处 `_chkTypeRef` 调用点各自判断 —— 分散、易漏。
- B. 在 `AccessChecker.CheckTypeRef`（既有唯一choke point）判断 —— 集中、DRY、自动覆盖所有位置 + 泛型实参递归。
**决定**：选 B。它已是「类型引用校验」的根因位置（enforce-class-access 就在此），未定义类型校验属同一职责。

### Decision 2: 如何拿到未解析的类型名（报「undefined type: C」而非「<unknown>」）
**问题**：`CheckTypeRef` 只收到 resolved `Z42Type`；`Z42UnknownType.Name()` 恒 "<unknown>"，丢了原名。
**选项**：
- A. 把 `TypeExpr`/name 串通过 `_chkTypeRef`→`CheckTypeRef` 透传 —— 改 ~15 调用点 + 两 overload + 实体签名，面广。
- B. 让 `Z42UnknownType` 携带 `UnresolvedName`：`ResolveTypeP` 在 NamedType 未解析 fallthrough（`SymbolTable.z42` line 217）构造时填 `nt.Name`。`CheckTypeRef` 读 `resolved.UnresolvedName`。
**决定**：选 B。**零签名改动**、名字在「知道它」的唯一位置（解析 fallthrough）就地记录，符合根因原则。
`new Z42UnknownType()` 其它调用点（`var` 无 init、错误恢复）保持 `UnresolvedName == ""` → 不报（匿名 Unknown = 非「具名未定义类型」）。

### Decision 3: 为什么不会误报（`var` / 泛型形参 / 级联）
- **`var`**：`StmtBinder._varType` 在调用 `_chkTypeRef` **之前**特判 `nt.Name == "var"` 早返（走 init 推断），`var` 永不到 `CheckTypeRef`。
- **泛型形参**：`TypeEnv.ResolveType` 与 `SymbolCollector` 的解析都把活跃类型形参名传给 `ResolveTypeP`，形参名 → `Z42GenericParamType`（非 Unknown）→ 不落 fallthrough。
- **嵌套类型**：`TypeEnv.ResolveType` 未命中时沿 enclosing `+` 链重试；仍失败才保留带原名的 Unknown（此时确属未定义）。
- **级联抑制**：只有**具名** Unknown（`UnresolvedName != ""`）触发 E0443；表达式类型检查里作 sentinel 的匿名 Unknown（操作数/条件的 `_checkOperand`/`_requireBool` early-return）不受影响——它们不经 `CheckTypeRef`。

### Decision 4: 错误码 & 措辞
新增 `E0443 UndefinedType`（E04xx TypeCheck 段下一个空位，现最高 E0442）。措辞 `undefined type: <name>`，与 E0401 `undefined: <name>` 对称。

### Decision 5: 数组元素递归
`CheckTypeRef` 既有对 `Z42InstantiatedType`（泛型实参）递归，但无 `Z42ArrayType` 递归。补一条 `Z42ArrayType → 递归 Elem`，使 `C[]`（元素未定义）也报。成本一行分支。

## Implementation Notes
- `Z42UnknownType`：加 `public string UnresolvedName;` + `public Z42UnknownType() { this.UnresolvedName = ""; }`。既有 `new Z42UnknownType()` 调用点不变。
- `SymbolTable.ResolveTypeP` line 217：`Z42UnknownType u = new Z42UnknownType(); u.UnresolvedName = n; return u;`（`n` = `nt.Name`）。line 234（非 NamedType 的 TypeExpr 兜底）保持匿名 Unknown。
- `AccessChecker.CheckTypeRef`：在 `Z42InstantiatedType` 分支后加 `Z42ArrayType` 递归；在末尾「放行」前加
  `if (resolved is Z42UnknownType) { Z42UnknownType u = resolved as Z42UnknownType; if (u.UnresolvedName != "") { diags.Error(DiagnosticCodes.UndefinedType, "undefined type: " + u.UnresolvedName, sp); } return; }`。

## Testing Strategy
- 单元测试 `z42c.semantics/tests/typecheck/undefined_type/`：局部/字段/参数/返回/泛型实参/数组元素 各 → E0443；`var`/泛型形参/合法类型/内建 → 无 E0443。用 `coll.Diags`/`SemanticDump` 断言（注意 [semanticdump-errorcount-skips-collector-diags]：字段/参数/返回经 SymbolCollector.Diags）。
- **全量 GREEN + 自举**：这是**blast-radius 的权威验证**——当前 main 源码 + stdlib 若有任何合法类型现在解析成带名 Unknown 并落 `CheckTypeRef`，会变红暴露；全绿 = 无误报。红 → 停下报告，按暴露的合法模式调整（预期无）。
- REPL 侧：`C c` 现应报 `undefined type: C`（回归 Bug 2 表象）。

## Deferred / Future Work
### undefined-type-future-namespace-split
- **来源**：本 change proposal Out of Scope
- **触发原因**：z42 暂不区分「命名空间未找到」与「类型未找到」（C# 有 CS0234/CS0246）
- **前置依赖**：命名空间解析细化
- **触发条件**：若未来需要更精准的 using 缺失提示
- **当前 workaround**：统一报 E0443 undefined type
