# Proposal: 用户自定义类型转换（implicit / explicit operator，比 C# 更严更可预测）

## Why

类型转换体系三 PR 阶梯的收官 PR。PR1（#169）立分类器地基（预留 `UserImplicit`/`UserExplicit`
两种类 + `Method` 字段），PR2（#174）收紧内建隐式窄化/有损转换。**PR3 把用户自定义转换填实**：
让用户能写 `public static implicit operator Target(Source s)` / `explicit operator`，并在
`(T)x` 显式转换与隐式赋值/return/传参处自动触发——同时**修掉 C# 用户转换的设计硬伤**，令 z42
比 C# 更严、更可预测。

C# 用户转换的已知硬伤（User 已分析确认）：①隐式转换隐形、搅乱可读性 + 重载决议；②`as`/`is`/
模式匹配完全不认用户转换；③不可传递 + "最多一个用户转换" 反直觉、错误信息不提示走中间类型；
④冲突不在**声明期**查、推迟到调用点。PR3 针对性改进 ②（部分，见 Out of Scope）③④。

## What Changes

- **词法**：新增 `implicit` / `explicit` 两个关键字（reserved words）。**support 先行**——z42c/stdlib
  源自身晚一个 nightly 再用（自举纪律，见 [bootstrap-seed.md](../../../../.claude/rules/bootstrap-seed.md)）。
- **语法**：`MemberParser` 扩展成员解析，吃 `public static implicit operator Target(Source s)` /
  `explicit operator` → 方法名 `op_Implicit` / `op_Explicit`（镜像 C#），静态、单参、返回类型 = Target。
- **`(T)x` 消歧**：`ExprParser` 放宽 C 风格 cast 识别，让 `(UserType)operand` 解析为 `CastExpr`
  （此前用户类 cast 故意不解析、要求写 `as`）。
- **语义分类器**：`Conversion.Classify` 填 `UserImplicit`/`UserExplicit` 两支——在内建转换给出
  `None` 时回退查 from/to 类型上的 `op_Implicit`/`op_Explicit`（精确 (源,目标) 匹配），命中返回带
  `Method` 的 `ConvResult`。
- **lowering**：隐式上下文（赋值/return/var-decl/传参/数组元素）的 `ConvertIfNeeded` 与显式
  `(T)x` 的 `_bindCastExpr` 把用户转换 **lower 成 `BoundCall`**（复用 Call opcode，**无格式 bump**）。
- **RegKey 消歧**（根因修复）：转换运算符按 (param, **return**) 唯一键——静态方法现仅按参数类型
  mangle（`op_Implicit$1$Foo`），两个同源不同目标的转换（`operator int(Foo)` + `operator string(Foo)`）
  会**撞键**；转换运算符 RegKey 附加返回类型消歧。
- **② 声明期冲突检测**（比 C# 好）：同 (源→目标) 重复、或 implicit+explicit 同对 → **声明处**报错
  E0440（C# 推迟到调用点）。
