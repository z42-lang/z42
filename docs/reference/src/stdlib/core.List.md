<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.List

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Collections`

[`src/libraries/z42.core/src/Collections/List.Query.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.Query.z42#L8)

`List<T>` 的查询 / 谓词族（partial 第二部分）：Find* / Exists / TrueForAll / RemoveAll / FindAll / GetRange / LastIndexOf / BinarySearch。

```z42
public partial class List<T> {
    T Find(Predicate<T> match)
    T FindLast(Predicate<T> match)
    int FindIndex(Predicate<T> match)
    int FindLastIndex(Predicate<T> match)
    List<T> FindAll(Predicate<T> match)
    bool Exists(Predicate<T> match)
    bool TrueForAll(Predicate<T> match)
    int RemoveAll(Predicate<T> match)
    int LastIndexOf(T item)
    List<T> GetRange(int index, int count)
    int BinarySearch(T item) where T : IComparable
    int Count
    List()
    List(int capacity)
    bool IsEmpty()
    ListEnumerator<T> GetEnumerator()
    T this[int i] { get; set; }
    void Add(T item)
    void AddRange(T[] items)
    T[] ToArray()
    void Insert(int index, T item)
    void RemoveAt(int index)
    bool Remove(T item)
    bool Contains(T item)
    int IndexOf(T item)
    void Clear()
    void Sort() where T : IComparable
    void Sort(Func<T,T,int> comparison)
    void Reverse()
    void Grow()
}
```

