# Tasks: JsonParser 大整数溢出回退 f64（而非裸抛异常）

> 状态：🟢 已完成 | 创建：2026-07-12 | 完成：2026-07-12 | 类型：fix（最小化模式）
> 子系统：stdlib（短占——converge DRAFT 预留、阻塞在 compiler 锁；User 授权续做 review fix，归档即释放）

**变更说明：** `JsonParser` 整数解析 `long.Parse(raw)` 无 try/catch——超 i64 的**合法** JSON
整数（如 1e20 整数形）溢出直接裸抛异常，而注释自称「on overflow fall back to f64」。补
try/catch → `OfDouble`（lossy，对齐 serde_json）。

**原因：** review §2.2——注释与代码不符；超 i64 合法 JSON 应回退 f64，不应崩。

**文档影响：** 无对外 API 变化；行为「超 i64 崩→回退 double」。z42.json README 无需改；无 book 变更。

- [x] 1.1 `JsonParser.z42`：`long.Parse` 入 try、overflow catch → `OfDouble(double.Parse(raw))`
- [x] 1.2 回归测试 `parse_numbers.z42`：10^20 回退 double（不抛）+ i64 max 仍为 long（无过早回退）
- [x] 1.3 GREEN：`xtask test` **全绿**（e2e 197/0 + stdlib 全 274 文件/22 库 0 failed（含本 2 例）
      + 自举不动点 7/7 + vscode-syntax；C#-free，4-streak 安静窗口无 clobber）
- [x] 1.4 归档 + 释放 ACTIVE.md stdlib 锁

## doc-check
- [x] 触发矩阵：无对外 API/依赖/入口变化；行为「超 i64 崩→回退 double」。z42.json README 无需改；
      无 book 机制变更
- [x] 本次触及文档相对链接可解析
