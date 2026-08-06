# Tasks: readonly 字段修饰符 + 优化管线利用

> 状态：🟢 已完成 | 创建：2026-08-06 | 完成：2026-08-06

## 进度概览
- [x] 阶段 1: 语法层（readonly token）
- [x] 阶段 2: 符号 + 类型检查（IsReadonly + ctor 强制）
- [x] 阶段 3: IR + emit（FieldGetInstr.Readonly 内存标志）
- [x] 阶段 4: 优化管线（OptSet 位 + CSE + LICM）
- [x] 阶段 5: 测试（codegen / typecheck / golden / bench）
- [x] 阶段 6: GREEN + 文档 + 归档 + PR

## 阶段 1: 语法层
- [x] 1.1 `TokenKind.z42` 加 `Readonly = 150`
- [x] 1.2 `Lexer.z42 _initKeywords()` 注册 `readonly`
- [x] 1.3 `DeclParser.z42 _isModifier()` 加 Readonly；确认进 `FieldDecl.Mods`

## 阶段 2: 符号 + 类型检查
- [x] 2.1 `Symbol.z42 FieldSymbol` 加 `IsReadonly`（+ 构造签名）
- [x] 2.2 `SymbolCollector.z42` 本地字段填 IsReadonly（`_modsHas`）；`ImportedSymbolLoader.z42` 导入字段传 false
- [x] 2.3 `TypeEnv.z42` 加 `InCtorThis` 标志；`DeclBinder.z42` 绑 ctor 体 + 字段初始化器注入路径置 true
- [x] 2.4 `DiagnosticCodes.z42` 加 `ReadonlyAssignment = "E0415"`
- [x] 2.5 `ExprTyper.z42 _bindAssign` readonly 违规检查（MemberExpr + 裸 IdentExpr 两路），不脱轨

## 阶段 3: IR + emit
- [x] 3.1 `IrInstr.z42 FieldGetInstr` 加内存 `Readonly`（默认 false，不序列化——确认 ZbcWriter/Reader 不动）
- [x] 3.2 `ExprEmitter.z42 _emitMember` + `_lookupIdent` emit FieldGet 时从 FieldSymbol 填 readonly
- [x] 3.3 确认现有 FieldGetInstr 构造点全部适配（默认/重载）

## 阶段 4: 优化管线
- [x] 4.1 `OptSet.z42` 加 `ReadonlyLoad = 256`、`All = 511`、名字映射 `readonly-load`
- [x] 4.2 `IrOptInfo.z42 CseKey` 加 readonly-FieldGet 分支（obj stable → `fget|obj|field`）
- [x] 4.3 `IrOptPipeline.z42 _passCse` 门控 ReadonlyLoad + FieldSet 失效值号
- [x] 4.4 `IrLicm.z42` this 接收者 readonly FieldGet 外提（确认接收者寄存器约定）

## 阶段 5: 测试
- [x] 5.1 `codegen_tests.z42`：readonly CSE 消重 / 非 readonly 不消重 / ctor 不误合并 / this-LICM 外提 / 非 this 不提
- [x] 5.2 `typecheck_tests.z42`：ctor 合法 / 初始化器合法 / 方法内 E0415 / 跨对象 E0415 / 不脱轨
- [x] 5.3 `src/tests/optimization/readonly-field-hoist/` 运行时 golden（正确性）
- [x] 5.4 `src/libraries/z42.core/bench/readonly_field_bench.z42` bench fixture + A/B 数字记 PR

## 阶段 6: 验证 + 文档 + 归档
- [x] 6.1 GREEN：`xtask test` 全 stage（重建 worktree xtask 后跑；自举字节不动点确认不破）
- [x] 6.2 `docs/book/src/runtime/optimization-pipeline.md` 加 readonly-load pass 机制
- [x] 6.3 `docs/book/src/language/readonly-fields.md` 新页 + 挂 SUMMARY.md；`docs/features.md` 登记
- [x] 6.4 `docs/roadmap.md` Deferred Backlog Index 登记跨包 readonly / 非 this LICM
- [x] 6.5 归档 + commit + PR

## 备注
- 分支基于 **origin/main（5490286c, zpkg 0.34, 含 #121 关键字清理）**。
- **无格式 bump**（FieldGetInstr.Readonly 内存标志，不入 zbc）。
- 自举安全：z42c/stdlib 源不用 readonly（support-first）→ 新 opt 对其输出零影响，gen1==gen2 不破。
- 实施期确认点：接收者寄存器约定（%0=this?）、FieldGetInstr 构造点数量、CSE pass FieldSet 事件流。
