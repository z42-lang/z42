# Tasks: 参数修饰符收敛为单一 `ref`

> 状态：🔴 待 User 审批（阶段 6.5 gate 未过，**不得开始写代码**） | 创建：2026-09-21
> 分支/worktree：`ref-null-model` @ `/Users/d.s.qiu/Documents/z42-lang/wt-refnull`（基于 origin/main a50f7e897 #723）
> 类型：`lang` —— 完整流程（阶段 1–9）
> 阻塞项：proposal Q1（`ref` 是否参与重载）待裁决

## 进度概览
- [ ] 阶段 0: User 审批 proposal + spec + design
- [ ] 阶段 1: 语法层 —— 三态收敛为单态
- [ ] 阶段 2: 语义层 —— 对称性 / 类型 / 值类型限制
- [ ] 阶段 3: 发射层 —— 零值初始化 + `ref _`
- [ ] 阶段 4: 迁移 —— 测试 / scripts / 内嵌源串
- [ ] 阶段 5: 自举 + GREEN
- [ ] 阶段 6: 文档同步 + 归档

---

## 阶段 0: 审批
- [ ] 0.1 User 审批 proposal.md
- [ ] 0.2 User 裁决 Q1（A：不参与重载 / B：参与重载）；若选 B 则回改 spec §重载 + 增阶段 2b
- [ ] 0.3 User 审批 specs/ref-parameters/spec.md + design.md
- [ ] 0.4 User 明确「可以开始」→ 阶段 6.5 gate 通过

## 阶段 1: 语法层
- [ ] 1.1 `Lexer.z42:475` —— `out` 移出关键字表（`in` 保留，foreach 用）
- [ ] 1.2 `MemberParser.z42:340` —— 形参侧只认 `TokenKind.Ref`；遇 `out`/`in` 发 `ObsoleteParamModifier` + 迁移提示，并消费 token 保持解析同步
- [ ] 1.3 `ExprParser.z42:239` —— 调用点只认 `TokenKind.Ref`；遇 `out`/`in` 同上
- [ ] 1.4 `ExprParser.z42:241` —— `ref var n` 沿用现 `IsVarDecl` 路径；新增 `ref int n`（显式类型）
- [ ] 1.5 `Ast.z42:25` —— `RefArgExpr` 增 `IsDiscard`；解析 `ref _`
> ⚠️ 原 2.6「限值类型」**已撤销**（实测：`escape_ref_param_writeback` 用 `ref string[]`，且它是 #690 的回归测试）
- [ ] 1.6 `Decl.z42:12` —— `Param.IsRef` 注释更正（删去 "ref/out"）
- [ ] 1.7 syntax 单测：`z42c.syntax/tests/decl.z42` + `parser.z42` 增阴性用例

## 阶段 2: 语义层
- [ ] 2.1 `DiagnosticCodes.z42` —— 5 个新码（见 design §D8），取 E04xx 未占用值
- [ ] 2.2 `ExprTyper.z42:300` —— `BoundRefArg` 携带 inner 真实类型（替掉 `Z42UnknownType`）
- [ ] 2.3 `ExprTyper._bindRefArg` —— `ref var` 的变量按**形参类型**定义（现为 `Z42UnknownType`）
- [ ] 2.4 `CallEmitter` / `ExprTyper` 调用绑定 —— 修饰符对称性检查（双向：缺 / 多）
- [ ] 2.5 同上 —— 实参/形参类型精确匹配（Canon 归一别名，**不做**隐式转换）
- [ ] 2.7 确认 `_chkRefArgLvalue`（四种不可取址形态 → E0470）不受影响

### 阶段 2b（**仅当 Q1 选 B**）
- [ ] 2b.1 `OverloadResolver.MangleKey` —— 键含修饰符
- [ ] 2b.2 跨包签名 / `TypeNameResolver.TsigTypeName` 带 `ref`
- [ ] 2b.3 元数据编码 + 可能的格式 bump（走 `version-bumping.md` 的 CI artifact 配方）

## 阶段 3: 发射层
- [ ] 3.1 `FunctionEmitter` —— 函数级预扫描，收集被 `ref` 传过的局部名（遍历 `BoundRefArg.Inner`）
- [ ] 3.2 `StmtEmitter.z42:30` —— 无 init 且命中预扫描集合 ⇒ 分配寄存器 + 发零值常量；**其余维持「不发 IR」**
- [ ] 3.3 struct 局部的零初始化走 `StructAlloc`（按布局清零），验证与 `default(T)` 路径一致
- [ ] 3.4 `ExprEmitter` —— `ref _` 分配匿名寄存器 + 零值，不登记 `Locals`
- [ ] 3.5 `ForwardGenerator.z42:384` —— 确认只生成 `ref`（三态消失后应自然正确，补用例验证）
- [ ] 3.6 `CtorInheritance.z42:185` —— `IsRef` 透传不变，注释更正

