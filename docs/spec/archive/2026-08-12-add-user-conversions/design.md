# Design: 用户自定义类型转换

## Architecture

```
Lexer          implicit/explicit 关键字
  │
Parser         MemberParser: `implicit operator T(S s)` → MethodDecl(op_Implicit, static, ret=T, param=S)
  │            ExprParser:   `(UserType)operand` → CastExpr
  │
SymbolCollector  转换运算符 RegKey = MangleKey + "$to$"+retCanon（(源,目标)唯一）
  │              ② 声明期冲突检测 → E0440
  │
Conversion.Classify  内建分支 None → _classifyUser(from,to) 查 op_Implicit/op_Explicit
  │                  → ConvResult{UserImplicit|UserExplicit, Method}
  │
TypeChecker      隐式上下文：ConvertIfNeeded(v, target, syms) → UserImplicit ? BoundCall(op_Implicit) : 数值/no-op
  │              ③ 转换失败 → 中间类型诊断
ExprTyper        显式 (T)x：_bindCastExpr → UserImplicit/UserExplicit ? BoundCall : BoundConvert
  │
IrGen            BoundCall → Call opcode（无新指令、无格式 bump）
```

## Decisions

### Decision 1: 保留 implicit + explicit 两者，用 ②③ 堵 C# 的坑（User 裁决）
**问题：** C# 最大痛点是 implicit operator 让转换隐形、搅乱重载决议。是否禁 implicit？
**选项：** A — 只保留 explicit，禁 implicit（消灭痛点，失表达力）；B — 两者都保留，用声明期冲突
检测（②）+ 走中间类型诊断（③）堵坑。
**决定：** 选 B（User 裁决）。保留 `Meters m = 5;` 的表达力，符合 C# 直觉；C# 的坑靠 ②（冲突提前到
声明期）③（多跳给提示）+ 本身"精确匹配、不组合链"（Decision 5）比 C# 更可预测来堵。

### Decision 2: 分类器回退——内建优先，用户转换兜底
**问题：** 用户转换与内建转换的优先级？
**决定：** 内建优先（镜像 C#）。`Classify` 保持现有内建分支不动，仅在其得出 `None` 时回退
`_classifyUser`。实现：现 `Classify` 体重命名为 `_classifyBuiltin`，新 `Classify` = builtin；
`Kind==None` 时 `return _classifyUser(from,to,symbols)`（无用户转换则仍 None）。
**副作用中立**：现有代码无用户转换 → 回退恒 None → 行为与 PR2 一致。

`_classifyUser(from, to, symbols)`：
1. 在 `from` 类与 `to` 类（`symbols.GetClass(name)`，各自可能 null）的 `Methods` 上枚举
   （`StrMap.Keys()`）。
2. 命中条件：`MethodSymbol` 满足 `IsStatic` ∧ `Name=="op_Implicit"|"op_Explicit"` ∧
   `Signature.ParamCount==1` ∧ `Canon(ParamTypes[0].Name())==Canon(from.Name())` ∧
   `Canon(Ret.Name())==Canon(to.Name())`。
3. op_Implicit 命中 → `ConvResult{UserImplicit, ms}`；op_Explicit 命中 → `{UserExplicit, ms}`。
   （② 保证同 (源,目标) 至多一个，故无歧义。）两类都无 → None。

### Decision 3: lowering——用户转换 lower 成静态 BoundCall（复用 op_Add 脱糖路径）
**问题：** 用户转换如何 lower？
**决定：** 复用运算符重载（op_Add）的静态调用脱糖：
`new BoundCall("static", false, null, ms.ContainingTypeName, ms.RegKey, [value], 1, ms.Signature.Ret, span)`。
- **隐式上下文**：`ConvertIfNeeded` 增 `SymbolTable syms` 参（11 调用点 + BoxArgs 透传，均有 env.Symbols
  在手）。在其**首部**（prim 守卫之前）加：`ConvResult r = Conversion.Classify(vt, target, syms);
  if (r.Kind==UserImplicit) return <BoundCall>;`。因 UserImplicit 已在 `ImplicitOk()` 白名单，
  `CheckImplicitConvert` 先行放行、此处只管 lower。
