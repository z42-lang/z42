# Tasks: JSON/TOML parser 递归深度上限（防栈溢出 DoS）

> 状态：🟢 已完成 | 创建：2026-07-12 | 完成：2026-07-12 | 类型：fix（最小化模式）
> 子系统：stdlib（短占，归档即释放）。**在隔离 git worktree 实施+验证**（主树被并发会话
> 未提交 params 迁移污染；worktree clean HEAD + clean 0.31 种子，"并行推进"无 clobber）。

**变更说明：** `JsonParser` / `TomlParser` 的 `ParseValue` 递归无深度上限——恶意深嵌套输入
（数千个 `[`）会打爆 VM 栈。两 parser 各加 `_depth` 计数器：ParseValue 入口 +1 并查 > 256
即抛格式异常（JsonException/TomlException，带行列），try/finally 保证各返回路径 -1。

**原因：** review §2.2 / item 8——三格式 parser 无深度上限。本 change 覆盖 JSON + TOML
（ParseValue 单点递归漏斗，改动干净）；**YAML 递归结构复杂（缩进 + flow），留独立 follow-up**。

**文档影响：** 无对外 API 变化；行为「超深嵌套栈溢出→抛异常」。z42.json/z42.toml README 无需改；
无 book 机制变更（深度守卫为局部防护，非新机制）。

- [x] 1.1 `JsonParser.z42`：`_depth` 字段 + 构造初始化 + ParseValue try/finally 深度守卫（>256 抛）
- [x] 1.2 `TomlParser.z42`：同上
- [x] 1.3 回归测试 `z42.json/tests/parse_errors.z42` + `z42.toml/tests/parse_errors.z42`：
      300 层嵌套抛异常（不栈溢出）+ 100/50 层合法嵌套正常解析并可下钻取值
- [x] 1.4 GREEN：worktree 内 `xtask test` 全绿
- [x] 1.5 归档 + 释放 ACTIVE.md stdlib 锁

## 后续（不在本 change）
- YAML parser 深度上限（`_ParseMappingKey`/flow 递归；结构复杂，独立 change 评估）

## doc-check
- [x] 触发矩阵：无对外 API/依赖/入口变化；行为「深嵌套抛异常」。README 无需改；无 book 变更
- [x] 本次触及文档相对链接可解析
