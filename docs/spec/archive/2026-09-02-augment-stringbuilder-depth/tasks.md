# Tasks: augment-stringbuilder-depth

> 状态：🟢 已完成 | 创建：2026-09-02 | 完成：2026-09-02

**变更说明：** 给 `Std.Text.StringBuilder` 补齐 C# `System.Text.StringBuilder` 常用成员——
`Clear` / 索引器 `this[int]` / `Length { get; set; }` 属性 / `Insert` / `Remove` /
`Replace` / `AppendFormat`。
**原因：** corelib 对齐（口令「推进 corelib 对齐」backlog #1）。当前 StringBuilder 极单薄，
只有 Append 家族 + GetLength + ToString，缺随机访问与就地编辑能力。
**文档影响：** `src/libraries/z42.text/README.md`（功能索引补新成员）；纯 additive stdlib API，
无 lang/ir/vm 变更，无 book 机制页需改（内部实现思路仍是 `string[]` parts + 需要随机编辑时
collapse 成单段）。

## 变更分类
`feat`（纯 additive stdlib 库 API），非 lang/ir/vm → 走轻量流程（tasks.md + 实现 + GREEN + PR）。

## 任务
- [x] 1.1 `StringBuilder.z42`：私有 `_setSingle(string)` helper（把 parts buffer 收敛成单段）
- [x] 1.2 `Clear()` → StringBuilder
- [x] 1.3 索引器 `char this[int index] { get; set; }`
- [x] 1.4 `Length { get; }` 计算属性（get 返回 _length，C# 读取对齐；保留 `GetLength()`）
      ⚠️ **Length setter 无法实现**：z42 命名属性只支持计算 getter body，完整 get+set
      accessor body 仅索引器（GS5）支持，命名属性不支持（seed z42c 报 E0202「expected get
      or set accessor」）。setter 语义由 `Clear()`（=0）+ `Remove(n, Length-n)`（截断）覆盖。
      根治需编译器加「命名属性 set-body」解析支持 → 记入 Deferred。
- [x] 1.5 `Insert(int, string)` / `Insert(int, char)` / `Insert(int, object)` → StringBuilder
- [x] 1.6 `Remove(int startIndex, int length)` → StringBuilder
- [x] 1.7 `Replace(string oldValue, string newValue)` → StringBuilder
- [x] 1.8 `AppendFormat(string format, params object[] args)` → StringBuilder
- [x] 1.9 测试：`tests/stringbuilder.z42` 补每个新成员的正常 + 边界用例
- [x] 1.10 `README.md` 功能索引同步
- [x] 1.11 GREEN（`xtask test` 全 stage）+ 自举边界（`xtask test bootstrap` 非必需，无编译器改动）

## 备注
- 内部表示是 `string[]` parts（按 2× 扩容，ToString 一次性合并）。随机访问/就地编辑成员
  先 `ToString()` collapse 再 `_setSingle` 重置为单段——O(n) 但语义正确，符合 StringBuilder
  「批量 Append 快、偶发编辑」的使用画像。
- 越界语义：Substring 已对越界 `throw`，Insert/Remove 复用其检查，天然报错，无需重复校验。
