# Tasks: YamlParser 递归深度上限（防栈溢出 DoS）

> 状态：🟢 已完成 | 创建：2026-07-16 | 完成：2026-07-16 | 类型：fix（最小化模式）
> 子系统：stdlib（短占，归档即释放）。隔离 git worktree（0.32）实施+验证。

**变更说明：** review §2.2/item 8——`fix-parser-depth-limit` 当时把 YAML 留作 follow-up（其递归比
JSON/TOML 复杂）。YAML 有两条递归漏斗：**block**（`_ParseBlockValue`，缩进驱动）与 **flow**
（`_ParseFlow`，内联 `[]`/`{}`）。两者共享 `_depth` 计数器：入口 +1 查 >256 抛 YamlException
（带位置），try/finally 各返回路径 -1。`_ParseBlockValue` 用 extract-wrapper（体改名 `_ParseBlockValueInner`
避免整体重缩进），`_ParseFlow` 短直接 try/finally 包裹。

**原因：** review §2.2/item 8——三格式 parser 深度上限；YAML 补齐（JSON/TOML 已落）。

**文档影响：** 无对外 API 变化；行为「超深嵌套栈溢出→抛异常」。z42.yaml README 无需改；无 book 变更。

- [x] 1.1 `YamlParser.z42`：`_depth` 字段 + 构造初始化
- [x] 1.2 `_ParseBlockValue`：depth 守卫 wrapper + 体改名 `_ParseBlockValueInner`
- [x] 1.3 `_ParseFlow`：depth 守卫 try/finally 包裹
- [x] 1.4 回归测试 `parse_errors.z42`：300 层 flow 嵌套抛异常 + 100 层合法解析可下钻取值
- [x] 1.5 GREEN：worktree 内 `xtask test` 全绿（yaml 14 文件）
- [x] 1.6 归档 + 释放 ACTIVE.md stdlib 锁；至此三格式 parser 深度上限全落（JSON/TOML/YAML）

## doc-check
- [x] 触发矩阵：无对外 API/依赖/入口变化；行为「深嵌套抛异常」。README 无需改；无 book 变更
