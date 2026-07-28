# Tasks: 补全查询 API（Phase 1，REPL-first）

> 状态：🟡 SPEC READY（D1–D5 已裁决）；排队等 `compiler` 锁 | 创建：2026-07-28
> 占用子系统：`compiler` + `toolchain` + `runtime`

## 裁决摘要（见 proposal.md）
- D1=A（只建内核 + REPL 补全）｜D2=混合（会话变量 live 反射 / 类型名编译期符号 / 任意 expr defer）
- D3=接受透出通道｜D4=轻量 CompletionItem 绕过 ISymbol｜D5=spike 先行

## 阶段
- [x] **阶段 0 — D5 spike ✅ 通过（2026-07-28，runtime 锁空闲下完成）**：rustyline `Completer` ↔ VM 回调**可行**
  - **风险 A（VM 重入回调）✅**：新 builtin `__repl_complete_probe(fqn,line,pos)` 经 `complete_via_callback` → `exec_function` 回调 z42 完成器，真实 VM 跑 `Spike.MyComplete("Co",2)` → 返回 `[Console,Convert,Contains,Copy]`（正确排除 Zebra）。无需 PTY。
  - **风险 B（rustyline 接线）✅**：`Editor<ReplHelper,DefaultHistory>` + `ReplHelper` 实现 `Completer`（Hinter/Highlighter/Validator 用默认）；`__repl_set_completer(fqn)` 注册；**thread-local `ACTIVE_CTX` 原始指针**在 `ed.readline()` 前后 set/clear 把 live `&VmContext` 递给 Completer（sound：同线程、仅该 readline 跨度）。PTY 驱动实测：输入 `Conso`+Tab → rustyline 补成 `Console` → 程序读到 `GOT:Console`。
  - **落地形态（定型，供阶段 3/4 复用）**：`complete_via_callback(ctx, fqn, line, pos) -> string[]` 共享核 + `__repl_set_completer` 注册 + rustyline Completer（thread-local ctx）。完成器契约：`string[] complete(string line, int pos)`。
  - 文件：`src/runtime/src/corelib/repl.rs`（+ mod.rs 注册 2 个 builtin）。**只动 runtime**（scratch z42 程序自带 extern，不碰锁住的 toolchain/stdlib）。
  - 结论：**D5 无需回退 proposal**；rustyline↔VM 机制成立，Phase 1 可照此接线。
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
