# Tasks: add-repl-multiline-completion

> 状态：🟢 已完成 | 创建：2026-08-06 | 完成：2026-08-06 | 类型：feat（REPL 交互 UX；仅 corelib/repl.rs）

## 进度概览
- [x] 1. 续行自动缩进
- [x] 2. inline ghost 补全提示
- [x] 3. 单元测试
- [x] 4. GREEN + 文档同步

## 1. 续行自动缩进（repl.rs）
- [x] 1.1 `read_one_line` 加 `initial: &str` 参数；非空 → `readline_with_initial(prompt,(initial,""))`
- [x] 1.2 `plain_readline` 加 `_initial`（no-tty/管道路径忽略，输入自带文本）
- [x] 1.3 `__repl_readblock` 续行按 `continuation_indent(&buf)` 预填；首行 + `__repl_readline` 传 `""`
- [x] 1.4 `continuation_indent` 纯函数：`bracket_depth.max(0) × 4 空格`

## 2. inline ghost 补全提示（repl.rs）
- [x] 2.1 `ReplHelper` 改带 `HistoryHinter` 字段 + `new()`
- [x] 2.2 `identifier_hint`：裸标识符（无 `.`）复用 completer，`starts_with` 前缀过滤取首个扩展候选后缀
- [x] 2.3 `Hinter::hint`：end-of-line 才提示；标识符 ghost 优先，回退历史 ghost
- [x] 2.4 `highlight_hint`：ANSI 灰字（`\x1b[90m…\x1b[0m`）渲染 ghost

## 3. 测试
- [x] 3.1 `continuation_indent` 单测 5 例（一级/嵌套/部分闭合/平衡·过闭合/串·注释）——全绿
- [x] 3.2 编译无错无新警；管道多行块读取无回归（`class` 多行定义 → 定义成功）

## 4. 验证
- [x] 4.1 `cargo build --release` + `cargo test --lib corelib::repl`——全绿（14 例：9 bracket + 5 indent）
- [x] 4.2 完整 GREEN：`xtask test` 全 stage（隔离 worktree，rebase 到最新 main 5490286c）——
      e2e 220/0 + cross-zpkg 8/0 + multi-exe 1/0 + stdlib 全绿 + compiler 自举 5/5 不动点 + vscode-syntax 同步
- [x] 4.3 文档同步：`docs/design/toolchain/repl.md`（多行段补自动缩进 + 行编辑器段补 inline 提示、修正 stale「Tab deferred」）
- [ ] 4.4 交互手感验收（User，需真实终端）：续行缩进、标识符 ghost、历史 ghost、Tab 补全 —— 待 User 验收

## 备注
- 交互特性（缩进预填/ghost/Tab）本质需 TTY，非交互环境（管道/CI）走 `plain_readline`、无 ghost；
  故本机自动验证覆盖到「纯逻辑单测 + 编译 + 管道无回归」，交互手感由 User 终端验收。
- 独立 worktree `z42-repl-ux`（基于 origin/main），与 loop-alloc / fix-repl-inmemory-dep-warn 正交。
- follow-up：dedent-on-`}`、`obj.` 成员 ghost、缩进宽度可配（见 proposal Out of Scope）。