- **③ 走中间类型诊断**（比 C# 好）：`(C)x` / 隐式转换失败且 A→C 不存在但 A→B→C 存在 → 错误信息
  提示 "写 `(C)(B)x`"。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.syntax/src/TokenKind.z42` | MODIFY | 新增 `Implicit` / `Explicit` int 常量 |
| `src/compiler/z42c.syntax/src/Lexer.z42` | MODIFY | `_initKeywords` 注册 `implicit` / `explicit` |
| `src/compiler/z42c.syntax/src/MemberParser.z42` | MODIFY | 解析 `implicit/explicit operator Target(Source s)` → op_Implicit/op_Explicit |
| `src/compiler/z42c.syntax/src/ExprParser.z42` | MODIFY | 放宽 `(UserType)operand` cast 识别 |
| `src/compiler/z42c.semantics/src/Conversion.z42` | MODIFY | 填 UserImplicit/UserExplicit 查找（`_classifyUser`） |
| `src/compiler/z42c.semantics/src/TypeChecker.z42` | MODIFY | `ConvertIfNeeded` 加 `syms` 参 + user-implicit lower 成 Call；`BoxArgs` 透传 syms；③ 中间类型诊断助手 |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | MODIFY | `_bindCastExpr` user-conv lower 成 Call；ConvertIfNeeded 调用点传 syms |
| `src/compiler/z42c.semantics/src/StmtBinder.z42` | MODIFY | ConvertIfNeeded 调用点传 syms |
| `src/compiler/z42c.semantics/src/OverloadBinder.z42` | MODIFY | ConvertIfNeeded / BoxArgs 调用点传 syms |
| `src/compiler/z42c.semantics/src/SymbolCollector.z42` | MODIFY | 转换运算符 RegKey 附返回类型消歧 + ② 声明期冲突检测 |
| `src/compiler/z42c.core/src/DiagnosticCodes.z42` | MODIFY | 新增 E0440（转换运算符声明冲突） |
| `src/compiler/z42c.semantics/src/SemanticDump.z42` | MODIFY | 加 `FirstErrorMessage`（测试支持——断言 ③ 走中间类型提示文本） |
| `scripts/install/xtask_install_vscode.z42` | MODIFY | `_kwModifier` 加 `implicit`/`explicit`（grammar 分类，防 vscode-syntax 漂移；需重建 xtask.zpkg） |
| `docs/roadmap.md` | MODIFY | Deferred Backlog Index 加 user-conversions-future 行 |
| `src/compiler/z42c.semantics/tests/conversion/conversion_tests.z42` | MODIFY | 加分类器/lowering/E0439/③ 诊断单测（4 个，复用现有 SemanticDump 助手，比新建 dir 省样板） |
| `src/compiler/z42c.semantics/tests/collect/collect_tests.z42` | MODIFY | 加 ② 冲突（E0440 ×2）+ RegKey 唯一单测（3 个，复用现有 collect/hasCode 助手） |
| `src/tests/user-conversions/implicit_explicit.z42` | NEW | e2e：隐式（赋值/return/assign）+ 显式 `(T)x` 运行期行为（interp+jit） |
| `src/tests/cross-zpkg/user_conv_cross_pkg/{target,ext,main}/...` + `expected_output.txt` | NEW | 跨包用户转换 e2e（转换运算符随 TSIG 导出） |
| `docs/book/src/compiler/type-conversion.md` | MODIFY | 补「用户自定义转换」节（机制 + ②③改进 + 与 C# 对比） |
| `src/compiler/z42c.syntax/README.md` | MODIFY | 功能索引：conversion operator 解析 |
| `src/compiler/z42c.semantics/README.md` | MODIFY | 功能索引：用户转换分类 + 冲突检测 |

**只读引用**：
- `src/compiler/z42c.semantics/src/Symbol.z42` — MethodSymbol / Z42FuncType 形状
- `src/compiler/z42c.semantics/src/SymbolTable.z42` — GetClass / 类方法枚举
- `src/compiler/z42c.semantics/src/OverloadResolver.z42` — MangleKey（RegKey 消歧复用）
- `src/libraries/z42.ir/src/StrMap.z42` — Keys() 枚举

## Out of Scope

- **`as` / `is` / 模式匹配接入用户转换**（C# 硬伤②）——涉及可失败语义，留 Deferred（design Deferred 段）。
  PR3 只做 `(T)x` 显式 + 隐式上下文。
- **用户转换与标准转换的组合链**（C# 的 "标准转换 + 一个用户转换 + 标准转换"）——PR3 用**精确
  (源,目标) 匹配**（更简单、更可预测），多跳由 ③ 诊断引导用户手写。留 Deferred。
- **z42c / stdlib 源自身使用用户转换**——晚一个 nightly（自举 support-先行纪律）。
- 无格式 bump（用户转换 lower 成既有 Call opcode）。

## Open Questions

- 无（两个设计分叉已由 User 裁决：保留 implicit+explicit 两者 + ②③ 全做）。
