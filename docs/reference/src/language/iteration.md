# 迭代（foreach）

`foreach` 遍历数组和集合。z42 支持**三种**可迭代形态，编译器按固定顺序判定用哪一种。

## 语法

```z42
foreach (var item in collection) { /* ... */ }
foreach (string s in names)      { /* ... */ }   // 也可以写显式类型
```

- 写 `var` 时元素类型由编译器推断；写显式类型时按该类型绑定。
- `in` 后面的表达式**只求值一次**，不是每轮一次。
- 循环体里可以用 `break` / `continue`。

## 三条路径与判定顺序

编译器按下面的顺序决定 `foreach` 怎么编译——**顺序很重要，前面的赢**：

| 顺序 | 条件 | 走哪条路径 |
|---|---|---|
| 1 | 目标是数组 `T[]` | **数组路径** |
| 2 | 有**整数下标**的 `get_Item`，**且**有计数成员 `Count` 或 `Length` | **索引路径** |
| 3 | 有 `GetEnumerator()`（而不满足第 2 条） | **枚举器路径** |

判定看的是目标**静态类型的成员**，不看它是类还是接口——`foreach (int x in xs)` 里
`xs` 声明成具体类还是声明成接口，走的是同一条路径、编出同样的代码。

`string` 走第 2 条（`Length` + `this[int]`），所以 `foreach (char c in s)` 直接可用，
**不物化 `char[]`**；`Length` / `CharAt` 都是 O(1) 摊还，按**字符**（scalar）计而非字节。

> ⚠️ 第 2 条优先于第 3 条。**一个同时提供整数索引器和 `GetEnumerator()` 的类型会走索引路径**，
> 即使它实现了 `IEnumerable<T>`。
>
> 「整数下标」这个限定是必要的：`Dictionary<K,V>` 的索引器是 `this[TKey]`，若只看「有没有
> `get_Item`」，它会被判去走索引路径、拿 `int` 计数器调 `get_Item(TKey)`。

> 2026-09 之前判定只认类那条继承线，目标的静态类型写成**接口**时三条路径全部落空，
> `foreach` 静默落到数组路径、对一个对象发 `array_len`：编译期零诊断，运行期抛
> `ArrayLen: expected array`。接口上的计数成员只以 `get_Count` / `get_Length` 形态
> 出现（接口没有字段面），这一档同样认。现已修正。

### 路径 1：数组

按下标从 `0` 到 `Length - 1` 遍历，无额外开销。

### 路径 2：索引鸭子协议

类型只要同时提供这两样，不需要实现任何接口：

```z42
public int Count;              // 或 Length；字段 / 方法 / 属性都行
public T get_Item(int i);      // 通常由索引器 `public T this[int i]` 合成
```

计数成员按 **`Count` 优先、其次 `Length`** 查找，三种声明形态都认：

| 声明 | 编译成 |
|---|---|
| `public int Count;`（字段） | `field_get Count` |
| `public int Count()`（方法） | `vcall Count()` |
| `public int Count { get; }`（属性） | `vcall get_Count()` |

标准库的 `List<T>` 走字段那档，`string` 走 `Length` 属性那档。

> 2026-09 之前只认字段与方法两档。`Count` 写成**属性**时编译器按源名发 `field_get`——
> 而 auto 属性的存储叫 `__prop_Count`、计算属性根本没有存储 ⇒ 读到空值 ⇒ 按一个垃圾
> 长度多迭代、静默越界。现已修正。

### 路径 3：枚举器协议

类型提供 `GetEnumerator()` 即可，**不要求显式实现 `IEnumerable<T>`**——按形状匹配，
返回的枚举器也按形状调用，是具体类型而非接口，因此**不装箱**。

枚举器需要满足：

```z42
public interface IEnumerator<T> : IDisposable {
    bool MoveNext();
    T Current { get; }        // 是属性，不是方法
}
```

`foreach` 在这条路径上等价于：

```z42
var __e = collection.GetEnumerator();
try {
    while (__e.MoveNext()) {
        var item = __e.Current;
        /* 循环体 */
    }
} finally {
    __e.Dispose();
}
```

`try` / `finally` 保证**任何离开方式**（正常结束、`break`、`return`、抛异常）都会调到 `Dispose()`。
`IEnumerator<T>` 继承 `IDisposable` 正是为了这个。

> **`Dispose` 是条件步骤，不是必需成员。** 枚举器**没有** `Dispose` 时，脱糖就只剩那个 `while`
> ——连 `try`/`finally` 一起省掉（finally 里没有任何事可做）。所以上面那两个成员
> （`MoveNext` + `Current`）就是枚举器形状的**全部**要求，与 C# 一致。
>
> 2026-09-25 之前是**无条件**发 `__e.Dispose()`：照本节写的最小枚举器编译会报
> `E0401: no method Dispose`，位置还指在 `foreach` 那一行、不提这个调用是脱糖合成的 ——
> 读者对不上号，只能被迫给每个自定义枚举器加一个空 `Dispose(){}` 桩。

## 协议接口

`Std.IEnumerable<T>` / `Std.IEnumerator<T>` 定义在标准库里。`foreach` 本身**不要求**你实现它们
（三条路径都是按形状匹配的），但显式实现有两个好处：

- 能用作泛型约束：`where T : IEnumerable<U>`
- 向读代码的人声明「这个类型可迭代」

显式实现它们与自定义形状走的是同一条路径：`Std.*` 在另一个包，跨包导入的接口带着父接口链，
`IEnumerator<T>` 上因此能找到继承自 `IDisposable` 的 `Dispose`，脱糖的 `finally` 照常成立
（见[接口](interfaces.md)的「接口继承」一节）。

## `Dictionary<K, V>`

`Dictionary<K, V>` 走**枚举器路径**（第 3 条）：它的索引器是 `this[TKey]`，不是整数下标，
因此不命中第 2 条。直接 `foreach` 即可，元素是 `KeyValuePair<K, V>`：

```z42
foreach (var kv in dict) { /* kv.Key / kv.Value */ }
```

**遍历顺序不作保证**。只要键或只要值时，`Keys()` / `Entries()` 仍然可用，但它们返回的是
**快照数组**（每次调用分配一次），逐项遍历用 `foreach (var kv in dict)` 更省。

> 2026-09 之前第 2 条只看「有没有 `get_Item`」，`Dictionary` 因此被判去走索引路径 ——
> `foreach` 跑满 `Count` 轮、每轮取到 `0`，且**没有任何诊断**。现已修正。

## 已知陷阱

> ### 迭代变量的只读性未强制
>
> C# 禁止给 `foreach` 的迭代变量赋值。**z42 当前没有这项检查**，写了也不报错，
> 但赋值只影响副本、不会写回集合。别依赖这个行为。

## 为什么这样设计

- **为什么不让 `foreach` 无条件走 `IEnumerable`**——索引路径对数组式集合能生成更紧凑的代码，
  没有枚举器对象、没有虚调用。有索引面就用索引面。
- **为什么 `IEnumerator` 是 C# 风格（`MoveNext` + `Current`）而不是 Rust 风格（`next() -> Option<T>`）**
  ——z42 的类型系统没有 `Option<T>`，C# 风格不需要它。
- **为什么 `IEnumerator<T>` 继承 `IDisposable`**——迭代状态可能持有需要释放的资源，
  统一由 `foreach` 生成的 `finally` 负责。

## 相关

- [数组](arrays.md)
- [控制流](control-flow.md)——`break` / `continue` 的规则
- [泛型约束](generic-constraints.md)——`where T : IEnumerable<U>`
