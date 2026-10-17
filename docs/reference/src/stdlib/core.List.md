<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.List

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Collections`

`src/libraries/z42.core/src/Collections/List.Query.z42:8`

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

### T Find(Predicate<T> match)

`src/libraries/z42.core/src/Collections/List.Query.z42:11`

返回第一个满足 `match` 的元素；无匹配返回 default(T)。

### T FindLast(Predicate<T> match)

`src/libraries/z42.core/src/Collections/List.Query.z42:18`

返回最后一个满足 `match` 的元素；无匹配返回 default(T)。

### int FindIndex(Predicate<T> match)

`src/libraries/z42.core/src/Collections/List.Query.z42:25`

返回第一个满足 `match` 的元素下标；无匹配返回 -1。

### int FindLastIndex(Predicate<T> match)

`src/libraries/z42.core/src/Collections/List.Query.z42:35`

返回最后一个满足 `match` 的元素下标；无匹配返回 -1。

### List<T> FindAll(Predicate<T> match)

`src/libraries/z42.core/src/Collections/List.Query.z42:45`

返回所有满足 `match` 的元素组成的新 List（保持相对顺序）。

### bool Exists(Predicate<T> match)

`src/libraries/z42.core/src/Collections/List.Query.z42:56`

是否存在满足 `match` 的元素。

### bool TrueForAll(Predicate<T> match)

`src/libraries/z42.core/src/Collections/List.Query.z42:59`

是否所有元素都满足 `match`（空列表返回 true，与 C# 一致）。

### int RemoveAll(Predicate<T> match)

`src/libraries/z42.core/src/Collections/List.Query.z42:69`

原地移除所有满足 `match` 的元素（稳定压缩），返回移除个数。

### int LastIndexOf(T item)

`src/libraries/z42.core/src/Collections/List.Query.z42:85`

返回最后一个等于 `item` 的元素下标（用 T 的相等判定）；无则 -1。

### List<T> GetRange(int index, int count)

`src/libraries/z42.core/src/Collections/List.Query.z42:95`

返回 [index, index+count) 子区间的新 List（浅拷贝，保持顺序）。

### int BinarySearch(T item) where T : IComparable

`src/libraries/z42.core/src/Collections/List.Query.z42:108`

在**已升序排序**的列表上二分查找 `item`（用 T 的 CompareTo）。 命中返回其下标；未命中返回负数：`-insertionPoint - 1`（等价 C# 的 `~insertionPoint`，insertionPoint = 第一个大于 item 的位置）。

### int Count

`src/libraries/z42.core/src/Collections/List.z42:15`

元素个数（公开字段，可读可写）。

### List()

`src/libraries/z42.core/src/Collections/List.z42:19`

新建空列表。

### List(int capacity)

`src/libraries/z42.core/src/Collections/List.z42:26`

按预期元素数预分配容量，避免后续反复 2× Grow 再分配。`capacity < 1` 视作 1。

### bool IsEmpty()

`src/libraries/z42.core/src/Collections/List.z42:35`

是否为空。

### ListEnumerator<T> GetEnumerator()

`src/libraries/z42.core/src/Collections/List.z42:40`

返回**具体** `ListEnumerator<T>` struct（非 `IEnumerator<T>` 接口）。List 自身 foreach 走索引快路径（Count + get_Item），不经此；本方法服务以接口静态类型 / 泛型约束持有的调用点。

### T this[int i] { get; set; }

`src/libraries/z42.core/src/Collections/List.z42:43`

读写第 `i` 个元素。

### void Add(T item)

`src/libraries/z42.core/src/Collections/List.z42:49`

追加元素。

### void AddRange(T[] items)

`src/libraries/z42.core/src/Collections/List.z42:57`

按数组顺序追加整个 `T[]`。

### T[] ToArray()

`src/libraries/z42.core/src/Collections/List.z42:80`

返回长度为 Count 的新数组快照（改它不影响原列表）。

### void Insert(int index, T item)

`src/libraries/z42.core/src/Collections/List.z42:87`

在 `index` 处插入，其后元素右移。

### void RemoveAt(int index)

`src/libraries/z42.core/src/Collections/List.z42:95`

删除 `index` 处元素，其后元素左移。

### bool Remove(T item)

`src/libraries/z42.core/src/Collections/List.z42:102`

删除第一个等于 `item` 的元素；删掉返回 true，找不到返回 false。

### bool Contains(T item)

`src/libraries/z42.core/src/Collections/List.z42:110`

是否含 `item`（线性查找）。

### int IndexOf(T item)

`src/libraries/z42.core/src/Collections/List.z42:113`

首个等于 `item` 的下标，未命中 -1。

### void Clear()

`src/libraries/z42.core/src/Collections/List.z42:125`

清空列表（Count 归零）。

### void Sort() where T : IComparable

`src/libraries/z42.core/src/Collections/List.z42:132`

升序稳定排序；基元元素走原生稳定排序。

### void Sort(Func<T,T,int> comparison)

`src/libraries/z42.core/src/Collections/List.z42:167`

用调用方给定的比较函数排序：`comparison(a, b) < 0` ⇒ a 排在 b 前。同样稳定。

### void Reverse()

`src/libraries/z42.core/src/Collections/List.z42:199`

原地反转。

### void Grow()

`src/libraries/z42.core/src/Collections/List.z42:212`

容量翻倍。容量管理入口，正常使用无需调用。

