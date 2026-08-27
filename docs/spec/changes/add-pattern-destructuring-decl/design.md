# Design: 模式匹配 B —— 解构声明 `Point(x, y) = p;`

## 数据流（三层，全复用现有模式引擎）

```
源码 `Point(x, y) = p;`
  → 解析：StmtParser._isDeconstructDeclStart（lookahead `T(...)=`）+ _parseDeconstructDecl
          → DeconstructDeclStmt { Pattern Pat=PositionalPattern; Expr Init }
  → 绑定：StmtBinder._bindDeconstructDecl
          → PatternBinder.Bind（record/arity/字段类型校验 + 绑定注册 env）
          → PatternBinder.CheckIrrefutable（结构 + 类型精确匹配静态校验）
          → BoundDeconstructDeclStmt { BoundPattern Pat=BoundPositionalPattern; BoundExpr Init }
  → 发射：StmtEmitter（Emit(Init)=subj）+ PatternEmitter.EmitIrrefutable
          → 逐字段 field_get 直读 + 绑定别名，递归嵌套；无 IsInstance / 无失败分支
```

## 解析：与函数调用语句的消歧

`Point(x, y) = p;` 与函数调用语句 `Foo(a, b);` 的唯一区别在**闭合 `)` 之后的 token**：调用后随 `;`，
解构声明后随 `=`（`f(x) = y` 在 z42 非合法赋值 lvalue，故 `T(...)=` 是干净、无歧义的信号）。

`_isDeconstructDeclStart`：
1. 首 token 须 `Identifier`；用 `_skipTypeOffset(0)` 跳过类型名前缀（支持 `Ns.T` 点分 / `T[]` / 泛型）。
2. 其后须紧跟 `(`；配平括号深度扫到匹配 `)`（跳过嵌套子模式的括号）。
3. 匹配 `)` 之后须是 `=`。

分派点：`Parser.ParseStatement` 在 `_isVarDeclStart` 之后、表达式语句回落之前插入
`if (_isDeconstructDeclStart()) return _parseDeconstructDecl();`。`_parseDeconstructDecl` 复用
`_patP._parsePrimaryPattern()`（不走 or-链，得 PositionalPattern），消费 `=` + `_parseExpr(0)` + `;`。

## 绑定：复用 `_bindPositional` + 不可失败校验

`_bindDeconstructDecl`：
- `init = _bindExpr(d.Init, env)`；`pat = _pattern.Bind(d.Pat, init.Type(), env)`——绑进**当前 env**
  （后续语句可见，同 `is` 表达式语义，非 `PushScope`）。`Bind` 复用 `_bindPositional`：校验
  `Z42ClassType` + `IsRecord` + `!IsStruct` + arity==`OwnFieldCount`，逐字段类型经 `_fieldType`，递归绑定。
- `CheckIrrefutable(pat, init.Type(), span)`——静态保证不可失败：
  - **结构**：仅 `BoundWildcardPattern` / `BoundBindingPattern` / `BoundPositionalPattern`（递归）合法；
    其它（常量 / or / range / relational / 类型测试）→ 报错。
  - **类型精确匹配**：每个 `BoundPositionalPattern` 的 `expected.Name() == pp.Type.Name()`
    （顶层 expected=init 静态类型，嵌套 expected=父 record 的 `FieldTypes[i]`）——保证 `IsInstance` 恒真。
  - 诊断复用 `DiagnosticCodes.TypeMismatch`（同 `_bindPositional` 系列，不新增码、不碰 core 冷启动）。

## 发射：`EmitIrrefutable`（无 IsInstance / 无失败分支）

因 binder 已静态保证 irrefutable + 类型精确匹配，`IsInstance` 恒真、无失败分支——故**不复用**
`EmitMatch`（它对位置模式发 `IsInstance`+`BrCond`、强制 failL），而新增精简 lowering：

```
EmitIrrefutable(subj, pat, contL):
  BoundBindingPattern:  Locals.Put(name, subj); br contL        // 裸绑定：别名到 subj 寄存器
  BoundPositionalPattern:
    for i in 0..ElemCount:
      fReg = Alloc(ToIrType(FieldTypes[i]))
      FieldGet(fReg, subj, FieldNames[i])                        // 直读，不 as_cast（避 jit 误编）
      EmitIrrefutable(fReg, Elems[i], nextFieldL); StartBlock(nextFieldL)
    br contL
  其它（通配 / binder 保证不达）:  br contL
```

StmtEmitter：`subj = Emit(Init)` → `contL = Fresh("des_cont")` → `EmitIrrefutable(subj, Pat, contL)`
→ `StartBlock(contL)` 续正常控制流。

**jit 安全**：字段读 `FieldGetInstr(fReg, subj, name)` **直读 subject 寄存器**，绝不 `as_cast(subj)→field_get`
（record 值语义合成曾踩此坑，jit 误编）。裸绑定别名到既有寄存器、不接 field_get，安全。

## 字节不动点 / 格式

- 无 zbc/zpkg 格式 bump、无新 token/关键字、无新 runtime、无新 IR（复用 field_get/copy）。
- z42c / stdlib / xtask 源**不使用**解构声明 → 自举 gen1==gen2 天然成立（新代码路径在编 z42c 自身时从不触发）。
- syntax→semantics 新符号 `DeconstructDeclStmt` 被 semantics 消费——A1 已修 `_buildCompilerViaZ42c`
  retry-on-fail 冷启动环，此路径已收敛；clean-cold self-build 本地已验绿。

## 限制（初版 defer）

- 仅 record class + 位置形态。属性形态 `Point { X: x } = p`、struct record 解构、泛型 record 解构、
  元组模式后续独立特性。
- 类型约束为精确名匹配（不做 is-a 子类放宽）——被解构值静态类型须与模式类型完全一致。
