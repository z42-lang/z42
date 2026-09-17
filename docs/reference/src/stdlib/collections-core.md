# 基础泛型集合（`List` / `Dictionary` / `HashSet`）

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 源码路径 `src/libraries/z42.core/src/Collections/`；命名空间 `Std.Collections`

`List<T>`、`Dictionary<TKey, TValue>`、`HashSet<T>` 是 z42 的三件基础泛型容器，
外加它们的附属类型 `KeyValuePair<TKey, TValue>`、`ReadOnlyCollection<T>` 与两个迭代器
struct。它们**物理上住在 `z42.core` 包**（隐式依赖，工程无需在 `[dependencies]` 里声明），
但命名空间是 `Std.Collections`——用之前仍要写 `using Std.Collections;`。

本页只写 **API 方法面**。语法面在别处：

- 字面量 `{1, 2, 3}` / `{"a": 1}` → [集合字面量](../language/collection-literals.md)
- `foreach` 的路径选择、`Dictionary` 为什么不能直接 `foreach` → [迭代（foreach）](../language/iteration.md)

## `List<T>`

泛型动态数组：摊还 O(1) 尾部追加与随机访问，O(n) 插入 / 删除 / 线性查找。
**对 `T` 没有类型约束**——相等与比较是**运行期**要求，不是编译期约束（见下「相等与比较」）。

```z42
public partial class List<T> {
    public int Count;                             // 字段，不是只读属性

    public List();
    public List(int capacity);                    // capacity < 1 视作 1

    public T this[int i] { get; set; }
    public bool IsEmpty();
    public ListEnumerator<T> GetEnumerator();

    public void Add(T item);
    public void AddRange(T[] items);
    public void Insert(int index, T item);
    public void RemoveAt(int index);
    public bool Remove(T item);
    public void Clear();

    public bool Contains(T item);
    public int  IndexOf(T item);
    public int  LastIndexOf(T item);
    public int  BinarySearch(T item);

    public void Sort();
    public void Sort(Func<T, T, int> comparison);
    public void Reverse();
    public T[]  ToArray();

    // 查询 / 谓词族
    public T       Find(Predicate<T> match);
    public T       FindLast(Predicate<T> match);
    public int     FindIndex(Predicate<T> match);
    public int     FindLastIndex(Predicate<T> match);
    public List<T> FindAll(Predicate<T> match);
    public bool    Exists(Predicate<T> match);
    public bool    TrueForAll(Predicate<T> match);
    public int     RemoveAll(Predicate<T> match);
    public List<T> GetRange(int index, int count);
}
```

| 成员 | 说明 |
|---|---|
| `Count` | 元素个数。**是公开字段**，可读也可写——写它会直接改变列表长度（见下「注意」） |
| `List(int capacity)` | 预分配 `capacity` 个槽；`capacity < 1` 时按 1 处理 |
| `this[int i]` | 读写第 `i` 个元素。**不检查 `Count` 边界**（见下「注意」） |
| `Add` / `AddRange` | 追加单个元素 / 追加整个 `T[]`（按数组顺序） |
| `Insert(index, item)` | 在 `index` 处插入，其后元素右移 |
| `RemoveAt(index)` | 删除 `index` 处元素，其后元素左移 |
| `Remove(item)` | 删除**第一个**等于 `item` 的元素；删掉返回 `true`，找不到返回 `false` |
| `Contains` / `IndexOf` | 线性查找，用元素的 `Equals`；`IndexOf` 找不到返回 `-1` |
| `LastIndexOf` | 从尾部向前找，找不到返回 `-1` |
| `BinarySearch(item)` | **要求列表已按升序排好**。命中返回下标；未命中返回 `-insertionPoint - 1`（等价 C# 的 `~insertionPoint`） |
| `Sort()` | 升序**稳定**排序，O(n log n)，用元素自己的 `CompareTo` |
| `Sort(comparison)` | 同样稳定，但顺序由 `comparison(a, b)` 决定（`< 0` ⇒ `a` 排在 `b` 前） |
| `Reverse()` | 原地反转 |
| `ToArray()` | 返回长度为 `Count` 的**新数组**（快照，改它不影响原列表） |
| `Find` / `FindLast` | 返回首个 / 末个满足谓词的元素；**无匹配返回 `default(T)`**（`int` → `0`，引用类型 → `null`），无法与"匹配到了一个零值元素"区分 |
| `FindIndex` / `FindLastIndex` | 同上但返回下标，无匹配返回 `-1` |
| `FindAll` | 返回满足谓词的元素组成的**新 `List<T>`**，保持相对顺序 |
| `Exists` | 是否存在满足谓词的元素 |
| `TrueForAll` | 是否所有元素都满足谓词；**空列表返回 `true`** |
| `RemoveAll` | 原地删除全部满足谓词的元素（保持剩余元素相对顺序），返回删除个数 |
| `GetRange(index, count)` | `[index, index+count)` 的新 `List<T>`（元素浅拷贝） |

