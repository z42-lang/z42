<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Array

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Array.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L20)

所有 `T[]` 的运行时基类。

元素读写、克隆与静态算法（Sort / IndexOf / Find / ...）都经此类；用户不能
`new Array()`，也不能继承。

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

成员：[`Length`](#Std.Array.Length) · [`Clone`](#Std.Array.Clone()) · [`CreateInstance`](#Std.Array.CreateInstance(Type,int)) · [`GetValue`](#Std.Array.GetValue(int)) · [`SetValue`](#Std.Array.SetValue(Object,int)) · [`CopyRange`](#Std.Array.CopyRange(Array,int,Array,int,int)) · [`IndexOf`](#Std.Array.IndexOf%3CT%3E(T[],T)) · [`Copy`](#Std.Array.Copy%3CT%3E(T[],T[],int)) · [`Fill`](#Std.Array.Fill%3CT%3E(T[],T)) · [`Reverse`](#Std.Array.Reverse%3CT%3E(T[])) · [`Sort`](#Std.Array.Sort%3CT%3E(T[])) · [`Sort`](#Std.Array.Sort%3CT%3E(T[],Func%3CT,T,int%3E)) · [`Sort`](#Std.Array.Sort%3CTKey,TValue%3E(TKey[],TValue[])) · [`Find`](#Std.Array.Find%3CT%3E(T[],Predicate%3CT%3E)) · [`FindLast`](#Std.Array.FindLast%3CT%3E(T[],Predicate%3CT%3E)) · [`FindIndex`](#Std.Array.FindIndex%3CT%3E(T[],Predicate%3CT%3E)) · [`FindLastIndex`](#Std.Array.FindLastIndex%3CT%3E(T[],Predicate%3CT%3E)) · [`FindAll`](#Std.Array.FindAll%3CT%3E(T[],Predicate%3CT%3E)) · [`Exists`](#Std.Array.Exists%3CT%3E(T[],Predicate%3CT%3E)) · [`TrueForAll`](#Std.Array.TrueForAll%3CT%3E(T[],Predicate%3CT%3E)) · [`BinarySearch`](#Std.Array.BinarySearch%3CT%3E(T[],T)) · [`Contains`](#Std.Array.Contains%3CT%3E(T[],T)) · [`LastIndexOf`](#Std.Array.LastIndexOf%3CT%3E(T[],T)) · [`ConvertAll`](#Std.Array.ConvertAll%3CT,U%3E(T[],Func%3CT,U%3E)) · [`ForEach`](#Std.Array.ForEach%3CT%3E(T[],Action%3CT%3E)) · [`Clear`](#Std.Array.Clear%3CT%3E(T[],int,int)) · [`Resize`](#Std.Array.Resize%3CT%3E(T[],int)) · [`Empty`](#Std.Array.Empty%3CT%3E()) · [`Sort`](#Std.Array.Sort%3CT%3E(T[],int,int)) · [`Reverse`](#Std.Array.Reverse%3CT%3E(T[],int,int)) · [`Fill`](#Std.Array.Fill%3CT%3E(T[],T,int,int)) · [`Copy`](#Std.Array.Copy%3CT%3E(T[],int,T[],int,int)) · [`IndexOf`](#Std.Array.IndexOf%3CT%3E(T[],T,int)) · [`IndexOf`](#Std.Array.IndexOf%3CT%3E(T[],T,int,int)) · [`LastIndexOf`](#Std.Array.LastIndexOf%3CT%3E(T[],T,int)) · [`BinarySearch`](#Std.Array.BinarySearch%3CT%3E(T[],int,int,T)) · [`BinarySearch`](#Std.Array.BinarySearch%3CT%3E(T[],T,Func%3CT,T,int%3E)) · [`AsReadOnly`](#Std.Array.AsReadOnly%3CT%3E(T[]))

<a id="Std.Array.Length"></a>
### int Length

[`src/libraries/z42.core/src/Array.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L22)

元素个数。实际值由 IR `array_len` 直读，此字段仅为类型检查占位。

<a id="Std.Array.Clone()"></a>
### extern Object Clone()

[`src/libraries/z42.core/src/Array.z42:26`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L26)

浅拷贝。元素是引用类型时共享引用（同 C# `System.Array.Clone()`）。

<a id="Std.Array.CreateInstance(Type,int)"></a>
### static extern Array CreateInstance(Type elementType, int length)

[`src/libraries/z42.core/src/Array.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L34)

以元素类型 + 长度建一维数组（元素为该类型默认值）。

<a id="Std.Array.GetValue(int)"></a>
### extern Object GetValue(int index)

[`src/libraries/z42.core/src/Array.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L38)

读元素 `index`（基元装箱为对应包装类，引用元素原样）。

<a id="Std.Array.SetValue(Object,int)"></a>
### extern void SetValue(Object value, int index)

[`src/libraries/z42.core/src/Array.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L42)

写 `value` 到元素 `index`（`value` 在前，与 C# 原生签名一致）。

<a id="Std.Array.CopyRange(Array,int,Array,int,int)"></a>
### static extern void CopyRange(Array source, int sourceIndex, Array destination, int destinationIndex, int length)

[`src/libraries/z42.core/src/Array.z42:48`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L48)

区间批量拷贝原语：`destination[destinationIndex..] = source[sourceIndex..]`（`length` 个）。

<a id="Std.Array.IndexOf<T>(T[],T)"></a>
### static int IndexOf<T>(T[] array, T value)

[`src/libraries/z42.core/src/Array.z42:59`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L59)

`value` 首次出现的索引（按 `.Equals`），未命中 -1。

<a id="Std.Array.Copy<T>(T[],T[],int)"></a>
### static void Copy<T>(T[] source, T[] destination, int length)

[`src/libraries/z42.core/src/Array.z42:68`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L68)

把 `source` 前 `length` 个元素拷入 `destination`（都从 0 起）。越界抛异常。

<a id="Std.Array.Fill<T>(T[],T)"></a>
### static void Fill<T>(T[] array, T value)

[`src/libraries/z42.core/src/Array.z42:76`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L76)

把每个元素都设为 `value`。

<a id="Std.Array.Reverse<T>(T[])"></a>
### static void Reverse<T>(T[] array)

[`src/libraries/z42.core/src/Array.z42:82`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L82)

原地反转元素顺序。

<a id="Std.Array.Sort<T>(T[])"></a>
### static void Sort<T>(T[] array) where T : IComparable

[`src/libraries/z42.core/src/Array.z42:95`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L95)

按元素自身 `CompareTo` 稳定升序排序（自顶向下归并，O(n log n)）。

<a id="Std.Array.Sort<T>(T[],Func<T,T,int>)"></a>
### static void Sort<T>(T[] array, Func<T,T,int> comparison)

[`src/libraries/z42.core/src/Array.z42:123`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L123)

按调用方提供的比较器（< 0 ⇒ a 在 b 前）稳定升序排序。

<a id="Std.Array.Sort<TKey,TValue>(TKey[],TValue[])"></a>
### static void Sort<TKey,TValue>(TKey[] keys, TValue[] items) where TKey : IComparable

[`src/libraries/z42.core/src/Array.z42:131`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L131)

成对排序：`keys` 按 `CompareTo` 升序，`items` 同步应用同一置换。

<a id="Std.Array.Find<T>(T[],Predicate<T>)"></a>
### static T Find<T>(T[] array, Predicate<T> match)

[`src/libraries/z42.core/src/Array.z42:181`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L181)

第一个命中 `match` 的元素，未命中返回 default(T)。

<a id="Std.Array.FindLast<T>(T[],Predicate<T>)"></a>
### static T FindLast<T>(T[] array, Predicate<T> match)

[`src/libraries/z42.core/src/Array.z42:190`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L190)

最后一个命中 `match` 的元素，未命中返回 default(T)。

<a id="Std.Array.FindIndex<T>(T[],Predicate<T>)"></a>
### static int FindIndex<T>(T[] array, Predicate<T> match)

[`src/libraries/z42.core/src/Array.z42:200`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L200)

第一个命中 `match` 的元素下标，未命中 -1。

<a id="Std.Array.FindLastIndex<T>(T[],Predicate<T>)"></a>
### static int FindLastIndex<T>(T[] array, Predicate<T> match)

[`src/libraries/z42.core/src/Array.z42:209`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L209)

最后一个命中 `match` 的元素下标，未命中 -1。

<a id="Std.Array.FindAll<T>(T[],Predicate<T>)"></a>
### static T[] FindAll<T>(T[] array, Predicate<T> match)

[`src/libraries/z42.core/src/Array.z42:219`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L219)

所有命中 `match` 的元素（按序，新数组，尺寸恰好）。

<a id="Std.Array.Exists<T>(T[],Predicate<T>)"></a>
### static bool Exists<T>(T[] array, Predicate<T> match)

[`src/libraries/z42.core/src/Array.z42:232`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L232)

是否存在任一元素命中 `match`。

<a id="Std.Array.TrueForAll<T>(T[],Predicate<T>)"></a>
### static bool TrueForAll<T>(T[] array, Predicate<T> match)

[`src/libraries/z42.core/src/Array.z42:241`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L241)

是否每个元素都命中 `match`（空数组恒 true）。

<a id="Std.Array.BinarySearch<T>(T[],T)"></a>
### static int BinarySearch<T>(T[] array, T value) where T : IComparable

[`src/libraries/z42.core/src/Array.z42:251`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L251)

在按 `CompareTo` 升序的数组中二分查找。命中返回下标；否则返回 `~insertionPoint` （其按位取反即保持有序的插入位置）。

<a id="Std.Array.Contains<T>(T[],T)"></a>
### static bool Contains<T>(T[] array, T value)

[`src/libraries/z42.core/src/Array.z42:265`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L265)

`value` 是否出现在 `array` 中（按 `.Equals`）。

<a id="Std.Array.LastIndexOf<T>(T[],T)"></a>
### static int LastIndexOf<T>(T[] array, T value)

[`src/libraries/z42.core/src/Array.z42:270`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L270)

`value` 最后一次出现的下标（按 `.Equals`），未命中 -1。

<a id="Std.Array.ConvertAll<T,U>(T[],Func<T,U>)"></a>
### static U[] ConvertAll<T,U>(T[] array, Func<T,U> converter)

[`src/libraries/z42.core/src/Array.z42:280`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L280)

把每个元素经 `converter` 投影到等长新数组。

<a id="Std.Array.ForEach<T>(T[],Action<T>)"></a>
### static void ForEach<T>(T[] array, Action<T> action)

[`src/libraries/z42.core/src/Array.z42:288`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L288)

对每个元素按序执行 `action`。

<a id="Std.Array.Clear<T>(T[],int,int)"></a>
### static void Clear<T>(T[] array, int index, int length)

[`src/libraries/z42.core/src/Array.z42:294`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L294)

把 `[index, index+length)` 置为 default(T)。越界抛异常。

<a id="Std.Array.Resize<T>(T[],int)"></a>
### static T[] Resize<T>(T[] array, int newSize)

[`src/libraries/z42.core/src/Array.z42:305`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L305)

返回长度为 `newSize` 的新数组，原数组不变。

前缀拷自 `array`，尾部填 default(T)；`newSize` 更小时截断。C# 用 `ref T[]`，
这里把结果赋回。

<a id="Std.Array.Empty<T>()"></a>
### static T[] Empty<T>()

[`src/libraries/z42.core/src/Array.z42:317`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L317)

一个空（零长）的 `T[]`。

<a id="Std.Array.Sort<T>(T[],int,int)"></a>
### static void Sort<T>(T[] array, int index, int length) where T : IComparable

[`src/libraries/z42.core/src/Array.z42:331`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L331)

按 `CompareTo` 稳定升序排序子区间 `[index, index+length)`。

<a id="Std.Array.Reverse<T>(T[],int,int)"></a>
### static void Reverse<T>(T[] array, int index, int length)

[`src/libraries/z42.core/src/Array.z42:339`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L339)

原地反转子区间 `[index, index+length)`。

<a id="Std.Array.Fill<T>(T[],T,int,int)"></a>
### static void Fill<T>(T[] array, T value, int startIndex, int count)

[`src/libraries/z42.core/src/Array.z42:353`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L353)

把 `[startIndex, startIndex+count)` 置为 `value`。

<a id="Std.Array.Copy<T>(T[],int,T[],int,int)"></a>
### static void Copy<T>(T[] source, int sourceIndex, T[] destination, int destinationIndex, int length)

[`src/libraries/z42.core/src/Array.z42:363`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L363)

从 `source` 拷 `length` 个元素到 `destination` 的指定偏移处。

即 `source[sourceIndex..]` → `destination[destinationIndex..]`。同一数组内
重叠区间按 memmove 语义（向高索引移动时反向拷贝）。

<a id="Std.Array.IndexOf<T>(T[],T,int)"></a>
### static int IndexOf<T>(T[] array, T value, int startIndex)

[`src/libraries/z42.core/src/Array.z42:373`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L373)

`value` 在 `startIndex` 及其后首次出现的下标（按 `.Equals`），未命中 -1。

<a id="Std.Array.IndexOf<T>(T[],T,int,int)"></a>
### static int IndexOf<T>(T[] array, T value, int startIndex, int count)

[`src/libraries/z42.core/src/Array.z42:382`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L382)

`value` 在 `[startIndex, startIndex+count)` 内首次出现的下标（按 `.Equals`），未命中 -1。

<a id="Std.Array.LastIndexOf<T>(T[],T,int)"></a>
### static int LastIndexOf<T>(T[] array, T value, int startIndex)

[`src/libraries/z42.core/src/Array.z42:392`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L392)

从 `startIndex` 起向前找 `value` 最后一次出现的下标（按 `.Equals`），未命中 -1。

<a id="Std.Array.BinarySearch<T>(T[],int,int,T)"></a>
### static int BinarySearch<T>(T[] array, int index, int length, T value) where T : IComparable

[`src/libraries/z42.core/src/Array.z42:404`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L404)

在有序子区间 `[index, index+length)` 内按 `CompareTo` 二分查找。

命中返回 `value` 的下标，否则返回 `~insertionPoint`。

<a id="Std.Array.BinarySearch<T>(T[],T,Func<T,T,int>)"></a>
### static int BinarySearch<T>(T[] array, T value, Func<T,T,int> comparison)

[`src/libraries/z42.core/src/Array.z42:421`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L421)

在调用方比较器下对全数组二分查找。

比较器约定：`comparison(element, value)` < 0 ⇒ element 在 value 前。命中返回
下标，否则返回 `~insertionPoint`。

<a id="Std.Array.AsReadOnly<T>(T[])"></a>
### static ReadOnlyCollection<T> AsReadOnly<T>(T[] array)

[`src/libraries/z42.core/src/Array.z42:435`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Array.z42#L435)

把 `array` 包装成只读视图（不复制，按引用包装；视图不提供任何变更操作）。