成员：[`Find`](#Std.Collections.List%3CT%3E.Find(Predicate%3CT%3E)) · [`FindLast`](#Std.Collections.List%3CT%3E.FindLast(Predicate%3CT%3E)) · [`FindIndex`](#Std.Collections.List%3CT%3E.FindIndex(Predicate%3CT%3E)) · [`FindLastIndex`](#Std.Collections.List%3CT%3E.FindLastIndex(Predicate%3CT%3E)) · [`FindAll`](#Std.Collections.List%3CT%3E.FindAll(Predicate%3CT%3E)) · [`Exists`](#Std.Collections.List%3CT%3E.Exists(Predicate%3CT%3E)) · [`TrueForAll`](#Std.Collections.List%3CT%3E.TrueForAll(Predicate%3CT%3E)) · [`RemoveAll`](#Std.Collections.List%3CT%3E.RemoveAll(Predicate%3CT%3E)) · [`LastIndexOf`](#Std.Collections.List%3CT%3E.LastIndexOf(T)) · [`GetRange`](#Std.Collections.List%3CT%3E.GetRange(int,int)) · [`BinarySearch`](#Std.Collections.List%3CT%3E.BinarySearch(T)) · [`Count`](#Std.Collections.List%3CT%3E.Count) · [`List`](#Std.Collections.List%3CT%3E.ctor()) · [`List`](#Std.Collections.List%3CT%3E.ctor(int)) · [`IsEmpty`](#Std.Collections.List%3CT%3E.IsEmpty()) · [`GetEnumerator`](#Std.Collections.List%3CT%3E.GetEnumerator()) · [`this`](#Std.Collections.List%3CT%3E.this[int]) · [`Add`](#Std.Collections.List%3CT%3E.Add(T)) · [`AddRange`](#Std.Collections.List%3CT%3E.AddRange(T[])) · [`ToArray`](#Std.Collections.List%3CT%3E.ToArray()) · [`Insert`](#Std.Collections.List%3CT%3E.Insert(int,T)) · [`RemoveAt`](#Std.Collections.List%3CT%3E.RemoveAt(int)) · [`Remove`](#Std.Collections.List%3CT%3E.Remove(T)) · [`Contains`](#Std.Collections.List%3CT%3E.Contains(T)) · [`IndexOf`](#Std.Collections.List%3CT%3E.IndexOf(T)) · [`Clear`](#Std.Collections.List%3CT%3E.Clear()) · [`Sort`](#Std.Collections.List%3CT%3E.Sort()) · [`Sort`](#Std.Collections.List%3CT%3E.Sort(Func%3CT,T,int%3E)) · [`Reverse`](#Std.Collections.List%3CT%3E.Reverse()) · [`Grow`](#Std.Collections.List%3CT%3E.Grow())

<a id="Std.Collections.List<T>.Find(Predicate<T>)"></a>
### T Find(Predicate<T> match)

[`src/libraries/z42.core/src/Collections/List.Query.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.Query.z42#L11)

返回第一个满足 `match` 的元素；无匹配返回 default(T)。

<a id="Std.Collections.List<T>.FindLast(Predicate<T>)"></a>
### T FindLast(Predicate<T> match)

[`src/libraries/z42.core/src/Collections/List.Query.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.Query.z42#L18)

返回最后一个满足 `match` 的元素；无匹配返回 default(T)。

<a id="Std.Collections.List<T>.FindIndex(Predicate<T>)"></a>
### int FindIndex(Predicate<T> match)

[`src/libraries/z42.core/src/Collections/List.Query.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.Query.z42#L25)

返回第一个满足 `match` 的元素下标；无匹配返回 -1。

<a id="Std.Collections.List<T>.FindLastIndex(Predicate<T>)"></a>
### int FindLastIndex(Predicate<T> match)

[`src/libraries/z42.core/src/Collections/List.Query.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.Query.z42#L35)

返回最后一个满足 `match` 的元素下标；无匹配返回 -1。

<a id="Std.Collections.List<T>.FindAll(Predicate<T>)"></a>
### List<T> FindAll(Predicate<T> match)

[`src/libraries/z42.core/src/Collections/List.Query.z42:45`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.Query.z42#L45)

返回所有满足 `match` 的元素组成的新 List（保持相对顺序）。

<a id="Std.Collections.List<T>.Exists(Predicate<T>)"></a>
### bool Exists(Predicate<T> match)

[`src/libraries/z42.core/src/Collections/List.Query.z42:56`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.Query.z42#L56)

是否存在满足 `match` 的元素。

<a id="Std.Collections.List<T>.TrueForAll(Predicate<T>)"></a>
### bool TrueForAll(Predicate<T> match)

[`src/libraries/z42.core/src/Collections/List.Query.z42:59`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.Query.z42#L59)

是否所有元素都满足 `match`（空列表返回 true，与 C# 一致）。

<a id="Std.Collections.List<T>.RemoveAll(Predicate<T>)"></a>
### int RemoveAll(Predicate<T> match)

[`src/libraries/z42.core/src/Collections/List.Query.z42:69`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.Query.z42#L69)

原地移除所有满足 `match` 的元素（稳定压缩），返回移除个数。

<a id="Std.Collections.List<T>.LastIndexOf(T)"></a>
### int LastIndexOf(T item)

[`src/libraries/z42.core/src/Collections/List.Query.z42:85`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.Query.z42#L85)

返回最后一个等于 `item` 的元素下标（用 T 的相等判定）；无则 -1。

<a id="Std.Collections.List<T>.GetRange(int,int)"></a>
### List<T> GetRange(int index, int count)

[`src/libraries/z42.core/src/Collections/List.Query.z42:95`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.Query.z42#L95)

返回 [index, index+count) 子区间的新 List（浅拷贝，保持顺序）。

<a id="Std.Collections.List<T>.BinarySearch(T)"></a>
### int BinarySearch(T item) where T : IComparable

[`src/libraries/z42.core/src/Collections/List.Query.z42:108`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.Query.z42#L108)

在**已升序排序**的列表上二分查找 `item`（用 T 的 CompareTo）。 命中返回其下标；未命中返回负数：`-insertionPoint - 1`（等价 C# 的 `~insertionPoint`，insertionPoint = 第一个大于 item 的位置）。

<a id="Std.Collections.List<T>.Count"></a>
### int Count

[`src/libraries/z42.core/src/Collections/List.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.z42#L15)

元素个数（公开字段，可读可写）。

<a id="Std.Collections.List<T>.ctor()"></a>
### List()

[`src/libraries/z42.core/src/Collections/List.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.z42#L19)

新建空列表。

<a id="Std.Collections.List<T>.ctor(int)"></a>
### List(int capacity)

[`src/libraries/z42.core/src/Collections/List.z42:26`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.z42#L26)

按预期元素数预分配容量，避免后续反复 2× Grow 再分配。`capacity < 1` 视作 1。

<a id="Std.Collections.List<T>.IsEmpty()"></a>
### bool IsEmpty()

[`src/libraries/z42.core/src/Collections/List.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.z42#L35)

是否为空。

<a id="Std.Collections.List<T>.GetEnumerator()"></a>
### ListEnumerator<T> GetEnumerator()

[`src/libraries/z42.core/src/Collections/List.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.z42#L40)

返回**具体** `ListEnumerator<T>` struct（非 `IEnumerator<T>` 接口）。List 自身 foreach 走索引快路径（Count + get_Item），不经此；本方法服务以接口静态类型 / 泛型约束持有的调用点。

<a id="Std.Collections.List<T>.this[int]"></a>
### T this[int i] { get; set; }

[`src/libraries/z42.core/src/Collections/List.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.z42#L43)

读写第 `i` 个元素。

<a id="Std.Collections.List<T>.Add(T)"></a>
### void Add(T item)

[`src/libraries/z42.core/src/Collections/List.z42:49`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.z42#L49)

追加元素。

<a id="Std.Collections.List<T>.AddRange(T[])"></a>
### void AddRange(T[] items)

[`src/libraries/z42.core/src/Collections/List.z42:57`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.z42#L57)

按数组顺序追加整个 `T[]`。

<a id="Std.Collections.List<T>.ToArray()"></a>
### T[] ToArray()

[`src/libraries/z42.core/src/Collections/List.z42:80`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.z42#L80)

返回长度为 Count 的新数组快照（改它不影响原列表）。

<a id="Std.Collections.List<T>.Insert(int,T)"></a>
### void Insert(int index, T item)

[`src/libraries/z42.core/src/Collections/List.z42:87`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.z42#L87)

在 `index` 处插入，其后元素右移。

<a id="Std.Collections.List<T>.RemoveAt(int)"></a>
### void RemoveAt(int index)

[`src/libraries/z42.core/src/Collections/List.z42:95`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.z42#L95)

删除 `index` 处元素，其后元素左移。

<a id="Std.Collections.List<T>.Remove(T)"></a>
### bool Remove(T item)

[`src/libraries/z42.core/src/Collections/List.z42:102`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.z42#L102)

删除第一个等于 `item` 的元素；删掉返回 true，找不到返回 false。

<a id="Std.Collections.List<T>.Contains(T)"></a>
### bool Contains(T item)

[`src/libraries/z42.core/src/Collections/List.z42:110`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.z42#L110)

是否含 `item`（线性查找）。

<a id="Std.Collections.List<T>.IndexOf(T)"></a>
### int IndexOf(T item)

[`src/libraries/z42.core/src/Collections/List.z42:113`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.z42#L113)

首个等于 `item` 的下标，未命中 -1。

<a id="Std.Collections.List<T>.Clear()"></a>
### void Clear()

[`src/libraries/z42.core/src/Collections/List.z42:125`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.z42#L125)

清空列表（Count 归零）。

<a id="Std.Collections.List<T>.Sort()"></a>
### void Sort() where T : IComparable

[`src/libraries/z42.core/src/Collections/List.z42:132`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.z42#L132)

升序稳定排序；基元元素走原生稳定排序。

<a id="Std.Collections.List<T>.Sort(Func<T,T,int>)"></a>
### void Sort(Func<T,T,int> comparison)

[`src/libraries/z42.core/src/Collections/List.z42:167`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.z42#L167)

用调用方给定的比较函数排序：`comparison(a, b) < 0` ⇒ a 排在 b 前。同样稳定。

<a id="Std.Collections.List<T>.Reverse()"></a>
### void Reverse()

[`src/libraries/z42.core/src/Collections/List.z42:199`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.z42#L199)

原地反转。

<a id="Std.Collections.List<T>.Grow()"></a>
### void Grow()

[`src/libraries/z42.core/src/Collections/List.z42:212`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/List.z42#L212)

容量翻倍。容量管理入口，正常使用无需调用。

