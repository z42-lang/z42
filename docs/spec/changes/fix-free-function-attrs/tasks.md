# Tasks: fix-free-function-attrs

> 状态：🟡 进行中 | 创建：2026-09-22

**变更说明：** 顶层（自由）函数上的 attribute 第一次真正落进产物；`[Deprecated]` 贴在自由函数上现在**同包与跨包都会在调用点告警**。顺带给 cross-zpkg harness 加上「期望编过且某条告警响了」的设门能力。

**原因：** 自由函数的 attribute 链**五处全断**，而每一处单看都像「有实现」，所以静默失效至今无人发现：
① `IrGenAuxEmitter.EmitFreeFunctions` 一进循环就 `_unwrap` 剥掉 `AttributedDecl`，方法级 `irf.Attrs` 从不填（#679 只补了 `ParamAttrs` 那半）；
② `ExportedFuncZ` **根本没有** `IsDeprecated` / `DeprecationMsg` 字段（`ExportedClassZ` / `ExportedMethodZ` 都有）；
③ `TsigReconcile.Rebuild` 的自由函数分支无处搬运 `$Deprecated` 哨兵；
④ `ImportedSymbolLoader` 的自由函数导入循环没有 `sym.IsDeprecated =` 这行（类方法那两支都有）；
⑤ **纯同包的一处**：`MemberCollector` 收顶层 `MethodDecl` 时从不问 `HandlerRegistry.HasDeprecated`，且 `MemberResolver` 的自由函数调用分支没有 `CheckDeprecatedM` —— 所以连同包 `[Deprecated] void f()` 都零告警。

空 attr 块与「这个函数没写 attribute」**字节全等**，无错无警，是纯静默缺口。

**文档影响：** `docs/internals/src/compiler/attribute-pipeline.md`（载体表「顶层函数」那格曾是空头支票 + 四棒搬运链）、`docs/reference/src/language/attributes.md`（`MethodInfo` 贴法一栏「含顶层函数」不准确 + 自由函数反射拿不到）、`src/tests/cross-zpkg/README.md`（新 fixture 种类）、`docs/agent/rules/parallel-development.md`（供种后必须重建 xtask）。

## 实测（修复前，基线 main 134c7d962 自建的编译器）

| 形态 | 修复前 | 修复后 |
|---|---|---|
| 同包 `[Deprecated("use freeG")] int freeF()` 的调用点 | **零诊断** | `deprecated: \`freeF\` 已弃用（deprecated）：use freeG` |
| 同包 `[Deprecated("use g")] int A.f()` 的调用点（对照组） | 告警 ✅ | 告警 ✅ |
| 跨包 `Demo.DepTarget.OldApi()`（自由函数） | **零诊断**（`z42c build: 1 warning(s)`） | 告警 ✅（`2 warning(s)`） |
| 跨包 `Holder.HOld()`（类方法，对照组） | 告警 ✅ | 告警 ✅ |

阴性对照用两个二进制：基线树 `wt-oldctl`（main 134c7d962，独立 `xtask build all`）对同一份跨包 fixture 只报 1 条告警，缺的正是自由函数那条。

## 任务

- [x] 1.1 `MemberCollector`：顶层 `MethodDecl` 收集时问 `HandlerRegistry.HasDeprecated(cu.Decls[i])`（取**未剥壳**那份）并置 `MethodSymbol.IsDeprecated` / `DeprecationMsg`
- [x] 1.2 `MemberResolver` 自由函数调用分支：解析到 `MethodSymbol` 后调 `CheckDeprecatedM`（use-site 告警，与类方法同一条抑制通道 ⇒ `#suppress deprecated` 自动生效）
- [x] 1.3 `IrGenAuxEmitter.EmitFreeFunctions`：保住 `rawD = cu.Decls[i]`，填 `irf.Attrs = _attrRefs(rawD)` / `irf.AttrCount`（`$Cctor` 哨兵与自由函数无关，`_cctorFuncName` 只认 `ClassDecl` → 恒 ""）
- [x] 1.4 `ExportedFuncZ` 加 `IsDeprecated` / `DeprecationMsg`（**ctor 元数不变**，默认 false/""、构造后赋值——同 `ParamsFrom` / `TypeParams` 的旧种子 ABI 纪律）
- [x] 1.5 `TsigReconcile.Rebuild`：`ef.IsDeprecated = IrDeprecation.Has(f.Attrs, f.AttrCount)`（镜像 `_methodFromSig` 对 `ExportedMethodZ` 的同款搬运）
- [x] 1.6 `ImportedSymbolLoader` 自由函数循环：`fsym.IsDeprecated = fz.IsDeprecated`
- [x] 1.7 cross-zpkg harness 新增 `expected_build_warning.txt`：`XpkgJobZ.ExpectWarn` + `_crossExpectedBuildWarning` + `_awaitBuildPkg` 判定（编不过 → 红；编过但没这条告警 → 红）；这类 fixture **照常进 run 波**
- [x] 1.8 测试：cross-zpkg `deprecated_free_function/`（自由函数 + 类方法对照组，带 `expected_build_warning.txt` 与 `expected_output.txt`）；analyzer 单测三条（自由函数告警 / 无 attribute 不误报 / `#suppress` 生效）
- [x] 1.9 文档同步：attribute-pipeline.md（载体表注 + 四棒链）、attributes.md（反射拿不到自由函数 `MethodInfo`）、cross-zpkg README、parallel-development.md（供种后重建 xtask）
- [x] 1.10 GREEN：`xtask test` 全绿（6m22s，base 134c7d962）；harness 改动后单独重跑 `test e2e --dir cross-zpkg` 60/60。**门禁有牙实证**：把 `expected_build_warning.txt` 改成一条不存在的告警 → `FAIL deprecated_free_function (main build)`，59 passed / 1 failed，改回即绿
- [ ] 1.11 归档 + PR

## 本批发现、未修（已登记）

- **自由函数的 attribute 反射不可达**：`irf.Attrs` 现在填了、也进了 zbc，但用户拿不到自由函数的 `MethodInfo`——`GetMethods()` 要先有 `Type`，`methodof` 语法上强制带 owner 类型（`TypeOpTyper._bindMethodOfExpr` 先解析 `mx.OwnerName`）。已在 attributes.md 如实写明，**不在本 change 新增发现 API**。
- **`_hasNativeAttr` 的侧效应不成立**：`TsigReconcile.Rebuild:261` 按 `f.Attrs` 过滤 `[Native]`，填 `Attrs` 后理论上会改变自由函数导出集——但 `EmitFreeFunctions` 只处理 `md.HasBody`，而 native 函数是 `extern` 无体，走 `StubEmitter` 另一条路，**根本到不了这里**。实测导出集无变化。
- **`docs/reference/` 全书没有 `[Deprecated]` 的用户向页面**（只有 `syntax.md` 提了 `deprecated` 这个 RuleId 可被 `#suppress`）。属先前缺口，不在本 change 范围。
