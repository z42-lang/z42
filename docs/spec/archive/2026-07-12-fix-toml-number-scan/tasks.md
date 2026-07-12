# Tasks: TomlParser 数字扫描收紧（拒 1.2.3 / 1e5e5 / 前导零 042）

> 状态：🟢 已完成 | 创建：2026-07-12 | 完成：2026-07-12 | 类型：fix（最小化模式）
> 子系统：stdlib（短占，归档即释放）。**隔离 git worktree 实施+验证**（主树被并发会话未提交
> params 迁移污染；worktree clean HEAD + clean 0.31 种子，"并行推进"无 clobber）。

**变更说明：** `TomlParser.ParseNumber` 扫描过宽：放行 `1.2.3`/`1e5e5`（随后 `double.Parse`
裸抛非 TomlException）、前导零 `042`（`long.Parse` 静默接受为 42）。修：
① 最终 `double.Parse`/`long.Parse` 入 try/catch → `this.Err`（1.2.3/1e5e5 变带行列 TomlException）；
② 新增 `RejectLeadingZeros`——十进制整数部分冗余前导零抛 TomlException（`0`/`0.5`/`0e0`/`-0` 仍合法）。

**原因：** review §2.2——数字扫描过度宽松，且失败抛的不是带行列的 TomlException。

**文档影响：** 无对外 API 变化；行为「非法数字裸崩/静默接受→带行列 TomlException 拒绝」。
      z42.toml README 无需改；无 book 机制变更。

- [x] 1.1 `TomlParser.z42` ParseNumber：try/catch 包最终 parse → Err；加 `RejectLeadingZeros` 辅助
- [x] 1.2 回归测试 `parse_errors.z42`：拒 1.2.3 / 1e5e5 / 042 / 01.5；接受 0 / 0.5 / 0e0 / 42
- [x] 1.3 GREEN：worktree 内 `xtask test` 全绿
- [x] 1.4 归档 + 释放 ACTIVE.md stdlib 锁

## doc-check
- [x] 触发矩阵：无对外 API/依赖/入口变化；行为「非法数字带行列拒绝」。README 无需改；无 book 变更
- [x] 本次触及文档相对链接可解析
