# Tasks: 拆 Uri.z42（770 行 3 类型）→ Uri / UriParser / UriCodec 三文件

> 状态：🟢 已完成 | 创建：2026-07-16 | 完成：2026-07-16 | 类型：refactor（最小化模式）
> 子系统：stdlib（短占，归档即释放）。隔离 git worktree（0.32）实施+验证。

**变更说明：** review §6 / §1.8——`Uri.z42` 单文件 770 行（超 500 硬限）装 3 个 public 类型
（Uri + UriParser + UriCodec）。按类型拆三文件（同 `Std.Uri` namespace，各 <500 行）：
- `Uri.z42`（429）：`Uri` 值对象
- `UriParser.z42`（177，新）：`UriParser` 解析器
- `UriCodec.z42`（172，新）：`UriCodec` percent-codec

**关键发现：** z42 **跨文件同 namespace 类引用现已可用**（Uri→`new UriParser`/`UriCodec.Encode`
跨文件解析，build stdlib 22/22 通过）——旧 E0402 限制（曾迫使多类型挤一文件）已被本会话
编译器重构消除。**这解锁了 review §6 全部文件拆分**。

**原因：** review §6——文件 500 行硬限；Uri.z42 超限。

**文档影响：** 纯文件重组，零行为变化。z42.uri README 核心文件表加 UriParser/UriCodec 两行。

- [x] 1.1 拆 `Uri.z42` → Uri（1-429）/ UriParser.z42（431-602）/ UriCodec.z42（604-770），各加 namespace 头
- [x] 1.2 z42.uri README 核心文件表同步
- [x] 1.3 GREEN：worktree 内 `xtask test` 全绿（uri 6 文件测试覆盖，行为不变；跨文件解析验证）
- [x] 1.4 归档 + 释放 ACTIVE.md stdlib 锁

## 后续（review §6 其余超限文件，各独立 refactor）
- 多类型文件（易，同款拆）：DateTime.z42（719：DateTime+DateTimeOffset）
- 单巨型类型（需抽 helper static class 或 partial）：BigInt 2198 / YamlParser 1647 / HttpClient 1406 /
  Aes 1102 / TomlParser 931 / ArgParser 729 / Rsa 566 / WebSocketClient 551 / Regex 532 / Tar 514

## doc-check
- [x] 触发矩阵：新增/删除文件 → README 核心文件表已更新；零行为变化，无 book 变更