- **显式 `(T)x`**：`_bindCastExpr` 先 `Classify(operandType, targetType, env.Symbols)`；
  `UserImplicit|UserExplicit` → BoundCall；否则维持现有 `BoundConvert`（数值/引用 cast 不变）。

### Decision 4: RegKey 按 (param, return) 唯一（根因修复）
**问题：** 静态方法现仅按参数类型 mangle（`OverloadResolver.MangleKey` → `op_Implicit$1$Foo`）。
两个 `implicit operator int(Foo)` + `implicit operator string(Foo)` 同参不同返回 → **撞键**
（`ct.Methods.Put` 后者覆盖前者）。
**选项：** A — 症状级：禁止同源多目标转换（太严、不 C#）；B — 根因：转换运算符 RegKey 含返回类型。
**决定：** 选 B（根因，见 philosophy 根因修复）。SymbolCollector 静态方法 mangle 分支里，若
`md.Name` 是 `op_Implicit`/`op_Explicit` → `regName = MangleKey(name, paramTypes, 1) + "$to$" +
Z42Type.Canon(retCanon)`。RegKey 是「body 绑定 / IrGen / 导出 / 派发」单一真相源 → 一处改，全链一致。
无格式 bump（仅函数名字符串变化）。

### Decision 5: v1 精确 (源,目标) 匹配，不做标准转换组合链（比 C# 更可预测）
**问题：** C# 允许 "标准转换 + 一个用户转换 + 标准转换"（如 `int`→(用户)`Foo`，再 `Foo`→base）。
**决定：** v1 只做**精确**源/目标匹配（`(源,目标)` 与运算符签名逐字匹配）。更简单、消除 C# 里
"到底走哪条链" 的不可预测性。多跳（A→B→C）不自动组合，由 ③ 诊断提示用户手写 `(C)(B)x`。
留 Deferred（design Deferred 段）。

### Decision 6: ② 声明期冲突检测落点
**决定：** SymbolCollector 收集完一个类的方法后（RegKey 已定），扫描该类 op_Implicit/op_Explicit
运算符，按 `(Canon(param), Canon(ret))` 建索引：
- 同 (param,ret) 出现 ≥2（无论 implicit/explicit）→ E0440「conversion operator 'S'→'T' 已声明」。
- 同 (param,ret) 既有 op_Implicit 又有 op_Explicit → E0440「不能同时声明 implicit 与 explicit」。
报在第二个声明的 span。因 RegKey 已含返回类型，撞键本身不再静默覆盖（Decision 4），此检测是**面向
用户的显式诊断**（键唯一 ≠ 语义允许两条）。

### Decision 7: ③ 走中间类型诊断
**决定：** 助手 `_suggestVia(from, to, syms) → string`（中间类名或 ""）：遍历所有类的转换运算符，
找存在 `from→B`（op_Implicit/op_Explicit）且 `B→to` 的 B，返回首个（按类名 Ordinal 稳定序，遵
[common-pitfalls §1](../../../../.claude/rules/common-pitfalls.md)）。在两处失败点调用并追加提示：
- 显式 `(T)x` 无任何转换（`_bindCastExpr` 的 classify==None）：报错 + 若 `_suggestVia` 非空 → 追加
  「可经 'B'：写 (T)(B)x」。
- 隐式 `CheckImplicitConvert` 的 `TypeMismatch` 分支：同样追加。

### Decision 8: `(UserType)x` 解析消歧（parser 细节）
现 `ExprParser` 已有 `(Ident)Ident` cast 分支（泛型类型形参 cast）。放宽为 `(Ident)operand`：
`peek==LParen ∧ peekAt(1)==Identifier ∧ peekAt(2)==RParen ∧ peekAt(3) ∈ {Identifier, IntLit,
FloatLit, StringLit, CharLit, True, False, New}` → CastExpr。
- **保持歧义安全**：`(Ident) (` → 不匹配 → 仍是 call `Ident(...)`；`(Ident) -/+/*...` → 不匹配 →
  仍是二元（`(a) - b` 不误判为 cast）。这与既有注释「`(val)ident` 在 z42 无效语法」一致——只把
  "本就无效的相邻 primary" 收编为 cast。

