# Tasks: fix — finally 在非局部退出（return/break/continue）时被跳过

> 状态：🟢 GREEN，待合并 | 创建：2026-08-01 | 类型：fix（编译器 codegen；恢复既有 try/finally 语义）
> 占用子系统：`compiler`（z42c.semantics）—— 隔离分支 off origin/main，合并后归档释放锁

**变更说明：** z42c 的 `_emitTry` 只把 finally 接到三条退出边（try 体 fall-through / catch fall-through /
合成 `"*"` catch-all 异常回卷）。当 try 体（或 catch 体）里出现 `return` / `break` / `continue` 时，
它们直接发射终结指令离开，**跳过 finally**——在 interp 与 jit 都复现（同一份坏 IR）。
**原因：** codegen 缺「非局部退出经 finally」的 lowering（非 VM 执行语义问题）。现场后果：
`Std.Json`/`Std.Toml` 的递归深度守卫 `try{return…}finally{_depth--}` 的 `_depth` 只增不减 → 大而浅的
文档误报 `nesting too deep (max 256)`（CI bench-regression 解析 baseline.json 崩溃的真因）。

**文档影响：** `docs/book/` 编译器 codegen 机制页补「finally 非局部退出 lowering」小节（阶段 5）；
`src/tests/exceptions/` README 若有则登记新用例。

## 设计（finally handler 栈）
- FunctionEmitter 加活跃 finally 栈 `_finBodies`/`_finDepth`（词法包裹当前发射点的 try-with-finally 体，
  内层在顶）。`_emitTry` 在发射 try 体+catch 体**前** push、发射 finally 三副本**前** pop。
- `return`：先求值返回值（finally 前，镜像 C#/Java），再 `_emitPendingFinallys(0)` 内→外跑**全部**
  外层 finally，最后 `RetTerm`。
- `break`/`continue`：`_emitPendingFinallys(floor)`，floor = 目标层入栈时记录的 finally 深；只跑
  跨越本层边界的 finally。EmitContext 循环栈每层记 `BreakFinFloor`/`ContFinFloor` 两个底
  （switch 的 continue 转发外层循环 → contFloor 继承外层，break 用自身）。
- 发射 finally[i] 时把 `_finDepth` 临时截断到 i → finally 内的 return 只跑更外层、不自我重入；
  任一 finally 自身非局部退出（块 Ended）即止。
- 嵌套 try-finally、return-in-finally、switch-continue-跨-finally 均覆盖。

## 自举安全
- z42c 源码**零** `try/finally` 语句（全部 finally 出现是 keyword/AST/label 字符串）→ 老 codegen 与
  新 codegen 发射的 z42c 字节相同 → **gen1==gen2 不动点不破**。仅 stdlib golden（JSON/TOML 守卫）改变
  ——即本修复的目的，regen。

## 阶段
- [x] 1. EmitContext：循环栈加 `BreakFinFloor`/`ContFinFloor` + `PushLoop` 四参 + `CurBreak/ContFinFloor`
- [x] 2. FunctionEmitter：`_finBodies`/`_finDepth` + `_pushFinally`/`_popFinally`/`_emitPendingFinallys`
- [x] 3. `_emitTry` push/pop；`return`/`break`/`continue` 经 finally；5 处 PushLoop 传 floor
- [x] 4. golden：`src/tests/exceptions/finally_nonlocal_exit`（8 例：return-in-try / 值保留 / break·continue
  经 finally / 嵌套 / return-in-finally 覆盖 / 循环在 try 内 break 不触发 finally）——interp+jit 均对账通过
- [x] 5. 构建 + GREEN：**完整 `xtask test` 全绿**——z42c 自举不动点 5/5 gen1==gen2 byte-identical、
  e2e（含新用例 `finally_nonlocal_exit` OK）、stdlib 280 file/23 lib（含 z42.json 8 + z42.toml 8 depth-guard
  自愈）、z42c [Test] 20、cross-zpkg、vscode-syntax；interp+jit 均对账
- [x] 6. 文档同步：`docs/book/src/compiler/source-compile.md` IrGen 节加「try/catch/finally 控制流下沉」小节
- [ ] 7. 归档 + 释放 compiler 锁（PR 合并后）

## 备注
- 发现自 PR #85（wasm 打包）追查 bench-regression 崩溃；见 memory `vm-finally-on-return-skipped`。
- 现有 `src/tests/exceptions/try_finally` 只覆盖 normal/catch/no-exception 退出，缺 return-in-try。
