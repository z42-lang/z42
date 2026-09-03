# Proposal: `Array.Sort<TKey,TValue>` 配对排序 + 泛型 arity 重载过滤

> 状态：进行中 | 创建：2026-09-03 | 类型：compiler（overload 决议）+ stdlib feat。**无格式 bump**。

## What / Why

补齐 `Std.Array` 向 C# `System.Array` 看齐的最后一项——**配对排序**
`Sort<TKey,TValue>(TKey[] keys, TValue[] items)`（按 keys 升序，items 同步换位）。

这一项此前（#382）被移出，因为它触发一个**编译器 overload 决议 bug**：`Sort<TKey,TValue>` 与既有
`Sort<T>(T[], Func<T,T,int>)` **同为 2 个值参、仅泛型类型形参个数不同**（2 vs 1）。加入后
`Array.Sort<int>(arr, lambda)`（1 个显式类型实参）**静默误绑**到 2-类型参的配对重载、丢掉 comparator
（降序 comparator 测试变升序），且**无歧义诊断**。

根因：`OverloadBinder._resolveOverload` 只按**值参数 arity** 过滤候选，从不看**显式类型实参个数**
（`call.TypeArgCount` 现成可用），且泛型形参的松散可赋值让 lambda「匹配」到 `TValue[]`。

## What Changes（方案 A：泛型 arity 过滤，C# 规则）

**核心**：调用带 N 个显式类型实参（`Sort<int>` → N=1）时，只有**恰好声明 N 个类型形参**的候选合格
（`Sort<T>` 合格；`Sort<TKey,TValue>` 不合格）。

**关键：无格式 bump**——SIGS 段**早已存**方法级泛型形参个数+名字（add-reflective-invoke 起 writer 写
真实值、reader 已读该块），此前只是**读弃**。本 change 只**捕获并上浮**该已有元数据。

1. **捕获**（z42.ir）：`ZpkgReader.ReadModuleSigs` 把已读的 `tpc` 写进 `IrFunction.TypeParamCount`
   （原读弃）；`ZbcReader._readSigs` 同款存进 `SigEntryZ.TypeParamCount`。
2. **上浮**（z42.ir → 编译器）：`ExportedMethodZ.TypeParamCount`（ABI-安全 post-ctor，默认 0）由
   `TsigReconcile._methodFromSig` 从 `IrFunction.TypeParamCount` 填；`MethodSymbol.TypeParamCount`
   本地由 `SymbolCollector` 从 `Decl.TypeParams.Count` 设、导入由 `ImportedSymbolLoader` 从
   `ExportedMethodZ.TypeParamCount` 还原。
3. **过滤**（`OverloadBinder._resolveOverload`）：新增 `typeArgCount` 形参；`typeArgCount>0` 时把
   `byArity` 再按 `TypeParamCount == typeArgCount` 过滤。**防御性**：仅当过滤后仍非空才采用——候选
   `TypeParamCount` 未知/为 0 时不误删全部候选，**保证不回归既有可用泛型调用**。7 个 `_resolveOverload`
   调用点传 arg（限定名/无限定静态调用传 `call.TypeArgCount`；实例调用/运算符无 CallExpr 传 0）。
4. **feature**（stdlib）：`Array.Sort<TKey,TValue>(TKey[], TValue[])` 配对归并排序（stable，O(n log n)）。

## Scope

| 文件 | 变更 |
|------|------|
| `z42.ir/src/BinaryFormat/ZbcReader.z42` | `SigEntryZ.TypeParamCount` + `_readSigs` 捕获 tpc |
| `z42.ir/src/ZpkgReader.z42` | `stub.TypeParamCount = tpc`（跨包 SIGS 读） |
| `z42.ir/src/ExportedTypes.z42` | `ExportedMethodZ.TypeParamCount`（post-ctor） |
| `z42.ir/src/TsigReconcile.z42` | `_methodFromSig` 填 `em.TypeParamCount` |
| `z42c.semantics/src/Symbol.z42` | `MethodSymbol.TypeParamCount` |
| `z42c.semantics/src/SymbolCollector.z42` | 本地 `ms.TypeParamCount = mtpc` |
| `z42c.semantics/src/ImportedSymbolLoader.z42` | 导入 `sym.TypeParamCount = me/m.TypeParamCount`（2 处） |
| `z42c.semantics/src/OverloadBinder.z42` | `_resolveOverload` 加 `typeArgCount` + 防御性 arity 过滤 |
| `z42c.semantics/src/MemberResolver.z42` + `ExprTyper.z42` | 7 个调用点传 typeArgCount |
| `z42.core/src/Array.z42` | `Sort<TKey,TValue>` + `_mergeSortPaired` |
| `z42.core/tests/array_algorithms.z42` | 配对排序测试 + comparator 回归 |
| `z42.core/README.md` | Array 行加配对 Sort |

## Out of Scope

- **完整 C# 泛型 arity 规则的其余角落**：本 change 只做「显式类型实参个数 → 类型形参个数」过滤，且**防御性**
  （不误删）。极罕见的「结构兼容同-arity 歧义」仍走原 type-based 决议，不额外收紧。
- **类型实参个数与形参个数**的**严格校验**（`Foo<int,string>` 调 1-类型参 `Foo` 报错）——本 change 只在
  na>1 消歧时过滤，不引入新诊断（避免误伤）。
