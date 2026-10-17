<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Array

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Array.z42:18`

所有 `T[]` 的运行时基类。元素读写、克隆与静态算法（Sort / IndexOf / Find / ...） 都经此类；用户不能 `new Array()`，也不能继承。

```z42
public sealed class Array {
    int Length
    extern Object Clone()
    static extern Array CreateInstance(Type elementType, int length)
    extern Object GetValue(int index)
    extern void SetValue(Object value, int index)
    static extern void CopyRange(Array source, int sourceIndex, Array destination, int destinationIndex, int length)
    static int IndexOf<T>(T[] array, T value)
    static void Copy<T>(T[] source, T[] destination, int length)
    static void Fill<T>(T[] array, T value)
    static void Reverse<T>(T[] array)
    static void Sort<T>(T[] array) where T : IComparable
    static void Sort<T>(T[] array, Func<T,T,int> comparison)
    static void Sort<TKey,TValue>(TKey[] keys, TValue[] items) where TKey : IComparable
    static T Find<T>(T[] array, Predicate<T> match)
    static T FindLast<T>(T[] array, Predicate<T> match)
    static int FindIndex<T>(T[] array, Predicate<T> match)
    static int FindLastIndex<T>(T[] array, Predicate<T> match)
    static T[] FindAll<T>(T[] array, Predicate<T> match)
    static bool Exists<T>(T[] array, Predicate<T> match)
    static bool TrueForAll<T>(T[] array, Predicate<T> match)
    static int BinarySearch<T>(T[] array, T value) where T : IComparable
    static bool Contains<T>(T[] array, T value)
    static int LastIndexOf<T>(T[] array, T value)
    static U[] ConvertAll<T,U>(T[] array, Func<T,U> converter)
    static void ForEach<T>(T[] array, Action<T> action)
    static void Clear<T>(T[] array, int index, int length)
    static T[] Resize<T>(T[] array, int newSize)
    static T[] Empty<T>()
    static void Sort<T>(T[] array, int index, int length) where T : IComparable
    static void Reverse<T>(T[] array, int index, int length)
    static void Fill<T>(T[] array, T value, int startIndex, int count)
    static void Copy<T>(T[] source, int sourceIndex, T[] destination, int destinationIndex, int length)
    static int IndexOf<T>(T[] array, T value, int startIndex)
    static int IndexOf<T>(T[] array, T value, int startIndex, int count)
    static int LastIndexOf<T>(T[] array, T value, int startIndex)
    static int BinarySearch<T>(T[] array, int index, int length, T value) where T : IComparable
    static int BinarySearch<T>(T[] array, T value, Func<T,T,int> comparison)
    static ReadOnlyCollection<T> AsReadOnly<T>(T[] array)
}
```

### int Length

`src/libraries/z42.core/src/Array.z42:20`

元素个数。实际值由 IR `array_len` 直读，此字段仅为类型检查占位。

### extern Object Clone()

`src/libraries/z42.core/src/Array.z42:24`

浅拷贝。元素是引用类型时共享引用（同 C# `System.Array.Clone()`）。

### static extern Array CreateInstance(Type elementType, int length)

`src/libraries/z42.core/src/Array.z42:32`

以元素类型 + 长度建一维数组（元素为该类型默认值）。

### extern Object GetValue(int index)

`src/libraries/z42.core/src/Array.z42:36`

读元素 `index`（基元装箱为对应包装类，引用元素原样）。

### extern void SetValue(Object value, int index)

`src/libraries/z42.core/src/Array.z42:40`

写 `value` 到元素 `index`（`value` 在前，与 C# 原生签名一致）。

### static extern void CopyRange(Array source, int sourceIndex, Array destination, int destinationIndex, int length)

`src/libraries/z42.core/src/Array.z42:46`

区间批量拷贝原语：`destination[destinationIndex..] = source[sourceIndex..]`（`length` 个）。

### static int IndexOf<T>(T[] array, T value)

`src/libraries/z42.core/src/Array.z42:57`

`value` 首次出现的索引（按 `.Equals`），未命中 -1。

### static void Copy<T>(T[] source, T[] destination, int length)

`src/libraries/z42.core/src/Array.z42:66`

把 `source` 前 `length` 个元素拷入 `destination`（都从 0 起）。越界抛异常。

### static void Fill<T>(T[] array, T value)

`src/libraries/z42.core/src/Array.z42:74`

把每个元素都设为 `value`。

### static void Reverse<T>(T[] array)

`src/libraries/z42.core/src/Array.z42:80`

原地反转元素顺序。

### static void Sort<T>(T[] array) where T : IComparable

`src/libraries/z42.core/src/Array.z42:93`

按元素自身 `CompareTo` 稳定升序排序（自顶向下归并，O(n log n)）。

### static void Sort<T>(T[] array, Func<T,T,int> comparison)

`src/libraries/z42.core/src/Array.z42:121`

按调用方提供的比较器（< 0 ⇒ a 在 b 前）稳定升序排序。

### static void Sort<TKey,TValue>(TKey[] keys, TValue[] items) where TKey : IComparable

`src/libraries/z42.core/src/Array.z42:129`

成对排序：`keys` 按 `CompareTo` 升序，`items` 同步应用同一置换。

### static T Find<T>(T[] array, Predicate<T> match)

`src/libraries/z42.core/src/Array.z42:179`

第一个命中 `match` 的元素，未命中返回 default(T)。

### static T FindLast<T>(T[] array, Predicate<T> match)

`src/libraries/z42.core/src/Array.z42:188`

最后一个命中 `match` 的元素，未命中返回 default(T)。

### static int FindIndex<T>(T[] array, Predicate<T> match)

`src/libraries/z42.core/src/Array.z42:198`

第一个命中 `match` 的元素下标，未命中 -1。

### static int FindLastIndex<T>(T[] array, Predicate<T> match)

`src/libraries/z42.core/src/Array.z42:207`

最后一个命中 `match` 的元素下标，未命中 -1。

### static T[] FindAll<T>(T[] array, Predicate<T> match)

`src/libraries/z42.core/src/Array.z42:217`

所有命中 `match` 的元素（按序，新数组，尺寸恰好）。

### static bool Exists<T>(T[] array, Predicate<T> match)

`src/libraries/z42.core/src/Array.z42:230`

是否存在任一元素命中 `match`。

### static bool TrueForAll<T>(T[] array, Predicate<T> match)

`src/libraries/z42.core/src/Array.z42:239`

是否每个元素都命中 `match`（空数组恒 true）。

### static int BinarySearch<T>(T[] array, T value) where T : IComparable

`src/libraries/z42.core/src/Array.z42:249`

在按 `CompareTo` 升序的数组中二分查找。命中返回下标；否则返回 `~insertionPoint` （其按位取反即保持有序的插入位置）。

### static bool Contains<T>(T[] array, T value)

`src/libraries/z42.core/src/Array.z42:263`

`value` 是否出现在 `array` 中（按 `.Equals`）。

### static int LastIndexOf<T>(T[] array, T value)

`src/libraries/z42.core/src/Array.z42:268`

`value` 最后一次出现的下标（按 `.Equals`），未命中 -1。

### static U[] ConvertAll<T,U>(T[] array, Func<T,U> converter)

`src/libraries/z42.core/src/Array.z42:278`

把每个元素经 `converter` 投影到等长新数组。

### static void ForEach<T>(T[] array, Action<T> action)

`src/libraries/z42.core/src/Array.z42:286`

对每个元素按序执行 `action`。

### static void Clear<T>(T[] array, int index, int length)

`src/libraries/z42.core/src/Array.z42:292`

把 `[index, index+length)` 置为 default(T)。越界抛异常。

### static T[] Resize<T>(T[] array, int newSize)

`src/libraries/z42.core/src/Array.z42:301`

返回长度为 `newSize` 的新数组：前缀拷自 `array`，尾部填 default(T)；`newSize` 更小时 截断。原数组不变（C# 用 `ref T[]`，这里把结果赋回）。

### static T[] Empty<T>()

`src/libraries/z42.core/src/Array.z42:313`

一个空（零长）的 `T[]`。

### static void Sort<T>(T[] array, int index, int length) where T : IComparable

`src/libraries/z42.core/src/Array.z42:327`

按 `CompareTo` 稳定升序排序子区间 `[index, index+length)`。

### static void Reverse<T>(T[] array, int index, int length)

`src/libraries/z42.core/src/Array.z42:335`

原地反转子区间 `[index, index+length)`。

### static void Fill<T>(T[] array, T value, int startIndex, int count)

`src/libraries/z42.core/src/Array.z42:349`

把 `[startIndex, startIndex+count)` 置为 `value`。

### static void Copy<T>(T[] source, int sourceIndex, T[] destination, int destinationIndex, int length)

`src/libraries/z42.core/src/Array.z42:357`

从 `source[sourceIndex..]` 拷 `length` 个元素到 `destination[destinationIndex..]`。 同一数组内重叠区间按 memmove 语义（向高索引移动时反向拷贝）。

### static int IndexOf<T>(T[] array, T value, int startIndex)

`src/libraries/z42.core/src/Array.z42:367`

`value` 在 `startIndex` 及其后首次出现的下标（按 `.Equals`），未命中 -1。

### static int IndexOf<T>(T[] array, T value, int startIndex, int count)

`src/libraries/z42.core/src/Array.z42:376`

`value` 在 `[startIndex, startIndex+count)` 内首次出现的下标（按 `.Equals`），未命中 -1。

### static int LastIndexOf<T>(T[] array, T value, int startIndex)

`src/libraries/z42.core/src/Array.z42:386`

从 `startIndex` 起向前找 `value` 最后一次出现的下标（按 `.Equals`），未命中 -1。

### static int BinarySearch<T>(T[] array, int index, int length, T value) where T : IComparable

`src/libraries/z42.core/src/Array.z42:397`

在有序子区间 `[index, index+length)` 内按 `CompareTo` 二分查找 `value`。 命中返回下标，否则返回 `~insertionPoint`。

### static int BinarySearch<T>(T[] array, T value, Func<T,T,int> comparison)

`src/libraries/z42.core/src/Array.z42:412`

在调用方比较器（`comparison(element, value)` < 0 ⇒ element 在 value 前）下对全数组 二分查找。命中返回下标，否则返回 `~insertionPoint`。

### static ReadOnlyCollection<T> AsReadOnly<T>(T[] array)

`src/libraries/z42.core/src/Array.z42:426`

把 `array` 包装成只读视图（不复制，按引用包装；视图不提供任何变更操作）。

