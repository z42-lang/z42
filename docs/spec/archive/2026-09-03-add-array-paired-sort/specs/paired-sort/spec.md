# Spec: 泛型 arity 重载过滤 + 配对排序

## Requirement: 泛型 arity 参与重载决议

调用带 N 个显式类型实参时，只有恰好声明 N 个类型形参的候选合格（当该过滤后仍有候选）。

#### Scenario: comparator sort 不被配对重载偷走（bug 回归）
- **WHEN** `Array.Sort<int>(arr, (a,b)=>b-a)`（1 个类型实参，第二参是 lambda），同名有 `Sort<T>(T[],Func)` 与 `Sort<TKey,TValue>(TKey[],TValue[])`
- **THEN** 绑到 `Sort<T>`（comparator），数组降序；**不**误绑配对重载丢 comparator

#### Scenario: 配对重载正常绑定
- **WHEN** `Array.Sort<int,string>(keys, items)`（2 个类型实参）
- **THEN** 绑到 `Sort<TKey,TValue>`

#### Scenario: 防御性——counts 缺失不回归
- **WHEN** 某泛型方法候选的 `TypeParamCount` 未回填（=0）、调用带类型实参
- **THEN** arity 过滤结果为空 → 退回原 byArity（不误删唯一候选 → 既有泛型调用不断）

## Requirement: `Array.Sort<TKey,TValue>(TKey[] keys, TValue[] items)` 配对排序

按 `keys` 升序（`CompareTo`），`items` 与 `keys` 平行同步换位；stable。

#### Scenario: 配对排序 + stable
- **WHEN** `keys=[3,1,4,1,5]`, `items=["c","a","d","A","e"]`, `Sort<int,string>(keys, items)`
- **THEN** `keys=[1,1,3,4,5]`；`items=["a","A","c","d","e"]`（两个 key=1 保持 "a" 先于 "A"）

#### Scenario: n<2 no-op
- **WHEN** `keys=[7]`, `items=["x"]`
- **THEN** 不变

## 无格式 bump
SIGS 段方法级 tp 块自 add-reflective-invoke 起已写真实值、reader 已读——本 change 仅捕获 count 上浮，
zbc/zpkg wire 布局与版本**不变**。