### 注意：索引器不做 `Count` 边界检查

`list[i]` 直接落到内部容量数组上，**只有越过容量才报错**：

```z42
List<int> l = new List<int>(8);   // capacity 8
l.Add(1); l.Add(2);               // Count 2
int a = l[5];                     // → 0，静默返回 default(T)，不报错
int b = l[100];                   // → VM 终止：array index 100 out of bounds (len=8)
```

越界读越过容量时是 **VM abort，不是可 `catch` 的异常**
（`src/runtime/src/interp/exec_array.rs:196`），与 [数组](../language/arrays.md) 的越界行为一致。
调用方自己保证 `0 <= i < Count`。

### 注意：`Count` 是可写字段

`Count` 是 `public int Count;`，不是只读属性。`l.Count = 1;` 会编译通过并把列表长度直接改成 1：

```z42
List<int> l = { 1, 2, 3 };
l.Count = 1;                      // 合法，列表现在只有 1 个元素
```

`Dictionary<K,V>.Count`、`HashSet<T>.Count`、`ReadOnlyCollection<T>.Count` 同样是可写字段。

### 相等与比较

`List<T>` 的类型形参**无约束**，所以「元素能不能比较」在运行期才判定：

- `Contains` / `IndexOf` / `LastIndexOf` / `Remove` 调用元素的 `Equals`；
- `Sort()` / `BinarySearch` 调用元素的 `CompareTo`——元素类型没有 `CompareTo` 时在排序过程中失败：

```
Error: uncaught exception: VCall: function `Pt.CompareTo` not found
  at Std.Collections.List.MergeSort ...
```

要给没有 `CompareTo` 的类型排序，用 `Sort(comparison)` 传比较函数。

### 谓词形参必须标注类型

传给 `Predicate<T>` / `Func<T, T, int>` 形参的 lambda，**必须显式写出参数类型**，
否则参数会以未实例化的 `T` 参与类型检查并报 **E0402**：

```z42
list.Find(v => v % 2 == 0);          // ✗ E0402: operator `%` requires numeric operand, got `T`
list.Find((int v) => v % 2 == 0);    // ✓
list.Sort((int a, int b) => b - a);  // ✓
list.Exists(IsFour);                 // ✓ 命名方法也可以
```

## `Dictionary<TKey, TValue>`

泛型哈希映射，平均摊还 O(1) 读写。

```z42
public class Dictionary<TKey, TValue> where TKey: IEquatable {
    public int Count;

    public Dictionary();

    public TValue this[TKey key] { get; set; }
    public bool IsEmpty();
    public DictionaryEnumerator<TKey, TValue> GetEnumerator();

    public void   Set(TKey key, TValue value);
    public TValue Get(TKey key);
    public bool   TryAdd(TKey key, TValue value);
    public TValue GetValueOrDefault(TKey key);
    public TValue GetValueOrDefault(TKey key, TValue defaultValue);
    public bool   ContainsKey(TKey key);
    public bool   Remove(TKey key);
    public void   Clear();

    public TKey[]   Keys();
    public TValue[] Values();
    public KeyValuePair<TKey, TValue>[] Entries();
}
```

| 成员 | 说明 |
|---|---|
| `this[key]` / `Set` / `Get` | 索引器的 setter 即 `Set`（**覆盖式**写入），getter 即 `Get` |
| `Get(key)` / `this[key]` | **键不存在时静默返回 `default(TValue)`**，不抛异常（见下） |
| `TryAdd(key, value)` | 仅当键不存在时写入。写入了返回 `true`；键已存在则**不覆盖**、返回 `false` |
| `GetValueOrDefault(key)` | 键不存在返回 `default(TValue)` |
| `GetValueOrDefault(key, defaultValue)` | 键不存在返回调用方给的 `defaultValue` |
| `ContainsKey(key)` | 键是否存在 |
| `Remove(key)` | 删掉返回 `true`，键本就不存在返回 `false` |
| `Keys()` / `Values()` / `Entries()` | 返回长度为 `Count` 的**快照数组**。**顺序不保证**，也不保证多次调用之间一致 |

### 注意：取不存在的键不报错

```z42
Dictionary<string, int> d = { "a": 1 };
int x = d["zzz"];            // → 0；d.Count 仍是 1，不会插入
string s = ds["nope"];       // → null（引用类型的 default）
```

