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
| 2 | 目标是类，且有 `get_Item`，**且**有计数成员 `Count` 或 `Length` | **索引路径** |
| 3 | 目标是类，且有 `GetEnumerator()`（而不满足第 2 条） | **枚举器路径** |

`string` 走第 2 条（`Length` + `this[int]`），所以 `foreach (char c in s)` 直接可用，
**不物化 `char[]`**；`Length` / `CharAt` 都是 O(1) 摊还，按**字符**（scalar）计而非字节。

> ⚠️ 第 2 条优先于第 3 条。**一个同时提供索引器和 `GetEnumerator()` 的类型会走索引路径**，
> 即使它实现了 `IEnumerable<T>`。见下方「已知陷阱」。

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

## 协议接口

`Std.IEnumerable<T>` / `Std.IEnumerator<T>` 定义在标准库里。`foreach` 本身**不要求**你实现它们
（三条路径都是按形状匹配的），但显式实现有两个好处：

- 能用作泛型约束：`where T : IEnumerable<U>`
- 向读代码的人声明「这个类型可迭代」

## 已知陷阱

> ### `Dictionary<K, V>` 不能直接 foreach
>
> `Dictionary<K, V>` 同时有 `Count` 字段和 `this[TKey key]` 索引器，因此命中**索引路径**，
> 编译器会拿 `int` 下标去调 `get_Item(TKey)`。请改用快照方法：
>
> ```z42
> foreach (var k in dict.Keys())    { /* ... */ }
> foreach (var e in dict.Entries()) { /* e 是 KeyValuePair<K,V> */ }
> ```
>
> 这是当前实现的缺口，不是设计意图。

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
