# Tasks: ParseIso8601 按月长校验日（拒 2026-02-31）

> 状态：🟢 已完成 | 创建：2026-07-12 | 完成：2026-07-12 | 类型：fix（最小化模式）
> 子系统：stdlib（短占，归档即释放）。**在隔离 git worktree 中实施**——主工作树被并发会话
> 未提交的 params 迁移（Path.Join/String.Join）污染，无法构建；worktree 以 clean HEAD +
> clean 0.31 种子隔离，规避交叉污染（"并行推进"）。

**变更说明：** `DateTime.ParseIso8601` 只校验日 `1..31`，`"2026-02-31"` 静默滚成 3/3；
同文件 `DateTime.Utc` 已按 `DaysInMonth(year,month)` 严格校验。改 ParseIso8601 镜像之。

**原因：** review §2.2——同文件两处日校验不一致，宽松侧接受非法日期。

**文档影响：** 无对外 API 变化；行为「非法日静默滚位→抛 ArgumentException」。z42.time README
      无需改；无 book 机制变更。

- [x] 1.1 `DateTime.z42` ParseIso8601：日上界改用 `DateTime.DaysInMonth(year, month)`（镜像 Utc）
- [x] 1.2 回归测试 `datetime_parse_iso8601.z42`：拒 2026-02-31 / 2026-04-31 / 2023-02-29；
      接受闰年 2024-02-29 + 各月末合法日
- [x] 1.3 GREEN：worktree 内 `xtask test` **全绿**（e2e 197/0 + stdlib 全 274 文件/22 库 0 failed
      （含本 5 例）+ 自举不动点 7/7 + vscode-syntax；C#-free；隔离 worktree 无 clobber）
- [x] 1.4 归档 + 释放 ACTIVE.md stdlib 锁

## doc-check
- [x] 触发矩阵：无对外 API/依赖/入口变化；行为「非法日抛异常」。z42.time README 无需改；无 book 变更
- [x] 本次触及文档相对链接可解析