## Implementation Notes

- **关键机制入口**（worktree 已核实行号，post-#174 origin/main）：
  - `Conversion.Classify` @ Conversion.z42:68；`ConvKind.UserImplicit=10 / UserExplicit=11`；
    `ConvResult.Method` 字段；`ImplicitOk()` 已含 UserImplicit。
  - `ConvertIfNeeded` @ TypeChecker.z42:101（加 syms 参）；`CheckImplicitConvert` @ :122；
    `BoxArgs` @ :83（透传 syms）。
  - `_bindCastExpr` @ ExprTyper.z42:675（现 `return new BoundConvert(...)`）。
  - 静态方法 mangle @ SymbolCollector.z42:765-766（`MangleKey`）；`ct.Methods.Put(regName, msym)` @ :788。
  - 运算符成员解析 @ MemberParser.z42:129（`operator` 分支，`_parseMethodTail`）。
  - C 风格 cast 分支 @ ExprParser.z42:~195-210。
  - 静态调用脱糖模板 @ ExprTyper.z42:819（op_Add）。
- **ContainingTypeName**：BoundCall 的类名取 `ms.ContainingTypeName`（转换运算符声明所在类）。
- **跨包**：转换运算符是静态方法，随 TSIG 导出（同 op_Add 跨包）。cross-zpkg golden 验证；若 TSIG
  未携带则记 Deferred（不阻塞同包 v1）。
- **无 VM 改动**：全部 lower 成既有 Call；`cargo build` 仅确认无破坏。

## Deferred / Future Work

### user-conversions-future-as-is: `as` / `is` / 模式匹配接入用户转换
- **来源**：本 spec Out of Scope（C# 硬伤②）
- **触发原因**：`as`/`is` 是可失败语义，与用户转换（可抛/单值）语义对齐需额外设计
- **前置依赖**：可失败用户转换协议（`op_Implicit` 无 Try 变体）
- **当前 workaround**：用 `(T)x` 显式转换

### user-conversions-future-conversion-chain: 标准转换 + 用户转换组合链
- **来源**：Decision 5
- **触发原因**：v1 精确匹配更可预测；组合链引入 C# 式重载决议复杂度
- **触发条件**：真实需求出现 `int`→用户`Foo` 且需 `Foo`→base 自动链
- **当前 workaround**：③ 诊断引导手写多跳 `(C)(B)x`

## Testing Strategy

- **单元测试**：
  - `z42c.syntax/tests/user-conversions`：conversion operator 解析产 op_Implicit/op_Explicit 方法（AST/dump）。
  - `z42c.semantics/tests/user-conv-conflict`：② 声明期冲突 → E0440（读 `coll.Diags` 断言，见
    [[semanticdump-errorcount-skips-collector-diags]] —— SymbolCollector 诊断不进 SemanticDump.ErrorCount）。
- **Golden / e2e**（`src/tests/user-conversions/`）：
  - `implicit`：隐式赋值/return 触发 op_Implicit，运行期值正确。
  - `explicit-cast`：`(T)x` 触发 op_Explicit；explicit-only 隐式上下文报 E0439。
  - `intermediate-diag`：③ A→B→C 提示（编译错误 golden）。
  - `cross-zpkg/user-conv`：跨包用户转换（条件性）。
- **VM 验证**：`xtask test`（完整 GREEN gate）。
- **自举**：`xtask test compiler` self-host 5/5 gen1==gen2（加特性但 z42c 源不使用 → 字节仍收敛）。
- **边界检查**：`xtask test bootstrap`（旧 nightly 能编当前源——当前源未用 implicit/explicit → 过）。
