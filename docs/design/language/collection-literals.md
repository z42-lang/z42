# z42 集合字面量（`{}` List / Dictionary）

> **Status**: L1 ✅（add-collection-literals，2026-08-07）
> 数组 `[]` 侧见 [arrays.md](arrays.md)；本页讲花括号 `{}` 侧（List / Dict / 空）。

## 设计参考

| 来源 | 借鉴点 |
|------|--------|
| **JSON / JavaScript** | `[]`＝数组、`{}`＝对象/映射 的直觉分工 |
| **C#** | 集合初始化器用 `{}`（`new List<int>{1,2,3}`）；Dictionary 键值语法 |
| **Python** | `{"a": 1}` 字典字面量 |

---

## 语法总览：`[]` 数组，`{}` 花括号族

| 字面量 | 归属 | 判据 |
|--------|------|------|
| `[1, 2, 3]` `[0; n]` `[..a]` | 数组 `T[]`（专属） | 方括号一律数组（见 arrays.md） |
| `{1, 2, 3}` | `List<T>` | 花括号 + 裸元素 |
| `{"a": 1, "b": 2}` | `Dictionary<K,V>` | 花括号 + `key: value` 对 |
| `[]` / `{}`（空） | 由目标类型定 | 无目标类型 → 报错 |

```z42
using Std.Collections;

List<int>              nums   = { 1, 2, 3 };            // → new List<int>(); Add(1); Add(2); Add(3);
var                    more   = { 100, 200 };           // 裸元素 → List<int>
Dictionary<string,int> scores = { "a": 90, "b": 85 };  // k:v → new Dictionary; Set("a",90); Set("b",85);
var                    m2     = { "x": 1 };             // k:v → Dictionary<string,int>
List<int>              el     = {};                     // 空 List（目标类型定）
Dictionary<string,int> ed     = {};                     // 空 Dict（目标类型定）
```

---

## 消歧规则

### 1. 位置消歧（块 vs 花括号字面量）

`{...}` 只在**表达式位置**（赋值右侧、实参、`return`、集合元素…）解析为 List/Dict 字面量；
**语句位置的 `{` 永远是块**。z42 无块表达式，故位置即可判定，无二义。花括号字面量作**裸表达式
语句**（`{ 1, 2 };`）不允许——无用途，归块解析。

### 2. 内容消歧（List vs Dict）

进入花括号字面量后看内容：

- 元素形如 `expr : expr`（冒号对）→ **Dictionary**；
- 裸 `expr`（无冒号）→ **List**；
- 空 `{}` → 由目标类型定（`List<..>` / `Dictionary<..>`）；无目标或目标非集合 → 报错；
- `字段 = 值`（`new Type { X = 1 }`）→ 对象初始化器，**本轮未支持**（后续 change）。

同一 `{}` 内混用冒号对与裸元素 → 报错。

---

## 脱糖（纯前端，零新 IR / 零格式 bump）

| 形态 | 脱糖为 |
|------|--------|
| `{e0, e1, ..}` | `$c = new List<T>(); $c.Add(e0); $c.Add(e1); ..; ⟨值＝$c⟩` |
| `{k0: v0, ..}` | `$c = new Dictionary<K,V>(); $c.Set(k0, v0); ..; ⟨值＝$c⟩` |
| `{}`（目标 List/Dict） | `new List<T>()` / `new Dictionary<K,V>()` |

- 目标类型（`List<long> x = {1,2,3}`）决定 `T`/`K`/`V`；无目标时由首元素/首键值推断。
- `List<T>` 的元素须满足其约束 `T: IEquatable<T> + IComparable<T>`，`Dictionary` 键须满足
  `TKey: IEquatable<TKey>`（与手写 `new List<T>()` 同约束）。
- 无目标时元素类型经 `Z42Type → TypeExpr` 合成（prim / 数组 / 泛型实例化 / 具名类短名）；
  非平凡元素类型（如需 FQ 的用户类）建议显式写目标类型。

实现原理（合成 AST + `BoundSeqExpr` 序列表达式 + emitter 委托）见
[`docs/spec/changes/add-collection-literals/design.md`](../../spec/changes/add-collection-literals/design.md)。

---

## 与其他特性的关系

- **数组 `[]`**：[arrays.md](arrays.md)——方括号侧，含重复 `[v;n]` 与 spread。
- **对象初始化器 `new Foo { X = 1 }` / 字段简写 / 结构更新 `..base`**：后续 change（依赖 struct 值语义）。
- **两阶段 nightly 纪律**：本特性只落"支持"，z42c / stdlib 源码晚一个 nightly 才 use（见
  [`bootstrap-seed.md`](../../../.claude/rules/bootstrap-seed.md)）。