## 阶段 4: 迁移
- [ ] 4.1 `src/tests/refs/out_var/` → `ref_var/`，源码改 `ref var`
- [ ] 4.2 `src/tests/refs/in_param/` 删除；新增 `src/tests/refs/obsolete_modifiers/` 阴性用例（`out`/`in` 各一）
- [ ] 4.3 新增 `src/tests/refs/ref_missing/`（漏写 `ref` → 报错）、`ref_discard/`（`ref _`）、`ref_zero_init/`（无 init 局部传 ref）、`ref_struct/`（struct 传 ref，**此前无覆盖**）
- [ ] 4.4 `scripts/test/xtask_*.z42` —— 12 处 `out` 改写（4 个文件）
- [ ] 4.5 `z42c.semantics/tests/{codegen,layout}` —— 内嵌源串里的 `out`/`in`
- [ ] 4.6 全仓 grep 清零：`\b(out|in)\s+[A-Za-z_]` 在形参/调用点位置无残留

## 阶段 5: 自举 + GREEN
- [ ] 5.1 按 `bootstrap-seed.md` 走冷种子（改 parser ⇒ 新编译器必须能编自己）
- [ ] 5.2 `xtask build` 全绿
- [ ] 5.3 `xtask test all` 全绿；`cargo test -p z42 --lib`（**debug，不能 `--release`**，见 memory：`--release` 会把 `debug_assert!` 编掉 → 本地全绿 CI 四红）
- [ ] 5.4 golden 核对：**除被 `ref` 传过的局部所在函数外，指令流应 byte-identical**；若有额外差异 ⇒ 停下查（D1 选项 B 的健康信号）
- [ ] 5.5 确认无 zbc/zpkg 格式 bump（`IsRef` 不进元数据）
- [ ] 5.6 推 PR 过 CI（`scripts/test/` 在 ci-bootstrap 路径上，本地不可验）

## 阶段 6: 文档 + 归档
- [ ] 6.1 `docs/reference/src/language/parameter-modifiers.md` 全文重写：单 `ref`、零契约、槽位零值、`ref var` / `ref _`、值类型限制
- [ ] 6.2 同文件 —— 删除「编译期 + 运行时全部已落地」的谎报；新增「为什么是 copy-in/copy-out 而非真别名」（design §D4，防后人重复怀疑）
- [ ] 6.3 `docs/internals/` —— 调用约定 / 逃逸分析相关页同步
- [ ] 6.4 `docs/spec/changes/fix-silent-semantic-gaps/` —— 剩余缺口第 1 条标为已由本变更解决
- [ ] 6.5 `docs/roadmap.md` —— 如有 ref/out/in 条目，同步为单 `ref`
- [ ] 6.6 归档到 `docs/spec/archive/YYYY-MM-DD-simplify-ref-parameters/`

---

## ✅ 已解除的硬约束
~~在 `record-ref-in-signature` 落地之前，不得让 stdlib 导出任何 `ref` 形参的公开 API。~~
**已解除**——`record-ref-in-signature` 已落地，跨包 `ref` 现在受检。原文保留如下：
跨包侧看不到 `ref`，导出等于把「漏写 ref 静默丢写入」推给用户包。
⇒ `enforce-value-type-non-null` 的 TryParse 迁移必须排在 `record-ref-in-signature` 之后。

## 后续 change（不在本变更范围）
- **`record-ref-in-signature`（优先级最高的 follow-up）** —— `TsigTypeName` / `ExportedParamZ`
  的类型串带 `ref` 前缀 + `ImportedSymbolLoader` 解析回 `IsRef` + minor bump
  （走 `version-bumping.md` 的 CI artifact overlay 配方，本地直接建会死锁）
- `optimize-readonly-alias` —— `CallEmitter._emitStructAwareArgs` 按「callee 不写该形参」跳过 `StructAlloc + StructCopy`（砍 `in` 的性能替代）
- `add-definite-assignment` —— DA pass；随之加「传 `ref` 前未赋值」警告 + 「直接读未赋值局部」错误
- `define-null-model` —— 值类型不可空 / `?` 标记 / 流分析 / `Expect` / 砍 `??` 与 `?.`
- `add-span-type` —— `Span<T>` / `ReadOnlySpan<T>`（普通 struct，不需 `ref struct`）