`d["zzz"]` 与 `d["a"]`（若值恰好是 `0`）无法区分。要区分「没有这个键」和「值是零值」，
先 `ContainsKey`；要取带回退值的结果，用 `GetValueOrDefault(key, fallback)`。

### 键类型约束

`TKey` 必须满足 `IEquatable`（**非泛型**接口，`bool Equals(Self other)` + `int GetHashCode()`）。
`int` / `long` / `double` / `char` / `string` 等 primitive 自动满足。约束**在编译期检查**：

```
E0402: type argument `Plain` for `TKey` does not satisfy constraint `IEquatable` on `Dictionary`
```

（发射点 `src/compiler/z42c.semantics/src/ConstraintChecker.z42:577`。）
自定义键类型必须写 `Equals(自己的类型 other)` 而不是 `Equals(object other)`，否则报 **E0412**
（发射点 `src/compiler/z42c.semantics/src/InheritanceResolver.z42:501`）：

```z42
class Box : IEquatable {
    public int V;
    public Box(int v) { this.V = v; }
    public bool Equals(Box other) { return other != null && other.V == this.V; }
    public override int GetHashCode() { return this.V; }
}
```

## `HashSet<T>`

泛型哈希集合，平均摊还 O(1) `Add` / `Remove` / `Contains`。元素约束与 `Dictionary` 的 `TKey`
完全一致（`where T: IEquatable`，编译期检查）。

```z42
public class HashSet<T> where T: IEquatable {
    public int Count;

    public HashSet();

    public bool IsEmpty();
    public bool Add(T item);
    public bool Contains(T item);
    public bool Remove(T item);
    public void Clear();
    public T[]  ToArray();

    public void UnionWith(T[] items);
    public void IntersectWith(T[] items);
    public void ExceptWith(T[] items);
}
```

| 成员 | 说明 |
|---|---|
| `Add(item)` | 新元素返回 `true`；**已存在返回 `false` 且不改变集合** |
| `Contains` / `Remove` | 判定 / 删除；`Remove` 删掉返回 `true`，本就不在返回 `false` |
| `ToArray()` | 元素快照数组，**顺序不保证** |
| `UnionWith` / `IntersectWith` / `ExceptWith` | 并 / 交 / 差，原地修改本集合。**三者都只接受 `T[]`**，不接受另一个 `HashSet<T>`——传 `other.ToArray()` |

「相同」由 `GetHashCode()` + `Equals` 决定，是**值相等**而非引用相等：两个字段相同的 `Box`
实例只会存进去一个。

`HashSet<T>` **没有 `GetEnumerator()`，也没有索引器，不能直接 `foreach`**。
`foreach (var x in set)` 目前**能通过编译**，但运行期会以 `ArrayLen: expected array, got Object(...)`
终止——遍历集合请一律先 `ToArray()`：

```z42
foreach (var s in set.ToArray()) { ... }
```

## `KeyValuePair<TKey, TValue>`

不可变键值对，`Dictionary.Entries()` 与 `DictionaryEnumerator.Current` 的元素类型。

```z42
[Record] public struct KeyValuePair<TKey, TValue>(TKey Key, TValue Value);
```

`[Record] struct` ⇒ 值类型、`Key` / `Value` 创建后不可变，值相等 / 哈希 / `ToString` 由编译器合成
（见 [`[Record]` 与主构造器](../language/record-attribute.md)）：

```z42
var kv  = new KeyValuePair<string, int>("k", 9);
var kv2 = new KeyValuePair<string, int>("k", 9);
kv.Equals(kv2);   // true
kv.ToString();    // "KeyValuePair { Key = k, Value = 9 }"
```

## `ReadOnlyCollection<T>`

对一个既有 `T[]` 的**只读视图**。只读性由「类型上没有任何变更成员」保证，不是运行期抛异常。
对 `T` 无约束。

```z42
public sealed class ReadOnlyCollection<T> {
    public int Count;

    public ReadOnlyCollection(T[] array);

    public T this[int index] { get; }      // 只有 getter
    public bool Contains(T value);
    public int  IndexOf(T value);
    public void CopyTo(T[] array, int arrayIndex);
    public T[]  ToArray();
}
```

- **包装的是引用，不是副本**：改动被包装的数组，通过视图能看到新值。要真正的防御性副本用 `ToArray()`。
- 有 `Count` + 只读索引器 ⇒ 直接支持 `foreach`。
- `Contains` / `IndexOf` 用 `Object.Equals` 线性查找；`IndexOf` 未命中返回 `-1`。
- 越界由底层数组负责（VM abort）。
- 也可由 `Array.AsReadOnly<T>(arr)` 构造。

