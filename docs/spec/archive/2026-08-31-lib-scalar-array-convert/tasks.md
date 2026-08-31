# Tasks: stdlib 纯脚本小补齐 —— TryParse / Double 分类 / Array 算法 / Convert（library-review PR-D）

> 状态：🟢 已完成 | 创建：2026-08-31 | 类型：feat(stdlib)（最小化流程）

## 背景

`docs/library_review.md` 第一波「一批纯脚本小补齐」。原 PR-D 计划含 String 补齐，但 String 部分
**拆出另走**（见下「String 部分为何拆出」）。本 PR 落四类**与 seed 兼容、无编译器改动**的补齐。

## 本 PR 内容

- **TryParse → T?**（非抛出解析，null 表失败，镜像 `IPAddress.TryParse` 的 nullable 返回）
  - `Int32.TryParse(string) → int?`、`Int64.TryParse → long?`、`Double.TryParse → double?`
  - try/catch 包 `Parse`；底层 `bail!` 以可捕获的 `Std.Exception` 冒泡（make-corelib-errors-catchable）。
- **Double IEEE-754 分类**（纯脚本，无 VM intrinsic）：`IsNaN`（`v != v`）/ `IsInfinity`（`!IsNaN && v-v≠0`）
  / `IsPositiveInfinity` / `IsNegativeInfinity` / `IsFinite`。
- **Array 静态算法**（泛型静态，C# `System.Array` 对标；兑现 Array.z42 顶部预告的「静态算法」）：
  `IndexOf<T>` / `Copy<T>` / `Fill<T>` / `Reverse<T>` / `Sort<T>`（默认 CompareTo）/ `Sort<T>(Func<T,T,int>)`。
  稳定归并排序 + 比较器重载，镜像 `List<T>.Sort`。
- **Convert.To***：`ToByte` / `ToInt16`（委托 Byte/Int16.Parse）/ `ToSingle`（`(float)Double.Parse`）/
  `ToBoolean`（"true"/"false" 大小写不敏感 + trim）/ `ToChar`（长度 1 串）。

## 进度

- [x] Int32/Int64/Double `TryParse`
- [x] Double `IsNaN`/`IsInfinity`/`IsPositiveInfinity`/`IsNegativeInfinity`/`IsFinite`
- [x] Array `Sort`/`Sort(cmp)`/`IndexOf`/`Copy`/`Fill`/`Reverse`
- [x] Convert `ToByte`/`ToInt16`/`ToSingle`/`ToBoolean`/`ToChar`
- [x] 测试：`scalar_tryparse_classify.z42` / `array_algorithms.z42` / `convert_extras.z42`
- [x] 文档：z42.core/README（功能索引：Int/Long/Double/Array/Convert 行）+ overview.md（Convert 文件树注）
- [x] 目标测试 `xtask test stdlib z42.core` 23/23 通过
- [x] 完整 `xtask test` GREEN → ✅ all stages passed；self-host 3/3 gen1==gen2
- [x] 归档 + PR

## String 部分为何拆出（重要）

原计划把 String 补齐（PadLeft/IndexOf(char)/Split(char[])/Trim(char)/LastIndexOf/Insert/Remove）加到
prelude `Std.String`。实测撞上**两道独立 z42c 限制**，均需编译器修复 + 两-nightly（z42.core 由 seed z42c
编译），故拆出后续 change：

1. **E0433**：`String` 是特例 `static class`，含同 arity 协议豁免重载（`Equals(object?)`/`Equals(string)`，
   同 native）。一旦标 `partial`，partial 重复成员检测按塌缩派发键判重 → 误报跨碎片重复 → 无法用 partial
   拆分（String.z42 加满会超 500 行硬限）。
2. **E0436**：`String` 的**实例**方法经 `CallEmitter` 的 `Deps.GetInstance(name,arity)` 跨包捷径解析，
   新方法名与下游包实例方法 (name,arity) 碰撞（`Split(char[])` ↔ `Regex.Split`；char-IndexOf 块 ↔ Cli）
   → z42c 记录对 `Std.Cli`/`Std.Regex` 的虚假依赖，而 z42.core **不能** import 它们（循环）→ 误报。

> **对比**：`Std.Text.Strings`（静态方法，scoped 解析，非 prelude）两道都避开——这也是其文件头「字符串
> 工具放 opt-in Strings、不加 prelude String」设计决策的**技术根因**被本次实测证实。User 裁决仍走 prelude
> String（编译器修复 + 两-nightly），故 E0436（必要）[+ 若用 partial 再加 E0433] 修复作为独立 support
> change 先行，String 方法待其随一个 nightly 发布后再落。

## 备注

- 无 zbc/zpkg 格式 bump。无编译器改动（本 PR 四类全部 seed 兼容，实测 `build stdlib` 25/25）。
- 验证过泛型静态方法编译 + 运行（`new T[n]` / 装箱 `.Equals` / 元素交换）——Array 算法基于此。
