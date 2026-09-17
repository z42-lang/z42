# 集合字面量（`{}` List / Dictionary）

> 对齐：2026-08-07（change `add-collection-literals`）

花括号 `{}` 在**表达式位置**是 List / Dictionary 字面量；方括号 `[]` 一律是数组，见
[数组](arrays.md)。

| 字面量 | 归属 | 判据 |
|--------|------|------|
| `[1, 2, 3]` `[0; n]` `[..a]` | 数组 `T[]`（专属） | 方括号一律数组 |
| `{1, 2, 3}` | `List<T>` | 花括号 + 裸元素 |
| `{"a": 1, "b": 2}` | `Dictionary<K,V>` | 花括号 + `key: value` 对 |
| `{}`（空） | 由目标类型定 | 无目标类型 → 报错 |

```z42
using Std.Collections;

List<int>              nums   = { 1, 2, 3 };
var                    more   = { 100, 200 };           // 裸元素 → List<int>
Dictionary<string,int> scores = { "a": 90, "b": 85 };
var                    m2     = { "x": 1 };             // k:v → Dictionary<string,int>
List<int>              el     = {};                     // 空 List（目标类型定）
Dictionary<string,int> ed     = {};                     // 空 Dict（目标类型定）
```

## 消歧规则

### 1. 位置消歧（块 vs 花括号字面量）

`{...}` 只在**表达式位置**（赋值右侧、实参、`return`、集合元素…）解析为 List / Dict 字面量；
**语句位置的 `{` 永远是块**。z42 无块表达式，故位置即可判定，无二义。花括号字面量作**裸表达式
语句**（`{ 1, 2 };`）不允许——无用途，归块解析。

### 2. 内容消歧（List vs Dict）

进入花括号字面量后看内容：

- 元素形如 `expr : expr`（冒号对）→ **Dictionary**；
- 裸 `expr`（无冒号）→ **List**；
- 空 `{}` → 由目标类型定；无目标类型、或目标不是 `List<..>` / `Dictionary<..>` → 报 **E0402**
  （发射点 `src/compiler/z42c.semantics/src/CollectionTyper.z42:132` / `:141`）；
- `字段 = 值`（`new Type { X = 1 }`）→ [对象初始化器](object-initializers.md)，不是本页的形态。

同一 `{}` 内混用冒号对与裸元素 → 报错。

## 语义

| 形态 | 等价于 |
|------|--------|
| `{e0, e1, ..}` | `$c = new List<T>(); $c.Add(e0); $c.Add(e1); ..; ⟨值＝$c⟩` |
| `{k0: v0, ..}` | `$c = new Dictionary<K,V>(); $c.Set(k0, v0); ..; ⟨值＝$c⟩` |
| `{}`（目标 List/Dict） | `new List<T>()` / `new Dictionary<K,V>()` |

> Dictionary 侧用的是 **`Set`**（覆盖式写入），不是 `Add`。

- **元素类型**：有目标类型时由目标决定（`List<long> x = {1,2,3}` → `T = long`）；无目标时由
  首元素 / 首键值推断。
- **约束照常生效**：与手写 `new List<T>()` / `new Dictionary<K,V>()` 完全同一套约束——`List<T>`
  对元素无约束；`Dictionary<TKey, TValue>` 要求 `where TKey : IEquatable`（**非泛型** `IEquatable`，
  配合 [`Self`](generic-constraints.md#self-类型仅接口)；基元 key 由 VM 内建路由，用户类型 key
  必须实现 `IEquatable`）。
- **非平凡元素类型建议写目标类型**：无目标时元素类型由编译期合成，覆盖基元 / 数组 / 泛型实例化 /
  具名类短名；需要全限定名才能定位的用户类，显式写出目标类型更稳。

## 相关

- [数组](arrays.md) —— `[]` 侧，含重复 `[v; n]` 与 spread `[..a]`
- [对象初始化器](object-initializers.md) —— `new Foo { X = 1 }` / 字段简写
- [泛型约束](generic-constraints.md) —— `IEquatable` 等约束的判定规则
