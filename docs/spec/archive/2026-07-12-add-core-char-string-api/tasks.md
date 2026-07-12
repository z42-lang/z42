# Tasks: Char.IsDigit/IsLetter + String.ToCharArray（§2.6 补齐）

> 状态：🟢 已完成 | 创建：2026-07-12 | 完成：2026-07-12 | 类型：feat（最小化模式，additive API）
> 子系统：stdlib（短占，归档即释放）。隔离 git worktree 实施+验证。

**变更说明：** review §2.6 指出 z42.core 缺 `Char.IsDigit/IsLetter`（调用方手写
`(int)ch<48||(int)ch>57`，见 BigInt.Parse/Decimal.Parse）与 `string.ToCharArray()`（有
FromChars 无逆，放大 CharAt O(n²) 面）。补齐：
- `Char.IsDigit()`/`IsLetter()`——ASCII 分类纯脚本（locale-sensitive 延后 L3，对齐 ToLower/ToUpper），
  用支持的 char `<`/`>` 运算符表达。
- `String.ToCharArray()`——FromChars 的逆，一次物化 char[] 供 O(1) 下标（如 Levenshtein 预转）。

**原因：** review §2.6——常用 core primitive/string API 缺失致手写重复。

**文档影响：** 新增对外 API。core README 若有 Char/String 功能索引可补（本 change 不改行为契约）；
      无 book 机制变更。

- [x] 1.1 `Char.z42`：IsDigit / IsLetter（ASCII，`!(this<x)&&!(this>y)` 形式）
- [x] 1.2 `String.z42`：ToCharArray（Length + CharAt 循环填 char[]）
- [x] 1.3 回归测试 `op_edge_cases.z42`：IsDigit（数字/字母/空格）+ IsLetter（大小写/数字/符号）
      + ToCharArray（round-trip via FromChars + 空串）
- [x] 1.4 GREEN：worktree 内 `xtask test` 全绿
- [x] 1.5 归档 + 释放 ACTIVE.md stdlib 锁

## 未做（本 change 外）
- `Dictionary.TryGetValue`：需 `out` 参数，z42 当前无（stdlib 零 out 用例）→ 跳过；
  可考虑 `GetOrDefault(key, fallback)` 变体作独立 change。

## doc-check
- [x] 新增对外 API；无行为契约变更；无 book 变更
