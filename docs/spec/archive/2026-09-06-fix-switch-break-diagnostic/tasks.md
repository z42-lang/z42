# Tasks: fix-switch-break-diagnostic

> 状态：🟢 已完成 | 创建：2026-09-06 | 完成：2026-09-06

**变更说明：** `break` 在 `switch` 臂内合法（此前被误报 `E0410: break outside of loop`）；
顺带根治同一机制的另一半——`break`/`continue` 上下文不再泄漏进 lambda 体。

**原因：** `StmtBinder._bindSwitchStmt` 从不建立 break 上下文，只看 `_loopDepth`，而发射端
`StmtEmitter._emitSwitch` 本就 `PushLoop(...)` 把 switch 登记为 break 目标——**binder 与 emitter
不对称**。后果：任何不在循环内的 `switch` 内 `break` 都编不过（实测 e2e 语料 65 条 E0410 / 8 文件）。
该误报此前被记录为「最小 Infer 路径的产物、非真错」，是错误判断（见下「事实校正」）。

**文档影响：** `docs/book/src/compiler/source-compile.md` 新增机制小节；三处把 bug 记录为
预期行为的陈述做事实校正。

## 1. 根因修复
- [x] 1.1 `TypeChecker.z42`：新增 `_switchDepth` 字段 + 构造归零
- [x] 1.2 `StmtBinder._bindSwitchStmt`：臂体绑定前后 ± `_switchDepth`（只包臂体；subject/guard/模式内不可能有 break）
- [x] 1.3 `StmtBinder._bindStmt` 的 `BreakStmt` 分支：判据改为 `_loopDepth == 0 && _switchDepth == 0`，
      消息改为「`break` outside of loop or switch」；`continue` 判据**不变**（switch 不是循环）
- [x] 1.4 `StmtBinder._bindLambda`：绑定体前把两个计数器归零、绑完恢复
      —— lambda 体发射成独立函数（`InLoop()` 恒 false），此前上下文泄漏使 lambda 内 `break`
      通过类型检查后**静默编译成 no-op**

## 2. 测试
- [x] 2.1 `z42c.semantics/tests/typecheck/break_context_tests.z42`（9 例）：switch 内 break ✓ /
      switch 内 continue 无外层循环 ✗ / switch 在循环内 continue ✓ / 裸 break ✗ /
      switch 之后 break ✗（计数器回退）/ lambda 内 break（外层 switch）✗ / lambda 内 break（外层循环）✗ /
      lambda 内 continue ✗ / lambda 之后 break 仍 ✓（上下文已恢复）
- [x] 2.2 `src/tests/control_flow/switch_break.z42`（e2e）：break 落点 / 不 fall-through /
      switch 在循环内 break 只跳出 switch、continue 转发外层 / 嵌套 switch 内层 break 只跳内层
      —— 该运行期路径此前**从未被覆盖**（既有 `switch_statement.z42` 每个 case 都用 `return` 绕开）

## 3. 文档同步
- [x] 3.1 `docs/book/src/compiler/source-compile.md`：类型检查节新增「`break` / `continue` 的合法上下文
      （binder ↔ emitter 对称）」小节 + 对齐日期刷新
- [x] 3.2 事实校正 ×3：`tests/exhaust/exhaust_tests.z42` / `tests/pattern/pattern_tests.z42` 抬头注释、
      `docs/spec/changes/add-switch-exhaustiveness/{design,tasks}.md`
      —— 三处都把该误报断言为「最小 Infer 路径的产物、非真错」，正是它让这个 bug 长期没被追查

## 4. 验证
- [x] 4.1 `xtask build compiler`（自建全绿）
- [x] 4.2 `xtask test compiler`：570 PASS，含新增 9 例
- [x] 4.3 `xtask test e2e --dir control_flow`：9/9 × interp+jit
- [x] 4.4 完整 `xtask test` GREEN（全 stage ✔；e2e 288 + cross-zpkg 17 + multi-exe 2；
      **z42c self-host 不动点 gen1==gen2 3/3 packages** —— 印证零 codegen 影响）
- [x] 4.5 分支基于 origin/main 顶（无 drift，无需 rebase）→ PR

## 备注
- **自举安全**：纯诊断放宽 + 一处上下文隔离，**零 codegen 改动**；z42c / stdlib / xtask 源码本身
  无 `switch` 语句（`add-switch-exhaustiveness` 已实测 `grep 'switch ('` = 0），故不影响自举字节
  不动点，也无 zbc/zpkg 格式 bump、无新语法（不触发 bootstrap-seed 的两-nightly 纪律）。
- **本修复是 [[restore-emit-zbc-diagnostics-program]] 推进顺序的第 2 步**（口令「推进诊断可见性」）：
  打开 `--emit-zbc` 诊断门之前必须先还清它掀开的欠债，E0410 是其中杠杆最高的一项
  （65 条 / 8 文件，零测试改动即清）。
