# Proposal: List.Sort 委托版排序（Func<T,T,int>）

## Why

`List<T>.Sort()` 当前只有唯一无参重载，写死走元素自身的 `IComparable<T>.CompareTo`
（`List.z42:130`）——排序准则被硬编码进类型，用户无法按调用点提供自定义排序逻辑。
`library_review.md` 优先级 #1 提出加委托版排序立即消除该痛点。

**设计校正（对 review 的事实修正）**：review 说加 `Sort(Comparison<T>)`「现在就能做」，前提有误——
z42c **不支持泛型用户委托**（`Comparison<T>` 无法作为 stdlib 委托解析，仅内建 Action/Func/Predicate
是泛型委托，见 `BuiltinTypeDefs.z42` + `SymbolTable.z42:131-147`）。且 C# 的 `Comparison<T>` 与
`Func<T,T,int>` 是**同签名的名义重复**（C# 需 `Comparer.Create` 搭桥两者），是应剔除的 wart。
故本 change **只用已有的内建 `Func<T,T,int>`** 作委托形态——既现在就能做、纯 stdlib，又主动避开
C# 的名义冗余。接口形态 `IComparer<T>`（职责不同：可复用/有状态/`.Default`）留待 E3。

## What Changes

- `List<T>` 新增 `Sort(Func<T, T, int> comparison)` 重载：复用现有自顶向下归并排序
  （stable, O(n log n)），比较从 `a.CompareTo(b)` 换成 `comparison(a, b)`（<0 ⇒ a 在前）。
- 回归测试覆盖：自定义升/降序、稳定性、空/单元素、与无参 `Sort()` 一致性。
- 不新增委托类型（复用内建 Func）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.core/src/Collections/List.z42` | MODIFY | 加 `Sort(Func<T,T,int>)` + `MergeSortCmp` 私有辅助 |
| `src/libraries/z42.core/tests/list_sort.z42` | MODIFY | 加委托版排序测试 |

**只读引用**：

- `src/libraries/z42.threading/src/RwLock.z42` — 参考委托直接调用语法 `body(x)`
- `src/compiler/z42c.semantics/src/SymbolTable.z42` — 确认 Func 为内建可解析委托、泛型用户委托不支持

## Out of Scope

- `Sort(IComparer<T>)` 接口版（被泛型接口 TypeArgs 分发 bug 阻塞，另立项 E3）
- z42c 支持泛型用户委托 / `Comparison<T>` 内建委托（独立 lang change，若将来需要）
- 松绑 `List<T>` 的 `where T: IEquatable + IComparable` 约束（属 E2 / EqualityComparer.Default）
- `Array.Sort` 静态算法（属 PR-D）

## Open Questions

- 无（Func 委托可解析、归并排序、调用语法均已实证）
