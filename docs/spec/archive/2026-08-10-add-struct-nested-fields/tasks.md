# Tasks: 嵌套 struct 字段

- [ ] **T1** `StructLayout.IsBlobStruct`：去掉"含 Struct 字段即拒"循环；加 `Size==0` 防护（自引用兜底）
- [ ] **T2** `ExprEmitter`：加 `_structChainRoot` / `_structChainOffset` 辅助；`_emitMember` blob 分支
      前移到 `Emit(m.Target)` 之前 + 累积 offset 叶子读 + 整字段读出（`_copyRegion`）
- [ ] **T3** `ExprEmitter._emitAssign`：blob 成员写分支前移 + 累积 offset 叶子写 + 整字段写入
- [ ] **T4** `_copyRegion` 递归叶子复制辅助（复用 StructFieldGet/SetPrim；递归 Struct 子字段）
- [x] **T5**（收窄）`IsBlobStruct` 的 `Size==0` 防护即防自引用崩溃；E0438 显式诊断 → follow-up（E0438 号已预留注释）
- [x] **T6** golden `src/tests/types/struct_nested.z42`（嵌套 prim r/w + 整字段复制独立性 + owner 方法嵌套读 + copy-in + string ref 叶子）
- [ ] **T7** GREEN：CI ci-bootstrap 两代自举 + 全量 golden（本地被 pre-A-use 种子的 axis④ 解析墙挡住，CI 权威）
- [ ] **T8** docs：`docs/book/` struct 机制页加嵌套小节；归档本 change + 更新记忆

## follow-up（本 change 后另开）

- **E0438 `StructValueCycle` 诊断**：`SymbolCollector` 对"struct → 其 struct 值字段"图做 DFS 环检测发射
  （号已在 DiagnosticCodes 预留）。当前 `Size==0` 兜底已防崩（自引用 struct 退化引用语义不报错）。

## 验证要点

- 全仓无嵌套 struct 现存用法 → T1–T4 对现有测试零影响（回归面为空）
- E0438：读 `coll.Diags` 直接断言 code，别用 SemanticDump.ErrorCount（漏 collector diags）
- 无格式 bump / 无 cargo 改动 → 本地 warm（`./xt` 或 two-gen）即可全验，不触发格式 bump 环境墙
