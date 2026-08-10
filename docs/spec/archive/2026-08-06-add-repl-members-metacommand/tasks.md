# Tasks: add-repl-members-metacommand

> 状态：🟢 已完成 | 创建：2026-08-06 | 完成：2026-08-06 | 类型：feat（REPL 元指令；interactive_main + Script.z42）

## 进度概览
- [x] 1. `.members` 实现
- [x] 2. 验证 + 文档同步

## 1. 实现
- [x] 1.1 `Script.EnsureWarm`（public 包装 `_ensureWarm`，供元指令无 eval 时预热）
- [x] 1.2 派发链加 `.members ` 分支（在 `.type ` 之后）
- [x] 1.3 `_showMembers`：`EnsureWarm` + `SetActive` + `replComplete("<arg>.", …)` → 逐行打印；空参提示用法
- [x] 1.4 `_help` 补 `.members <T>` 一行

## 2. 验证
- [x] 2.1 **`xtask build toolchain` EXIT 0**——interactive_main.z42 + Script.z42 编译成功
- [x] 2.2 管道实测：`.members Console`→Write/WriteLine/ReadLine/IsTerminal（首命令 EnsureWarm 生效）；`.members s`（string 变量）→ToString/get_Length/CharAt/…（当前 vm 含 #133 基元成员）；`.members NoSuchThing`→优雅提示 ✓
- [x] 2.3 文档同步：`docs/design/toolchain/repl.md`（指令表 / 落地状态标 `.members` 已接）
- [x] 2.4 GREEN 判定：本改动**纯 scripting/interactive**（Script.z42 + interactive_main.z42），不碰 VM/stdlib/compiler/e2e——`xtask test` provably 不覆盖 z42.scripting/interactive（memory green-gate-skips-scripting-interactive），**build toolchain 编译 + 管道实测即相关门禁**；未跑 full GREEN（对本改动零信号）

## 备注
- 基于 #134 分支 `add-repl-type-metacommand`（含 `.type` + 成员 ghost）——`.members` 与 `.type` 同改 interactive_main，
  避免冲突；#134 合并后本分支 rebase 到 main。
- `.members` 是元指令、**可管道实测**（不像 Tab/ghost 需 TTY）。
- 独立 worktree `z42-repl-members`。
