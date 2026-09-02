# Design: 泛型 arity 重载过滤 + 配对排序

## 数据流（TypeParamCount 上浮，无格式 bump）

```
zbc/zpkg SIGS 段（早已存方法级 tp 块：count u8 + 每 tp 名+约束）
   │  add-reflective-invoke：writer 写真实值；reader 读该块——但此前 count 读弃
   ├─(本地编译) MethodDecl.TypeParams.Count ──────────────────► SymbolCollector: ms.TypeParamCount
   └─(跨包导入) ZpkgReader.ReadModuleSigs: tpc → IrFunction.TypeParamCount   ← 新捕获（stub.TypeParamCount=tpc）
                     │
                     └► TsigReconcile._methodFromSig: em.TypeParamCount = f.TypeParamCount   ← 新
                            │
                            └► ImportedSymbolLoader: sym.TypeParamCount = me.TypeParamCount    ← 新
                                   │
                                   ▼
   OverloadBinder._resolveOverload(..., typeArgCount, ...):
       byArity（值 arity 过滤）→ if typeArgCount>0 && na>1: 保留 TypeParamCount==typeArgCount 者（防御性：非空才采用）
```

## Decisions

### D1: 方案 A（泛型 arity 过滤），非方案 B（收紧 assignable）
User 裁决。A 是 C# 规则（泛型 arity 是方法标识的一部分），通用；B（禁 lambda 匹配数组形参）局部但只解
lambda-vs-array 一种歧义。A 需类型形参个数元数据——**发现 SIGS 早已存**（add-reflective-invoke），故 A
退化为纯上浮，**无格式 bump**、与 B 同等轻量却更通用。

### D2: 防御性过滤（不误删全部候选）——**关键正确性保证**
`typeArgCount>0` 时按 `TypeParamCount==typeArgCount` 过滤，但**仅当过滤结果非空才采用**。理由：某些
候选路径的 `TypeParamCount` 可能未回填（=0），若无条件过滤会把 `Sort<int>` 的**唯一**正确候选（tp=1）
在其 count 恰为 0 的场景误删 → 无候选 → 破坏既有可用泛型调用。防御性版**永不回归**：counts 齐备时正确
消歧，counts 缺失时退回原行为（不比修改前差）。

### D3: 只在 na>1（值-arity 消歧）时过滤，不引入新诊断
不做「类型实参数 ≠ 形参数则报错」的严格校验（如 `Foo<int,string>` 调 1-tp `Foo`）——那会误伤边角、超本
change 目标。只把 arity 当**消歧信号**，行为最小侵入。

### D4: 7 个 `_resolveOverload` 调用点
- 有 `CallExpr call`（限定名静态 `Array.Sort<int>`、无限定静态、_bindCall 静态）→ 传 `call.TypeArgCount`。
- 无 `call`（`_bindInstanceMemberCall` 实例调用 ×3、运算符重载）→ 传 0（不启用过滤；实例泛型重载消歧
  罕见，防御性下无回归）。Sort 是限定名静态调用 → 走 `call.TypeArgCount` 路径，覆盖。

### D5: 配对排序实现
`Sort<TKey,TValue>` 复用归并排序骨架（`_mergeSortPaired`），keys 比较用 `CompareTo`（升序），items 数组
与 keys 平行换位。stable（同 key 保持原相对序）。O(n log n)、一次性分配 kScratch/vScratch。

## Testing
- `test_sort_comparator_descending`（既有，回归）：`Sort<int>(arr, lambda)` 仍绑 comparator（降序），不被
  配对重载偷走——**这是 bug 的直接复现/验证点**。
- `test_sort_paired` / `test_sort_paired_noop_small`（新）：`Sort<int,string>(keys, items)` 正确配对+stable。
- 完整 `xtask test`：全 stdlib + 自举不动点（gen1==gen2；本 change 改 z42c 逻辑，须确认字节收敛）。
