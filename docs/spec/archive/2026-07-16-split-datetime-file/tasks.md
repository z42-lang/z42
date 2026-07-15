# Tasks: 拆 DateTime.z42（724 行）→ DateTime / DateTimeOffset / _DateTimeHelpers 三文件

> 状态：🟢 已完成 | 创建：2026-07-16 | 完成：2026-07-16 | 类型：refactor（最小化模式）
> 子系统：stdlib（短占，归档即释放）。隔离 git worktree（0.32）实施+验证。

**变更说明：** review §6——`DateTime.z42` 单文件 724 行超 500 硬限。与 Uri 不同，DateTime 类本身
~558 行、静态辅助与实例方法交错，非纯类型拆分。拆三文件：
- `DateTime.z42`（455）：`DateTime` 类（保留实例 format 方法 + 公开 API DaysInMonth/IsLeapYear/Utc/Add*）
- `DateTimeOffset.z42`（165，新）：`DateTimeOffset` 类 + Parse（跨文件引用 DateTime，E0402 已修）
- `_DateTimeHelpers.z42`（106，新）：11 个纯静态辅助（civil↔days 日历换算 + _parseFixed/_expect/_isDigit/_charStr ISO 解析 + _pad2/3/4/_floorDiv/_floorMod）；DateTime/instance 方法的 22 处调用点改 `_DateTimeHelpers.` 限定

**关键前置：** probe 验证 z42.time 包内 E0402（新文件引用兄弟跨文件类型，DateTime.z42 旧注释记为
workaround）**现已修复**——本会话编译器重构消除。旧 E0402 注释随 DateTimeOffset 拆出时删除。

**原因：** review §6——文件 500 行硬限；DateTime.z42 超限。

**文档影响：** 纯文件重组零行为变化。z42.time README 核心文件表加 DateTimeOffset/_DateTimeHelpers 两行。

- [x] 1.1 脚本抽 3 块：DateTime 保留 [1-432]+[482-503]+[555]；helpers=[433-481]+[504-554]；offset=[564-724]
- [x] 1.2 helpers 转 `public static`；DateTime 22 调用点 `_X(`→`_DateTimeHelpers._X(` 限定；删旧 E0402 注释
- [x] 1.3 z42.time README 核心文件表同步
- [x] 1.4 GREEN：worktree 内 `xtask test` 全绿（time 7 文件测试覆盖，行为不变；跨文件解析验证）
- [x] 1.5 归档 + 释放 ACTIVE.md stdlib 锁

## doc-check
- [x] 触发矩阵：新增/删除文件 → README 核心文件表已更新；零行为变化，无 book 变更
