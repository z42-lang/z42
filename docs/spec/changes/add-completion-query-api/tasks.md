# Tasks: 补全查询 API（Phase 1，REPL-first）

> 状态：🟡 SPEC READY（D1–D5 已裁决）；排队等 `compiler` 锁 | 创建：2026-07-28
> 占用子系统：`compiler` + `toolchain` + `runtime`

## 裁决摘要（见 proposal.md）
- D1=A（只建内核 + REPL 补全）｜D2=混合（会话变量 live 反射 / 类型名编译期符号 / 任意 expr defer）
- D3=接受透出通道｜D4=轻量 CompletionItem 绕过 ISymbol｜D5=spike 先行

## 阶段
- [ ] **阶段 0 — D5 spike（最大未知，先做）**：验 rustyline `Completer` ↔ VM 回调可行性
  - `corelib/repl.rs`：能否在 rustyline Completer 里回调进 VM 跑一个 z42 补全函数取候选？形态（callback 注册 / raw 模式）？
  - 出口判据：跑通"Tab → z42 侧返回固定候选列表 → rustyline 显示"最小闭环；否则回 proposal 调整 D5。
- [ ] 阶段 1 — `CompletionQuery`（`z42c.semantics`）：三查询面（ScopeSymbols / TypeMembers / NamespaceExports），封装 StrMap 枚举 + visibility 过滤 + 前缀过滤
- [ ] 阶段 2 — 语义透出通道（D3）：`PackageCompile.Compile` → `CompileArtifacts` 加 SemanticModel/context 视图
- [ ] 阶段 3 — REPL completer（`toolchain/scripting` + host）：组 CompletionContext（VarNames/Usings/DeclNames/DeclTypeNames/CachedScan + 透出的语义模型）→ 调 query；会话变量 `.` 走 live 反射（读 Vars{N} 字段值 + GetType + GetMembers，零副作用）
- [ ] 阶段 4 — 接线 Tab（`interactive_main` + repl.rs 钩子）
- [ ] 阶段 5 — 测试：CompletionQuery 单测（scope/成员/ns + 前缀）；REPL 补全 e2e（scope 名 / 类型名静态 / 会话变量成员）
- [ ] 阶段 6 — 文档：补全机制页（`docs/design/`）；`repl-future-tab-completion` 前置改"补全查询 API"；roadmap Deferred Index 更新；标注 LSP 为未来第二客户端（架构预留）

## 前置 / 阻塞
- **`compiler` 锁争用**：converge-z42c-onto-z42-project（排队）/ unify-run-modes 系列在动 → 按子系统互斥锁排队，锁空闲再登记 ACTIVE.md + 开工。
- 阶段 0 spike 结论可能反推 D5 形态调整（甚至 D1 范围）。

## 明确 defer（Phase 2+）
- 任意 `expr.` 成员补全（需 SemanticModel Expr 级类型推断，缺口⑤）
- LSP server / 协议（0.5.x）｜ISymbol（F2.2）｜签名帮助 / hover / 跳转
