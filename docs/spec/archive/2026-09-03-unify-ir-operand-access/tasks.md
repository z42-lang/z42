# Tasks: IR 指令统一操作数接口（unify-ir-operand-access）

> 状态：🟢 已完成 | 创建：2026-09-03 | 完成：2026-09-03 | 类型：refactor（compiler + stdlib(z42.ir)）
**变更说明：** 给 `IrInstr` / `IrTerminator` 加统一操作数接口（`DefReg`/`SetDefReg`/`ReadCount`/`ReadAt`/`SetReadAt`/
`StrCount`/`StrAt`/`Clone`；终结符 `ReadReg`/`SetReadReg`），并引入 `IrDefOnlyInstr`/`IrUnInstr`/`IrBinInstr` 三个共享
形状基类；`IrOptInfo`（DstId/AddReads/ReplaceReads/SetDst/AddTermReads/ReplaceTermReads）、`IrInline._cloneRemap`、
`ZbcWriter._regtInstr/_regtTerm`、`ZbcInstr.InternStrings`、`IrEscapeAnalysis._markEscaping` 五处逐指令 if-is 链
改为走接口迭代；逃逸分析规则表改「显式 neutral + 保守兜底」。`IrInstr.z42`（619 行）按类别拆为 5 个文件。
**原因：** 三面评审 C-1——同一份「指令有哪些操作数」知识在 ≥5 处手工平行维护，新增指令改点约 12 处，且逃逸
分析头注释明言「漏一个 escaping 操作数 = 悬垂栈引用」（漏枚举默认不安全）。接口化后新增指令只需实现接口 +
编解码，其余消费者自动覆盖；逃逸分析漏登记退化为「少一次栈分配」而非悬垂引用。
**文档影响：** `src/libraries/z42.ir/README.md`（核心文件表）、`docs/book/src/runtime/optimization-pipeline.md` /
`escape-analysis-stack-alloc.md`（枚举来源改接口）。

## 进度概览
- [x] 1. z42.ir：拆 IrInstr.z42 + 接口 + 共享基类；IrTerminator 接口
- [x] 2. 消费者改接口：IrOptInfo / IrInline / ZbcWriter / ZbcInstr / IrEscapeAnalysis
- [x] 3. 验证：产物字节对比（改前 32 个 zpkg 快照 vs 改后）+ `xtask test` 完整 GREEN（含自举不动点）
- [x] 4. 文档同步 + 归档

## 备注
- 字节影响预判：REGT 收集以前漏走 `BuiltinInstr`/`CallNativeInstr`（extern 桩的参数寄存器在 REGT 里为 Unknown），
  统一后补齐 → extern 桩函数的 REGT 字节可能变化（deterministic，不动点不受影响）；`IrOptInfo` 以前漏
  `MethodTypeArgInsn`/`MethodDefaultInsn` 的 Dst → defs 计数补齐。实测差异记录在验证步骤。

## 验证记录（2026-09-03）
- `xtask test` ✅ GREEN 14:00（e2e 19/3/4 组全过；z42c [Test] 23/23；self-host 不动点 3/3 gen1==gen2；vscode-syntax）。
- 产物字节对比（改前主树 regen 快照 vs 改后 worktree，32 个 zpkg）：25 个逐字节相同；`z42.ir` / `z42c.semantics`
  为源码变更；`z42.core`（39 B）/ `z42.compression`（1 B）/ `z42.test`（2 B）仅 REGT 中 void 型 extern 桩的结果寄存器
  由 Unknown(0) 变为 Void(15)（以前 `_regtInstr` 漏走 Builtin/CallNative，现经接口补齐）+ 尾部内容哈希；
  `z42c.core` / `z42c.syntax` 仅调试信息里嵌的源码绝对路径长度差（worktree 路径），无其他差异。
- 逃逸分析：书页「规则表未列指令 → 保守兜底」此前与代码（未列 = neutral）不一致，本次代码对齐书页。
