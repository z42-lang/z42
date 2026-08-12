# Tasks: 用户自定义类型转换（implicit / explicit operator）

> 状态：🟢 已完成 | 创建：2026-08-12 | 完成：2026-08-12

## 进度概览
- [x] 阶段 1: 词法 + 语法（关键字 + 成员解析 + `(T)x` 消歧）
- [x] 阶段 2: 符号收集（RegKey 消歧 + ② 声明期冲突）
- [x] 阶段 3: 分类器 + lowering（Classify 用户支 + ConvertIfNeeded/cast lower）
- [x] 阶段 4: ③ 走中间类型诊断
- [x] 阶段 5: 测试（单元 7 过 + e2e interp/jit + 跨包 10/10）
- [x] 阶段 6: 文档 + 完整 GREEN（全 stage 绿）+ 自举 5/5 byte-identical + bootstrap 边界检查

## Scope 实施期细化（记录）
- 单测复用现有 `conversion_tests.z42` / `collect_tests.z42`（比新建 dir 省样板），非原计划新 dir。
- 加 `SemanticDump.FirstErrorMessage`（断言 ③ 提示文本）、`scripts/install/xtask_install_vscode.z42`
  `_kwModifier` 加 implicit/explicit（防 vscode-syntax 漂移，需重建 xtask.zpkg）、`docs/roadmap.md`
  Deferred 索引行——均已补入 proposal Scope。
- 顺带修 `type-conversion.md` 两处 pre-existing 死链（触及文档 doc-check）。

## 阶段 1: 词法 + 语法
- [x] 1.1 `TokenKind.z42`：加 `Implicit` / `Explicit` int 常量
- [x] 1.2 `Lexer.z42` `_initKeywords`：注册 `implicit` / `explicit`
- [x] 1.3 `MemberParser.z42` `_parseMemberBody`：`implicit`/`explicit` 后接 `operator Target(Source s)`
      → `_parseMethodTail(mods, ..., ret=Target, name=op_Implicit|op_Explicit)`
- [x] 1.4 `ExprParser.z42`：放宽 `(Ident)operand` cast 识别（Decision 8 的 follow-token 集）

## 阶段 2: 符号收集
- [x] 2.1 `SymbolCollector.z42`：静态方法 mangle 分支——op_Implicit/op_Explicit 的 RegKey 附 `$to$`+retCanon
- [x] 2.2 `DiagnosticCodes.z42`：加 E0440（ConversionOperatorConflict）
- [x] 2.3 `SymbolCollector.z42`：② 声明期冲突检测（同 (源,目标) 重复 / implicit+explicit 同对 → E0440）

## 阶段 3: 分类器 + lowering
- [x] 3.1 `Conversion.z42`：`Classify` = builtin + None 回退 `_classifyUser`（枚举 from/to 类的 op_ 方法）
- [x] 3.2 `TypeChecker.z42`：`ConvertIfNeeded` 加 `syms` 参 + 首部 UserImplicit → BoundCall；`BoxArgs` 透传
- [x] 3.3 `ExprTyper.z42` / `StmtBinder.z42` / `OverloadBinder.z42`：所有 ConvertIfNeeded/BoxArgs 调用点传 syms
- [x] 3.4 `ExprTyper.z42` `_bindCastExpr`：UserImplicit/UserExplicit → BoundCall；否则 BoundConvert

## 阶段 4: ③ 走中间类型诊断
- [x] 4.1 `TypeChecker.z42`：`_suggestVia(from, to, syms)` 助手（稳定序遍历，两跳）
- [x] 4.2 接入 `_bindCastExpr` 无转换失败 + `CheckImplicitConvert` TypeMismatch 分支，追加提示

## 阶段 5: 测试
- [x] 5.1 `z42c.syntax/tests/user-conversions`：conversion operator 解析 golden
- [x] 5.2 `z42c.semantics/tests/user-conv-conflict`：② E0440（断言 coll.Diags）
- [x] 5.3 `src/tests/user-conversions/implicit`：隐式赋值/return e2e
- [x] 5.4 `src/tests/user-conversions/explicit-cast`：`(T)x` + explicit-only 报 E0439 e2e
- [x] 5.5 `src/tests/user-conversions/intermediate-diag`：③ 诊断 golden
- [x] 5.6 `src/tests/cross-zpkg/user-conv`：跨包用户转换（条件性；TSIG 不携带则记 Deferred）

## 阶段 6: 文档 + 验证
- [x] 6.1 `docs/book/src/compiler/type-conversion.md`：补「用户自定义转换」节（机制 + ②③ + 与 C# 对比）
- [x] 6.2 `z42c.syntax/README.md` + `z42c.semantics/README.md`：功能索引更新
- [x] 6.3 `cargo build --release`（z42vm）无错
- [x] 6.4 `xtask test`（完整 GREEN gate：e2e / cross-zpkg / stdlib / compiler / vscode-syntax）
- [x] 6.5 `xtask test compiler` self-host 5/5 gen1==gen2
- [x] 6.6 `xtask test bootstrap`（旧 nightly 能编当前源）
- [x] 6.7 spec scenarios 逐条覆盖确认

## 备注
- 无格式 bump（lower 成既有 Call opcode）。
- 自举纪律：z42c/stdlib 源自身**不使用** implicit/explicit（support 先行，晚一 nightly）。
- lowering 若在 arg 位（BoxArgs 路径）blast radius 超预期 → 停下汇报（arg 位可退为 Deferred，covariance 位 v1）。