```z42
int[] src = new int[] { 1, 2, 3 };
var ro = new ReadOnlyCollection<int>(src);
src[0] = 99;
ro[0];            // 99 —— 视图跟着底层数组走
```

## 迭代器 struct

两个 `[Record] struct` 迭代器，形状符合 `foreach` 的枚举器路径（`MoveNext` / `Current` / `Dispose`）。
值类型，迭代时不产生堆分配。

```z42
[Record] public struct ListEnumerator<T>(List<T> _list) {
    public bool MoveNext();
    public T Current { get; }
    public void Dispose();
}

[Record] public struct DictionaryEnumerator<TKey, TValue>(Dictionary<TKey, TValue> _dict)
    where TKey: IEquatable {
    public bool MoveNext();
    public KeyValuePair<TKey, TValue> Current { get; }
    public void Dispose();
}
```

`List<T>` 自身的 `foreach` 走索引路径（`Count` + 索引器），不经过 `ListEnumerator<T>`；
该 struct 服务于以接口静态类型 / 泛型约束持有集合的调用点，也可手动驱动：

```z42
var it = list.GetEnumerator();
while (it.MoveNext()) { Console.WriteLine(it.Current); }
```

`DictionaryEnumerator` 需要手动驱动或经泛型约束使用——`Dictionary<K,V>` 同时有 `Count`
字段和索引器，`foreach` 会命中索引路径而不是枚举器路径，详见
[迭代（foreach）](../language/iteration.md)。遍历字典的常规写法是
`foreach (var kv in dict.Entries())`。

## 用法

```z42
using Std.IO;
using Std.Collections;

void Main() {
    List<int> nums = { 5, 3, 9, 1 };
    nums.Sort();
    Console.WriteLine(nums[0]);                       // 1
    Console.WriteLine(nums.BinarySearch(9));          // 3
    Console.WriteLine(nums.FindAll((int v) => v > 3).Count);  // 2

    Dictionary<string, int> scores = { "ann": 90, "bob": 85 };
    if (scores.ContainsKey("ann")) Console.WriteLine(scores["ann"]);
    Console.WriteLine(scores.GetValueOrDefault("zoe", -1));   // -1
    foreach (var kv in scores.Entries()) {
        Console.WriteLine($"{kv.Key}={kv.Value}");
    }

    var seen = new HashSet<string>();
    seen.Add("a");
    Console.WriteLine(seen.Add("a"));                 // false（已存在）
    foreach (var s in seen.ToArray()) { Console.WriteLine(s); }
}
```

## 不支持

- **`Dictionary.TryGetValue(key, out value)`** 没有。用 `ContainsKey` + 索引器，或 `GetValueOrDefault`。
- **`Dictionary.ContainsValue(value)`** 没有。自己遍历 `Values()`。
- **`List<T>.ConvertAll<TOut>` / `List<T>.AsReadOnly()`** 没有。前者用 `foreach` + `Add` 手写；
  后者用 `new ReadOnlyCollection<T>(list.ToArray())`（那是快照，不是活视图）。
- **`List<T>` 没有 `Capacity` 成员**：容量只能在 `List(int capacity)` 构造时给定。
- **`List<T>.Sort(index, count)`、`Insert`/`RemoveRange` 的区间重载** 没有。
- **`HashSet<T>` 的集合运算不接受集合参数**，只接受 `T[]`；也没有 `IsSubsetOf` /
  `IsSupersetOf` / `Overlaps` / `SetEquals` / `SymmetricExceptWith`。
- **`HashSet<T>` 不能 `foreach`**（无 `GetEnumerator()`、无索引器）；写了能编过，运行期崩。用 `ToArray()`。
- **没有自定义比较器**：所有相等判定固定走元素自己的 `Equals` / `GetHashCode`，
  容器构造器不接受 `IEqualityComparer<T>` / `IComparer<T>`。排序是唯一例外
  （`List<T>.Sort(Func<T, T, int>)`）。
- **索引器不做 `Count` 边界检查**（见上）；也没有 `IndexOutOfRangeException` 可接。
- `Grow()`（`List` / `Dictionary`）与 `Dictionary.FindSlot(key)` 虽然是 `public`，
  属于容量 / 槽位管理入口，正常使用不需要调用。

## 相关

- [集合字面量](../language/collection-literals.md)——`{}` 构造 `List` / `Dictionary` 的语法与脱糖规则
- [迭代（foreach）](../language/iteration.md)——三条 `foreach` 路径与 `Dictionary` 的例外
- [数组](../language/arrays.md)——`T[]`、越界行为、`Std.Array` 的静态算法
- [进阶集合](collections.md)——`Stack` / `Queue` / `LinkedList` / `PriorityQueue` / `SortedSet`
