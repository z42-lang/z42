# Tasks: add-repl-history-keyword-completion

> 状态：🟢 已完成 | 创建：2026-08-06 | 完成：2026-08-06 | 类型：feat（REPL 交互 UX）

## 进度概览
- [x] 1. 历史跨会话持久（repl.rs）
- [x] 2. 关键字补全（Completer.z42）
- [x] 3. 验证 + 文档同步

## 1. 历史持久（repl.rs）
- [x] 1.1 `history_path()`：`$HOME`（回退 `%USERPROFILE%`）`/.z42_history`；均无 → None（纯进程内）
- [x] 1.2 编辑器 init `load_history`（best-effort）
- [x] 1.3 每行 `add_history_entry` 后 `save_history`（best-effort，跨会话/崩溃存活）

## 2. 关键字补全（Completer.z42）
- [x] 2.1 `using Z42.Syntax`
- [x] 2.2 `_addKeywords`：以 `Lexer.KeywordCount/KeywordNameAt` 为权威源枚举关键字（零漂移），前缀非空才补
- [x] 2.3 接入 ③ 裸标识符分支（`_addImportedNames`/`_addIndexedTypeNames` 之后）

## 3. 验证
- [x] 3.1 `cargo build --release` + `cargo test --lib corelib::repl`——16 例不回归
- [x] 3.2 完整 GREEN：`xtask test`（无 Z42_HOME）——e2e 223/0 + cross-zpkg 8/0 + multi-exe 1/0 +
      stdlib 全绿 + manifest + examples；compiler 自举 fixpoint 冷种子 drift 3/5 → `build compiler` 收敛 → **5/5**
- [x] 3.3 **`xtask build toolchain` EXIT 0**——Completer.z42 编译成功（GREEN 不覆盖 z42.scripting）
- [x] 3.4 文档同步：`docs/design/toolchain/repl.md`（行编辑器段补历史持久 + 补全段补关键字）
- [ ] 3.5 交互手感验收（User，需真实终端）：上箭头跨会话找回历史 / 打 `wh`+Tab 补 `while` —— 待 User

## 备注
- 交互特性需 TTY：本机自动验证覆盖 cargo 单测 + 编译（含 build toolchain 验 Completer）+ GREEN 无回归；
  历史/关键字手感由 User 终端验收。
- 关键字用 Lexer 访问器 → 每次 Tab（及 ghost 每键）构造一个空 Lexer 枚举 ~85 关键字，成本极小可接受。
- 独立 worktree `z42-repl-hist-kw`（基于 origin/main 27ea88ac）。
