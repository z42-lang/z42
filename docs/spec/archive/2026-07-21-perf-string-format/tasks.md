# Tasks: perf-string-format

> 状态：🟢 已完成 | 创建：2026-07-21 | 完成：2026-07-21 | 类型：perf（最小模式，行为保持）
> 子系统：`stdlib`（短占 converge 锁，隔离 worktree）

**变更说明：** perf change #3——`String.Format` 从链式 Replace 改单趟扫描替换。

**原因：** perf 分析 + 新 bench 实测 `String.Format` 16 次调用 2.07ms（链式 k 次全串
Replace + 每 pass Convert.ToString + needle 构建 O(k·len)）。

**文档影响：** 无外部 API 变化。行为保持（无 arg 含 `{n}` 时输出一致；更贴 C# 非递归语义）。

- [x] `String.Format`：args 一次性 stringify + 两趟扫描（算长度 → 填 char[]），单趟识别
      `{n}` token（`_fmtToken`/`_fmtIndex` 助手）。非递归替换（旧链式 Replace 会在 arg 文本内
      再替换——z42.core 无 Format 测试覆盖该边界，且 C# 亦非递归）。
- [x] GREEN + 量化：`test stdlib z42.core` **8 文件全绿**；**string_format 2.07ms → 0.896ms = 2.31×**。

## 备注（course correction，perf 经验）
- **同时尝试的 Join/Concat char[] 改写被回退**：实测 string_join **反而慢 4.5×**（0.015→0.067ms）。
  根因——小 n 下 `+`（StrConcatInstr，快 IR op）远优于 char[]+CharAt（每字符 builtin 分发 +
  FromChars 重编码）；O(n²) 只在大 n 才咬人。**教训：char[]+CharAt 只在原实现比它更糟时才赢
  （Format 的链式 Replace 是；Join 的 `+` 不是）**。大 n Join 的真正解是 native `__str_join` /
  StringBuilder，留后。Join/Concat 保持原 `+`，注释记明。
- Regex.Replace（R7）、StringBuilder 重载（P2）、parser run-based append（P1）留 change #4。
