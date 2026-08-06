# Tasks: fix-repl-completion-span-and-index

> 状态：🟢 已完成 | 创建：2026-08-06 | 完成：2026-08-06 | 类型：fix（最小化模式）

**变更说明：** 修复 REPL Tab 补全两个 bug：① 成员补全（`Console.WriteLine`）会清空 receiver；
② 未执行过的符号补不出（`Console` 只有引用/执行后才能补）。

**原因：**
- **Bug ①（替换区间）**：Rust 侧 `word_start`（`corelib/repl.rs`）把 `.` 也算进标识符 → `Console.Wr`
  的替换区间从 `C` 起；而 z42 补全器（`Completer.z42`）返回**成员名** `WriteLine`（不含 receiver）→
  rustyline 用 `WriteLine` 替换整段 `Console.Wr` → `Console.` 被清空。两个 `word_start` 不一致（z42 侧不含 `.`）。
- **Bug ②（候选源）**：`Completer._addImportedNames` 只遍历**已 reconcile** 的 `Exported[]`；`Console`
  等没被引用过的类型不在里面 → 补不出。而 ns 索引（`DepScanResult.TypeShort`/`TypeNs`，#101）prewarm 就
  记全了每 ns 的类型短名。

**根因修复：**
- ① `repl.rs word_start` 去掉 `.`（对齐 z42 `_wordStart`）→ 成员候选只替换 `.` 后前缀；`identifier_hint`
  的成员跳过守卫改为「word 前一字符是否 `.`」（word_start 不再含 `.` 后 `word.contains('.')` 失效）。
- ② `Completer._addIndexedTypeNames`：裸标识符补全也从 `TypeShort`/`TypeNs` 索引按活跃 using 补出类型名
  （Bug 2a）；`_typeStaticMembers` 前置 `_ensureReconciled`——`Type.` 成员补全若类型未 reconcile 则按需
  reconcile 一次（复用 E0401 恢复的 `ReconcileCandidatesInNs`，Tab 触发、一次性 ~150ms）（Bug 2b）。

**文档影响：** repl.md 的 tab-completion 说明可后续补（本 fix 先修行为；`repl-future-tab-completion` 的
「未落地」窄化已在 docs-correct-repl-deferred-status PR 处理）。

## 改动文件
- [x] 1.1 `src/runtime/src/corelib/repl.rs` — `word_start` 去 `.`；`identifier_hint` 成员守卫改 char-before-`.`
- [x] 1.2 `src/toolchain/scripting/src/Completer.z42` — `_addIndexedTypeNames` + `_nsActive` + `_ensureReconciled`；接入 ③ 裸标识符 + `_typeStaticMembers`
- [x] 1.3 `src/runtime/src/corelib/repl_tests.rs` — `word_start` 单测（成员 stops-after-dot / 裸词整段）

## 验证
- [x] 2.1 `cargo build --release` + `cargo test --lib corelib::repl`——word_start 单测绿（16 例）
- [x] 2.2 完整 GREEN：`xtask test`（隔离 worktree，无 Z42_HOME）全绿——build stdlib（Completer.z42 编译）✓ +
      e2e 220/0 + cross-zpkg 8/0 + multi-exe 1/0 + stdlib 全绿 + compiler 自举 5/5 不动点 + vscode-syntax
- [ ] 2.3 交互手感验收（User，需真实终端）：裸 `Console` 未执行即补 / `Console.WriteLine` 不清空 receiver / 未执行的 `Type.` 成员可补

## 备注
- 均为**既有** bug（非 #122 引入；#122 只加 ghost/缩进，未动 Completer 替换逻辑）。
- 独立 worktree `z42-replcomplete-fix`（基于 origin/main 7402d069，含 #122）。
- 交互特性需 TTY：本机自动验证覆盖 word_start 单测 + 编译 + GREEN 无回归；补全候选正确性由 User 终端验收。
