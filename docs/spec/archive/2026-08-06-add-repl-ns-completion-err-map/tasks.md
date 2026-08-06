# Tasks: add-repl-ns-completion-err-map

> 状态：🟢 已完成 | 创建：2026-08-06 | 完成：2026-08-06 | 类型：feat（REPL 补全 + 诊断；均 z42.scripting）

## 进度概览
- [x] 1. 命名空间名补全（Completer.z42）
- [x] 2. 错误行号回映（Script.z42）
- [x] 3. 验证 + 文档同步

## 1. 命名空间补全
- [x] 1.1 `replComplete` 加 `.using ` 上下文分支
- [x] 1.2 `_namespaceComplete`：`NsNames` 前缀匹配 → 返回「下一段」候选（复用 `_wordStart` 最后一段替换区间）

## 2. 错误行号回映
- [x] 2.1 `_countNewlines`（(char)10）+ `_remapDiag`（`<file>(L,C): rest` → 用户行 = L − prelude行；丢文件/列）
- [x] 2.2 Eval 错误路径回映；`_evalDecl` 错误路径回映

## 3. 验证
- [x] 3.1 **`xtask build toolchain` EXIT 0**——Completer.z42 + Script.z42 编译成功
- [x] 3.2 管道实测：`replComplete(".using Std.C",12)`→`[Collections,Cli,Compression,Crypto]`；单行 `undefinedThing`→`E0401: undefined: undefinedThing`（干净）；多行 class 方法错误→`第 3 行: E0401: undefined: undefinedInMethod` ✓
- [x] 3.3 文档同步：`docs/design/toolchain/repl.md`（Tab 补全段补命名空间；错误恢复段补行号回映）
- [x] 3.4 GREEN 判定：纯 scripting（Completer.z42 + Script.z42），不碰 VM/stdlib/compiler → build toolchain + 管道实测即相关门禁；未跑 full GREEN（零信号）

## 备注
- 列号不回映（Rewriter 移位、wrapper 前缀各轮不同 → 不可靠）——只映行，见 proposal Out of Scope。
- 独立 worktree `z42-repl-nsdiag`（基于 origin/main）。
