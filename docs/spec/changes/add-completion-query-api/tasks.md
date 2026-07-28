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
- [x] **阶段 3（先做，作用域级）✅**：REPL 作用域 completer 落地（`toolchain/scripting`，隔离分支并行）
  - `Completer.z42`：自由函数 `replComplete(line,pos)` + `Completer.SetActive/GetActive`。数据源 = 当前 `ScriptState`（`VarNames` + `DeclNames`），**REPL 自持、不经 compiler**（Phase 1 洞察：作用域候选 REPL 已握，无需 `CompletionQuery`）。前缀提取 + 大小写敏感过滤 + 去重。
  - `Repl.z42`：加 `SetCompleter(fqn)` 绑定（`__repl_set_completer`）。
  - `interactive_main.z42`：注册 `Std.Scripting.replComplete` + 每轮 `Completer.SetActive(s)`（`.reset` 重建 s 亦覆盖）。
- [x] **阶段 4（作用域级）✅**：Tab 接线 = 阶段 0 的 rustyline 机制 + 阶段 3 注册，已通。**真实 REPL PTY 实测**：`var banana=42` → 输入 `ban`+Tab → 补成 `banana` → 求值 42。
- [x] **阶段 5（部分）✅**：`tests/repl_completion/`（driver + expected）——经 `__repl_complete_probe` 走**与 rustyline 同一运行期 FQN 查找**，验前缀过滤/去重/顺序（VarNames 先 DeclNames 后）+ 大小写敏感。PASS。
  - 注：`repl_decls_multiline` 本地 warm 失败是**陈旧种子 z42c**（2026-07-26，早于 #49 的 fix-imported-free-func-namespace 2026-07-28）所致，非本 change——去掉 `Completer.z42` 同样复现；CI（fresh z42c）为权威。
- [ ] 阶段 1 — `CompletionQuery`（`z42c.semantics`）：LSP 共享内核 + `TypeMembers`（类型名静态成员）。**排队等 compiler 锁**。作用域级已由阶段 3 用 REPL 自持数据先行交付；此阶段服务 LSP + 静态类型成员。
- [ ] 阶段 2 — 语义透出通道（D3）：`PackageCompile.Compile` → `CompileArtifacts` 加 SemanticModel/context 视图。**排队等 compiler 锁**。
- [ ] 阶段 3b — `obj.` 成员补全（D2 混合，**地基已勘定，可直接实施**）：会话变量走 live 反射，零副作用。
  - **静态字段 key 格式已确认**：`Repl.R{VarsRound}.Vars{VarsRound}.{varName}`（loader.rs:571，`Ns.Class.Field`）。
  - **runtime 读值**：`ctx.static_get(&key)`（vm_context.rs:1037，按 key 读 static field Value，未设为 Null）。
  - **成员枚举**：`builtin_type_members`（reflection.rs:1250，Type→fields+methods+properties+nested 名）；需 Value→Type（`make_type_from_name` / GetType 路径）。
  - **待做**：新 builtin `__repl_member_names(fqn)→string[]`（static_get + value→type→members）；completer 侧检测 `recv.prefix`、`recv∈VarNames` 时构 key 调 builtin、按 dot 后前缀过滤。任意 `expr.` defer（需静态类型推断）。
- [ ] 阶段 6b — REPL 语法着色（`repl-future-syntax-highlight`，**User 2026-07-28 暂缓、已记录**）：rustyline `Highlighter`（`ReplHelper` 已建、当前默认空实现）+ `Z42.Syntax.Lexer` 分色；无前置，随时可做。见 repl.md + roadmap Deferred Index。
- [ ] 阶段 6 — 文档：补全机制页（`docs/design/`）；`repl-future-tab-completion` 前置改"补全查询 API"；roadmap Deferred Index 更新；标注 LSP 为未来第二客户端（架构预留）

## 前置 / 阻塞
- **`compiler` 锁争用**：converge-z42c-onto-z42-project（排队）/ unify-run-modes 系列在动 → 按子系统互斥锁排队，锁空闲再登记 ACTIVE.md + 开工。
- 阶段 0 spike 结论可能反推 D5 形态调整（甚至 D1 范围）。

## 明确 defer（Phase 2+）
- 任意 `expr.` 成员补全（需 SemanticModel Expr 级类型推断，缺口⑤）
- LSP server / 协议（0.5.x）｜ISymbol（F2.2）｜签名帮助 / hover / 跳转
