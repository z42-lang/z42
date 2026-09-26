# 数组与集合

前面存的都是单个值。这一章讲怎么**一次装一批**：定长的数组、能增删的 `List`、按键取值的
`Dictionary`。

## 数组

`T[]` 是**定长**的一串同类型元素，建出来长度就不变了。

```z42
// examples/basics/collections/arrays/arrays.z42
{{#include ../../../../examples/basics/collections/arrays/arrays.z42:code}}
```

```console
{{#include ../../../../examples/basics/collections/arrays/run.console:run}}
```

- **下标从 `0` 开始**，`a.Length` 是长度。
- `new int[3]` 只给长度时元素是**零值**：数字 `0`、`bool` 是 `false`、引用类型是 `null`。

### 🔴 下标越界会直接终止程序

越界抛出的东西**不是 `Exception` 的实例**，所以 `catch (Exception e)` 认不出它，
程序就停在那里：

```z42
// examples/basics/collections/arrays/oob.z42
{{#include ../../../../examples/basics/collections/arrays/oob.z42}}
```

```console
{{#include ../../../../examples/basics/collections/arrays/run.console:oob}}
```

**访问下标前自己确认范围**——这是唯一正确的做法。
（什么都接的 `catch { }` 技术上能拦下它，但那会连你没预料到的问题一起吞掉；
细节见异常处理一章。）

### 方括号字面量

`[...]` 是数组的专属写法，比 `new int[] {...}` 短：

```z42
// examples/basics/collections/literals/literals.z42
{{#include ../../../../examples/basics/collections/literals/literals.z42:code}}
```

```console
{{#include ../../../../examples/basics/collections/literals/run.console:run}}
```

三种形态：

| 写法 | 意思 |
|------|------|
| `[1, 2, 3]` | 逐个列出元素 |
| `[0; 5]` | **重复填充**——5 个 `0`，数量可以是运行期算出来的 |
| `[..xs, 99, ..ys]` | **展开拼接**——把 `xs` 和 `ys` 的元素摊平，中间插 `99` |

> ⚠️ `[v; n]` 的 `v` **只求值一次**。`v` 是引用类型时，n 个格子指向**同一个对象**，
> 改一个就是改全部。

空数组 `[]` 必须让编译器知道元素类型——`int[] e = [];` 可以，`var e = [];` 不行。

### 多行数组

`T[][]` 是「数组的数组」，每行长度可以不同：

```z42
// examples/basics/collections/jagged/jagged.z42
{{#include ../../../../examples/basics/collections/jagged/jagged.z42:code}}
```

```console
{{#include ../../../../examples/basics/collections/jagged/run.console:run}}
```

> 熟悉 C# 的读者请注意：**z42 没有 `T[,]` 多维数组**，只有这种交错形式；
> `a[i, j]` 会报错并提示你改写成 `a[i][j]`。

## `List<T>`：能增删的序列

数组定长，要随时加东西就用 `List<T>`。**花括号 `{}` 是它的字面量**：

```z42
// examples/basics/collections/list/list.z42
{{#include ../../../../examples/basics/collections/list/list.z42:code}}
```

```console
{{#include ../../../../examples/basics/collections/list/run.console:run}}
```

- 数量是 **`Count`**（数组是 `Length`），取元素同样用 `[i]`。
- 常用的还有 `Remove` / `RemoveAt` / `Contains` / `IndexOf` / `Clear` / `Sort`。

> 记法：**方括号 `[]` 一律是数组，花括号 `{}` 是 `List` / `Dictionary`。**

## `Dictionary<K, V>`：按键取值

花括号里写成 `键: 值`，就是字典：

```z42
// examples/basics/collections/dict/dict.z42
{{#include ../../../../examples/basics/collections/dict/dict.z42:code}}
```

```console
{{#include ../../../../examples/basics/collections/dict/run.console:run}}
```

- `d[key]` 既能读也能写；键不存在时写入就是新增。
- 取之前拿不准就先 `ContainsKey`。

空的 `{}` 到底是 `List` 还是 `Dictionary`，由左边的类型决定——所以空字面量必须写明类型。

## 遍历

三者都能直接 `foreach`：

```z42
// examples/basics/collections/iterate/iterate.z42
{{#include ../../../../examples/basics/collections/iterate/iterate.z42:code}}
```

```console
{{#include ../../../../examples/basics/collections/iterate/run.console:run}}
```

遍历 `Dictionary` 时每一轮拿到一个**键值对**，用 `kv.Key` 和 `kv.Value` 取。
⚠️ **字典的遍历顺序不作保证**，别依赖它——上面例子里只有一个键，所以看不出来。

只要键或只要值时，还有 `d.Keys()` / `d.Entries()`，但它们每次调用都会**新建一个数组**；
逐项遍历直接 `foreach` 更省。

## 小结

- **数组 `T[]` 定长**，`Length` 是长度，下标**越界直接终止程序**，`catch` 接不住。
- `[1,2,3]` 列元素、`[0; n]` 重复填充（**`v` 只求值一次**）、`[..a, x, ..b]` 展开拼接。
- **方括号是数组，花括号是 `List` / `Dictionary`**；空字面量必须让编译器知道类型。
- `List<T>` 能增删，数量叫 **`Count`**；`Dictionary<K,V>` 用 `d[key]` 读写。
- 三者都能直接 `foreach`；字典每轮给一个 `kv`，**顺序不保证**。

下一章讲**元组**——一次返回或打包好几个值。
