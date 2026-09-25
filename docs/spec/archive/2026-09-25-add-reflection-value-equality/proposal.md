# proposal：add-reflection-value-equality

## 一句话

`typeof(int) == typeof(int)` 今天是 **`false`** —— `Std.Type` 没有值相等，`==` 落引用比较，
而每次 `typeof` 新建对象。于是**一切按 `typeof` 分派的代码都静默走错分支**（不分泛型与否）。
给 `Type` / `MethodInfo` 补按身份名的值相等。

## 实测

```
typeof(int) == typeof(int)        = false
typeof(int).Equals(typeof(int))   = false
typeof(int).FullName == 同上       = true      ← 名字一直是对的
x.GetType() == typeof(int)        = false
```

🔴 **这与泛型无关**：非泛型的 `typeof(int) == typeof(int)` 就已经是 `false`。

## 它是怎么被记错的

坑点清单第 ③ 条记的是「**类级** `typeof(T)` 产占位名 ⇒ `typeof(T) == typeof(int)` → false
⇒ 按 typeof 分派的泛型代码静默走错分支」，并注明「方法级 `typeof(U)` 是对的」。

实测把这个归因拆成了**两个互不相干的缺陷**：

| | 现象 | 真因 |
|---|---|---|
| ③（`add-generic-methods` 的 D3 延后项）| 类级 `typeof(T).FullName` → `"T"` | 类级型参不解析，产占位名 |
| **本 change** | **任何** `typeof(a) == typeof(b)` → `false` | `Type` 无值相等 |

而且「方法级是对的」**只对了一半**：方法级 `typeof(U).FullName` 确实是 `Std.Int32`，
但 `typeof(U) == typeof(int)` **同样是 false** —— 因为坏的是相等，与型参层级无关。

⇒ **只修 ③ 不会让招牌症状消失**（`FullName` 变对、`==` 仍 false）。本 change 先行。

## 🔴 规范冲突（本 change 一并裁掉）

| 出处 | 说法 |
|---|---|
| `Type.z42` 头注 | 「`typeof(int)` is the **same** Type as `(5).GetType()`（C# `typeof(int) == 5.GetType()`）」 |
| `methodof.md:74` | 「对象身份：每次求值**新建**，`typeof(T) == typeof(T)` 为 `false`」，且称这是**刻意**保持与 `methodof` 对称，「反射对象驻留是独立的优化项，要做就两边一起做」 |

前者**是假的**（实测 false）。后者描述的行为属实，但**归因错了**：C# 里
`typeof(int) == typeof(int)` 为真**不是靠对象驻留**，而是靠 `Type` 的**值相等语义**。
两者是不同的东西——值相等不要求任何缓存，也不引入对象身份语义。

**以「值相等」为准**：`Type.z42` 那句假断言按新行为变成真；`methodof.md` 那条改写成
「**值相等按身份名**；对象身份（驻留）仍不保证，且那是另一回事」。

## 改动

### ① `Std.Type`：`op_Equality` / `op_Inequality` / `Equals` / `GetHashCode`

判据 = **`__fullName`**（FQ 名，VM 写入）。理由：`Type` 的身份在 z42 里就是 FQ 名——
`typeof(int)` 与 `(5).GetType()` 都得 `Std.Int32`（`fix-type-reflection-names` 起如此），
数组等无句柄合成体也有填好的 `FullName`。

⚠️ **null 两侧都要处理**：`t == null` 必须照常可用（今天是引用比较、恒可写）。

### ② `Std.Reflection.MethodInfo`：同款，判据 = `__qualified`

**必须一起做**，否则就制造出 `methodof.md` 明确警告的那种不对称
（「单给一边加……会让两个号称对称的特性行为不一致」）。判据取 `MethodBase.__qualified`
（FQ 方法名，重载决议后已定）。

## 边界

- **不做对象驻留**：`ReferenceEquals(typeof(int), typeof(int))` 仍为假。值相等不需要它，
  驻留仍是独立的优化项（`methodof.md` 那句保留，只是从「解释为什么 == 是 false」改成
  「解释对象身份仍不保证」）。
- `Std.Type` 是 `sealed`，无派生类 ⇒ 不必考虑子类重写。

## 风险

- ⚠️ **行为变更**：今天写 `typeof(a) == typeof(b)` 恒得 `false`；改后按名比较。
  依赖「恒假」的代码会变 —— 但那样的代码本身就是 bug（没人会**故意**写一个恒假比较）。
  爆炸半径本 change 内实测。
- ⚠️ `op_Equality` 在**引用类型**上的派发已验证可用（用户 class 实测通过），机制现成。
