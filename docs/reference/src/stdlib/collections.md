<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.collections/ -->

# z42.collections —— 进阶集合容器。

> 包路径 `src/libraries/z42.collections/`；命名空间 `Std.Collections`

Stack（LIFO）/ Queue（FIFO）/ LinkedList / LinkedListNode（双向链表）、
PriorityQueue（最小堆）、SortedSet（有序去重集合）。
最基础的 List / Dictionary / HashSet 不在本包，它们随 z42.core 隐式可用。
本包的五个类型共用同一个 Std.Collections 命名空间，但要显式声明包依赖才能用。

## 不支持

- 五个类型都不能直接 foreach（都没有 GetEnumerator()，也没有索引器）。
  写了能通过编译，但运行期以 ArrayLen: expected array, got Object(...) 终止。
  Stack / Queue / LinkedList / SortedSet 请用 ToArray()；PriorityQueue 连 ToArray() 都没有。
- 没有预分配容量的构造器：五个类型都只有无参构造器。
- 不能从既有集合 / 数组构造：没有 new Queue<T>(items) 这类构造器，只能逐个加。
- 没有自定义比较器：顺序固定由元素的 CompareTo（PriorityQueue / SortedSet）、
  相等固定由 Equals（LinkedList / SortedSet）决定，构造器不接受比较器。
- SortedSet<T> 没有区间视图（GetViewBetween / Reverse() / 前驱后继查询）。
- 没有 SortedDictionary<K,V>、没有并发集合、没有 Deque 两端队列。
- Grow()（Stack / Queue / SortedSet）虽然是 public，属于容量管理入口，正常使用不需要调用。

## LinkedListNode

[`src/libraries/z42.collections/src/LinkedList.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L4)

双向链表节点，持有可读可写的 Value 字段。

```z42
public class LinkedListNode<T> {
    T Value
    LinkedListNode(T value)
    LinkedListNode<T>? Next()
    LinkedListNode<T>? Previous()
    void SetNext(LinkedListNode<T> n)
    void SetPrevious(LinkedListNode<T> p)
}
```

成员：[`Value`](#Std.Collections.LinkedListNode%3CT%3E.Value) · [`LinkedListNode`](#Std.Collections.LinkedListNode%3CT%3E.ctor(T)) · [`Next`](#Std.Collections.LinkedListNode%3CT%3E.Next()) · [`Previous`](#Std.Collections.LinkedListNode%3CT%3E.Previous()) · [`SetNext`](#Std.Collections.LinkedListNode%3CT%3E.SetNext(LinkedListNode%3CT%3E)) · [`SetPrevious`](#Std.Collections.LinkedListNode%3CT%3E.SetPrevious(LinkedListNode%3CT%3E))

<a id="Std.Collections.LinkedListNode<T>.Value"></a>
### T Value

[`src/libraries/z42.collections/src/LinkedList.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L6)

节点值（可读可写字段）。

<a id="Std.Collections.LinkedListNode<T>.ctor(T)"></a>
### LinkedListNode(T value)

[`src/libraries/z42.collections/src/LinkedList.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L14)

创建持有 value 的节点。

<a id="Std.Collections.LinkedListNode<T>.Next()"></a>
### LinkedListNode<T>? Next()

[`src/libraries/z42.collections/src/LinkedList.z42:21`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L21)

后继节点；尾节点返回 null。

<a id="Std.Collections.LinkedListNode<T>.Previous()"></a>
### LinkedListNode<T>? Previous()

[`src/libraries/z42.collections/src/LinkedList.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L23)

前驱节点；头节点返回 null。

<a id="Std.Collections.LinkedListNode<T>.SetNext(LinkedListNode<T>)"></a>
### void SetNext(LinkedListNode<T> n)

[`src/libraries/z42.collections/src/LinkedList.z42:26`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L26)

设置后继节点（通常由 LinkedList<T> 维护）。

<a id="Std.Collections.LinkedListNode<T>.SetPrevious(LinkedListNode<T>)"></a>
### void SetPrevious(LinkedListNode<T> p)

[`src/libraries/z42.collections/src/LinkedList.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L28)

设置前驱节点（通常由 LinkedList<T> 维护）。

## LinkedList

[`src/libraries/z42.collections/src/LinkedList.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L33)

双向链表：O(1) 的两端插入/删除与端点访问，O(n) 的按值查找。 where T: IEquatable（Find / Contains / Remove 需要比较相等）。

```z42
public class LinkedList<T> : IBasicCollection<T> where T : IEquatable {
    LinkedList()
    int Count()
    bool IsEmpty()
    LinkedListNode<T>? First()
    LinkedListNode<T>? Last()
    T[] ToArray()
    LinkedListNode<T> AddFirst(T value)
    LinkedListNode<T> AddLast(T value)
    void AddOne(T item)
    T RemoveFirst()
    T RemoveLast()
    bool Remove(T value)
    void Clear()
    bool Contains(T value)
    LinkedListNode<T>? Find(T value)
}
```

成员：[`LinkedList`](#Std.Collections.LinkedList%3CT%3E.ctor()) · [`Count`](#Std.Collections.LinkedList%3CT%3E.Count()) · [`IsEmpty`](#Std.Collections.LinkedList%3CT%3E.IsEmpty()) · [`First`](#Std.Collections.LinkedList%3CT%3E.First()) · [`Last`](#Std.Collections.LinkedList%3CT%3E.Last()) · [`ToArray`](#Std.Collections.LinkedList%3CT%3E.ToArray()) · [`AddFirst`](#Std.Collections.LinkedList%3CT%3E.AddFirst(T)) · [`AddLast`](#Std.Collections.LinkedList%3CT%3E.AddLast(T)) · [`AddOne`](#Std.Collections.LinkedList%3CT%3E.AddOne(T)) · [`RemoveFirst`](#Std.Collections.LinkedList%3CT%3E.RemoveFirst()) · [`RemoveLast`](#Std.Collections.LinkedList%3CT%3E.RemoveLast()) · [`Remove`](#Std.Collections.LinkedList%3CT%3E.Remove(T)) · [`Clear`](#Std.Collections.LinkedList%3CT%3E.Clear()) · [`Contains`](#Std.Collections.LinkedList%3CT%3E.Contains(T)) · [`Find`](#Std.Collections.LinkedList%3CT%3E.Find(T))

<a id="Std.Collections.LinkedList<T>.ctor()"></a>
### LinkedList()

[`src/libraries/z42.collections/src/LinkedList.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L39)

创建空链表。

<a id="Std.Collections.LinkedList<T>.Count()"></a>
### int Count()

[`src/libraries/z42.collections/src/LinkedList.z42:46`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L46)

返回元素个数（方法，不是字段）。

<a id="Std.Collections.LinkedList<T>.IsEmpty()"></a>
### bool IsEmpty()

[`src/libraries/z42.collections/src/LinkedList.z42:48`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L48)

是否为空。

<a id="Std.Collections.LinkedList<T>.First()"></a>
### LinkedListNode<T>? First()

[`src/libraries/z42.collections/src/LinkedList.z42:51`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L51)

头节点；空链表返回 null（不抛）。

<a id="Std.Collections.LinkedList<T>.Last()"></a>
### LinkedListNode<T>? Last()

[`src/libraries/z42.collections/src/LinkedList.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L53)

尾节点；空链表返回 null（不抛）。

<a id="Std.Collections.LinkedList<T>.ToArray()"></a>
### T[] ToArray()

[`src/libraries/z42.collections/src/LinkedList.z42:56`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L56)

头 → 尾顺序的快照数组；不改动链表。

<a id="Std.Collections.LinkedList<T>.AddFirst(T)"></a>
### LinkedListNode<T> AddFirst(T value)

[`src/libraries/z42.collections/src/LinkedList.z42:71`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L71)

头插，返回新建的节点。

<a id="Std.Collections.LinkedList<T>.AddLast(T)"></a>
### LinkedListNode<T> AddLast(T value)

[`src/libraries/z42.collections/src/LinkedList.z42:87`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L87)

尾插，返回新建的节点。

<a id="Std.Collections.LinkedList<T>.AddOne(T)"></a>
### void AddOne(T item)

[`src/libraries/z42.collections/src/LinkedList.z42:102`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L102)

IBasicCollection 契约的统一插入入口，委托给 AddLast。

<a id="Std.Collections.LinkedList<T>.RemoveFirst()"></a>
### T RemoveFirst()

[`src/libraries/z42.collections/src/LinkedList.z42:107`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L107)

摘掉头节点并返回其 Value；空链表抛 Exception。

<a id="Std.Collections.LinkedList<T>.RemoveLast()"></a>
### T RemoveLast()

[`src/libraries/z42.collections/src/LinkedList.z42:124`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L124)

摘掉尾节点并返回其 Value；空链表抛 Exception。

<a id="Std.Collections.LinkedList<T>.Remove(T)"></a>
### bool Remove(T value)

[`src/libraries/z42.collections/src/LinkedList.z42:141`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L141)

删除第一个值相等的节点；删掉返回 true，没找到返回 false。

<a id="Std.Collections.LinkedList<T>.Clear()"></a>
### void Clear()

[`src/libraries/z42.collections/src/LinkedList.z42:166`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L166)

清空（幂等）。

<a id="Std.Collections.LinkedList<T>.Contains(T)"></a>
### bool Contains(T value)

[`src/libraries/z42.collections/src/LinkedList.z42:175`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L175)

是否存在值相等的节点，等价 Find(value) != null。

<a id="Std.Collections.LinkedList<T>.Find(T)"></a>
### LinkedListNode<T>? Find(T value)

[`src/libraries/z42.collections/src/LinkedList.z42:178`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/LinkedList.z42#L178)

第一个值相等的节点；没找到返回 null。

## PriorityQueue

[`src/libraries/z42.collections/src/PriorityQueue.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/PriorityQueue.z42#L9)

最小堆优先队列：O(log n) 的 Enqueue / Dequeue，O(1) 的 Peek。 where T: IComparable —— CompareTo 小的先出队。

只有最小堆形态；要最大堆请把排序键取负，或用反向 CompareTo 的包装类型。
单类型形参：优先级就是元素本身，没有独立的 TPriority。

```z42
public class PriorityQueue<T> : IBasicCollection<T> where T : IComparable {
    PriorityQueue()
    int Count()
    bool IsEmpty()
    void Enqueue(T item)
    void AddOne(T item)
    T Dequeue()
    T Peek()
    void Clear()
}
```

成员：[`PriorityQueue`](#Std.Collections.PriorityQueue%3CT%3E.ctor()) · [`Count`](#Std.Collections.PriorityQueue%3CT%3E.Count()) · [`IsEmpty`](#Std.Collections.PriorityQueue%3CT%3E.IsEmpty()) · [`Enqueue`](#Std.Collections.PriorityQueue%3CT%3E.Enqueue(T)) · [`AddOne`](#Std.Collections.PriorityQueue%3CT%3E.AddOne(T)) · [`Dequeue`](#Std.Collections.PriorityQueue%3CT%3E.Dequeue()) · [`Peek`](#Std.Collections.PriorityQueue%3CT%3E.Peek()) · [`Clear`](#Std.Collections.PriorityQueue%3CT%3E.Clear())

<a id="Std.Collections.PriorityQueue<T>.ctor()"></a>
### PriorityQueue()

[`src/libraries/z42.collections/src/PriorityQueue.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/PriorityQueue.z42#L15)

创建空优先队列。

<a id="Std.Collections.PriorityQueue<T>.Count()"></a>
### int Count()

[`src/libraries/z42.collections/src/PriorityQueue.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/PriorityQueue.z42#L22)

返回元素个数（方法，不是字段）。

<a id="Std.Collections.PriorityQueue<T>.IsEmpty()"></a>
### bool IsEmpty()

[`src/libraries/z42.collections/src/PriorityQueue.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/PriorityQueue.z42#L24)

是否为空。

<a id="Std.Collections.PriorityQueue<T>.Enqueue(T)"></a>
### void Enqueue(T item)

[`src/libraries/z42.collections/src/PriorityQueue.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/PriorityQueue.z42#L27)

入堆，O(log n)。

<a id="Std.Collections.PriorityQueue<T>.AddOne(T)"></a>
### void AddOne(T item)

[`src/libraries/z42.collections/src/PriorityQueue.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/PriorityQueue.z42#L35)

IBasicCollection 契约的统一插入入口，委托给 Enqueue。

<a id="Std.Collections.PriorityQueue<T>.Dequeue()"></a>
### T Dequeue()

[`src/libraries/z42.collections/src/PriorityQueue.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/PriorityQueue.z42#L38)

取出并返回最小元素；空队列抛 Exception。O(log n)。

<a id="Std.Collections.PriorityQueue<T>.Peek()"></a>
### T Peek()

[`src/libraries/z42.collections/src/PriorityQueue.z42:52`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/PriorityQueue.z42#L52)

只看最小元素不取出；空队列抛 Exception。O(1)。

<a id="Std.Collections.PriorityQueue<T>.Clear()"></a>
### void Clear()

[`src/libraries/z42.collections/src/PriorityQueue.z42:60`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/PriorityQueue.z42#L60)

清空所有元素，保留已分配容量。

## Queue

[`src/libraries/z42.collections/src/Queue.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Queue.z42#L5)

先进先出队列，摊还 O(1) 的 Enqueue / Dequeue。对 T 无约束。

```z42
public class Queue<T> : IBasicCollection<T> {
    Queue()
    int Count()
    bool IsEmpty()
    void Enqueue(T item)
    void AddOne(T item)
    T Dequeue()
    T Peek()
    void Clear()
    T[] ToArray()
    void Grow()
}
```

成员：[`Queue`](#Std.Collections.Queue%3CT%3E.ctor()) · [`Count`](#Std.Collections.Queue%3CT%3E.Count()) · [`IsEmpty`](#Std.Collections.Queue%3CT%3E.IsEmpty()) · [`Enqueue`](#Std.Collections.Queue%3CT%3E.Enqueue(T)) · [`AddOne`](#Std.Collections.Queue%3CT%3E.AddOne(T)) · [`Dequeue`](#Std.Collections.Queue%3CT%3E.Dequeue()) · [`Peek`](#Std.Collections.Queue%3CT%3E.Peek()) · [`Clear`](#Std.Collections.Queue%3CT%3E.Clear()) · [`ToArray`](#Std.Collections.Queue%3CT%3E.ToArray()) · [`Grow`](#Std.Collections.Queue%3CT%3E.Grow())

<a id="Std.Collections.Queue<T>.ctor()"></a>
### Queue()

[`src/libraries/z42.collections/src/Queue.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Queue.z42#L14)

创建空队列。

<a id="Std.Collections.Queue<T>.Count()"></a>
### int Count()

[`src/libraries/z42.collections/src/Queue.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Queue.z42#L23)

返回元素个数（方法，不是字段）。

<a id="Std.Collections.Queue<T>.IsEmpty()"></a>
### bool IsEmpty()

[`src/libraries/z42.collections/src/Queue.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Queue.z42#L25)

是否为空。

<a id="Std.Collections.Queue<T>.Enqueue(T)"></a>
### void Enqueue(T item)

[`src/libraries/z42.collections/src/Queue.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Queue.z42#L28)

入队（队尾）。

<a id="Std.Collections.Queue<T>.AddOne(T)"></a>
### void AddOne(T item)

[`src/libraries/z42.collections/src/Queue.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Queue.z42#L36)

IBasicCollection 契约的统一插入入口，委托给 Enqueue。

<a id="Std.Collections.Queue<T>.Dequeue()"></a>
### T Dequeue()

[`src/libraries/z42.collections/src/Queue.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Queue.z42#L39)

出队并返回队首；空队列抛 Exception。

<a id="Std.Collections.Queue<T>.Peek()"></a>
### T Peek()

[`src/libraries/z42.collections/src/Queue.z42:50`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Queue.z42#L50)

只看队首；空队列抛 Exception。

<a id="Std.Collections.Queue<T>.Clear()"></a>
### void Clear()

[`src/libraries/z42.collections/src/Queue.z42:58`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Queue.z42#L58)

清空（幂等）。

<a id="Std.Collections.Queue<T>.ToArray()"></a>
### T[] ToArray()

[`src/libraries/z42.collections/src/Queue.z42:65`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Queue.z42#L65)

快照数组，队首在前（FIFO 顺序）；不消耗队列。

<a id="Std.Collections.Queue<T>.Grow()"></a>
### void Grow()

[`src/libraries/z42.collections/src/Queue.z42:78`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Queue.z42#L78)

扩容容量（容量管理入口，正常使用无需调用）。

## SortedSet

[`src/libraries/z42.collections/src/SortedSet.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/SortedSet.z42#L5)

按升序维护的去重集合：O(log n) 查找，Add / Remove 的元素搬移是 O(n)。 where T: IComparable + IEquatable。

```z42
public class SortedSet<T> : IBasicCollection<T> where T : IComparable, IEquatable {
    SortedSet()
    int Count()
    bool IsEmpty()
    bool Add(T item)
    void AddOne(T item)
    bool Contains(T item)
    bool Remove(T item)
    T Min()
    T Max()
    T[] ToArray()
    void Clear()
    int LowerBound(T item)
    void Grow()
}
```

成员：[`SortedSet`](#Std.Collections.SortedSet%3CT%3E.ctor()) · [`Count`](#Std.Collections.SortedSet%3CT%3E.Count()) · [`IsEmpty`](#Std.Collections.SortedSet%3CT%3E.IsEmpty()) · [`Add`](#Std.Collections.SortedSet%3CT%3E.Add(T)) · [`AddOne`](#Std.Collections.SortedSet%3CT%3E.AddOne(T)) · [`Contains`](#Std.Collections.SortedSet%3CT%3E.Contains(T)) · [`Remove`](#Std.Collections.SortedSet%3CT%3E.Remove(T)) · [`Min`](#Std.Collections.SortedSet%3CT%3E.Min()) · [`Max`](#Std.Collections.SortedSet%3CT%3E.Max()) · [`ToArray`](#Std.Collections.SortedSet%3CT%3E.ToArray()) · [`Clear`](#Std.Collections.SortedSet%3CT%3E.Clear()) · [`LowerBound`](#Std.Collections.SortedSet%3CT%3E.LowerBound(T)) · [`Grow`](#Std.Collections.SortedSet%3CT%3E.Grow())

<a id="Std.Collections.SortedSet<T>.ctor()"></a>
### SortedSet()

[`src/libraries/z42.collections/src/SortedSet.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/SortedSet.z42#L12)

创建空集合。

<a id="Std.Collections.SortedSet<T>.Count()"></a>
### int Count()

[`src/libraries/z42.collections/src/SortedSet.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/SortedSet.z42#L19)

返回元素个数（方法，不是字段）。

<a id="Std.Collections.SortedSet<T>.IsEmpty()"></a>
### bool IsEmpty()

[`src/libraries/z42.collections/src/SortedSet.z42:21`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/SortedSet.z42#L21)

是否为空。

<a id="Std.Collections.SortedSet<T>.Add(T)"></a>
### bool Add(T item)

[`src/libraries/z42.collections/src/SortedSet.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/SortedSet.z42#L24)

新元素返回 true；已存在返回 false 且不改变集合。

<a id="Std.Collections.SortedSet<T>.AddOne(T)"></a>
### void AddOne(T item)

[`src/libraries/z42.collections/src/SortedSet.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/SortedSet.z42#L41)

IBasicCollection 契约的统一插入入口，委托给 Add。

<a id="Std.Collections.SortedSet<T>.Contains(T)"></a>
### bool Contains(T item)

[`src/libraries/z42.collections/src/SortedSet.z42:44`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/SortedSet.z42#L44)

二分查找是否存在。

<a id="Std.Collections.SortedSet<T>.Remove(T)"></a>
### bool Remove(T item)

[`src/libraries/z42.collections/src/SortedSet.z42:50`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/SortedSet.z42#L50)

删除并返回 true；本就不在返回 false。

<a id="Std.Collections.SortedSet<T>.Min()"></a>
### T Min()

[`src/libraries/z42.collections/src/SortedSet.z42:65`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/SortedSet.z42#L65)

最小元素；空集合抛 Exception。

<a id="Std.Collections.SortedSet<T>.Max()"></a>
### T Max()

[`src/libraries/z42.collections/src/SortedSet.z42:71`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/SortedSet.z42#L71)

最大元素；空集合抛 Exception。

<a id="Std.Collections.SortedSet<T>.ToArray()"></a>
### T[] ToArray()

[`src/libraries/z42.collections/src/SortedSet.z42:77`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/SortedSet.z42#L77)

升序快照数组。

<a id="Std.Collections.SortedSet<T>.Clear()"></a>
### void Clear()

[`src/libraries/z42.collections/src/SortedSet.z42:84`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/SortedSet.z42#L84)

清空（幂等）。

<a id="Std.Collections.SortedSet<T>.LowerBound(T)"></a>
### int LowerBound(T item)

[`src/libraries/z42.collections/src/SortedSet.z42:89`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/SortedSet.z42#L89)

第一个 >= item 的元素下标；全都小于 item 时返回 Count()。

<a id="Std.Collections.SortedSet<T>.Grow()"></a>
### void Grow()

[`src/libraries/z42.collections/src/SortedSet.z42:104`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/SortedSet.z42#L104)

扩容容量（容量管理入口，正常使用无需调用）。

## Stack

[`src/libraries/z42.collections/src/Stack.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Stack.z42#L24)

后进先出栈，摊还 O(1) 的 Push / Pop。对 T 无约束。

```z42
public class Stack<T> : IBasicCollection<T> {
    Stack()
    int Count()
    bool IsEmpty()
    void Push(T item)
    void AddOne(T item)
    T Pop()
    T Peek()
    void Clear()
    T[] ToArray()
    void Grow()
}
```

成员：[`Stack`](#Std.Collections.Stack%3CT%3E.ctor()) · [`Count`](#Std.Collections.Stack%3CT%3E.Count()) · [`IsEmpty`](#Std.Collections.Stack%3CT%3E.IsEmpty()) · [`Push`](#Std.Collections.Stack%3CT%3E.Push(T)) · [`AddOne`](#Std.Collections.Stack%3CT%3E.AddOne(T)) · [`Pop`](#Std.Collections.Stack%3CT%3E.Pop()) · [`Peek`](#Std.Collections.Stack%3CT%3E.Peek()) · [`Clear`](#Std.Collections.Stack%3CT%3E.Clear()) · [`ToArray`](#Std.Collections.Stack%3CT%3E.ToArray()) · [`Grow`](#Std.Collections.Stack%3CT%3E.Grow())

<a id="Std.Collections.Stack<T>.ctor()"></a>
### Stack()

[`src/libraries/z42.collections/src/Stack.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Stack.z42#L31)

创建空栈。

<a id="Std.Collections.Stack<T>.Count()"></a>
### int Count()

[`src/libraries/z42.collections/src/Stack.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Stack.z42#L38)

返回元素个数（方法，不是字段）。

<a id="Std.Collections.Stack<T>.IsEmpty()"></a>
### bool IsEmpty()

[`src/libraries/z42.collections/src/Stack.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Stack.z42#L40)

是否为空。

<a id="Std.Collections.Stack<T>.Push(T)"></a>
### void Push(T item)

[`src/libraries/z42.collections/src/Stack.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Stack.z42#L43)

压栈。

<a id="Std.Collections.Stack<T>.AddOne(T)"></a>
### void AddOne(T item)

[`src/libraries/z42.collections/src/Stack.z42:50`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Stack.z42#L50)

IBasicCollection 契约的统一插入入口，委托给 Push。

<a id="Std.Collections.Stack<T>.Pop()"></a>
### T Pop()

[`src/libraries/z42.collections/src/Stack.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Stack.z42#L53)

弹出并返回栈顶；空栈抛 Exception。

<a id="Std.Collections.Stack<T>.Peek()"></a>
### T Peek()

[`src/libraries/z42.collections/src/Stack.z42:62`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Stack.z42#L62)

只看栈顶不弹出；空栈抛 Exception。

<a id="Std.Collections.Stack<T>.Clear()"></a>
### void Clear()

[`src/libraries/z42.collections/src/Stack.z42:70`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Stack.z42#L70)

清空（幂等）。

<a id="Std.Collections.Stack<T>.ToArray()"></a>
### T[] ToArray()

[`src/libraries/z42.collections/src/Stack.z42:75`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Stack.z42#L75)

快照数组，栈顶在前（LIFO 顺序）；不改动栈本身。

<a id="Std.Collections.Stack<T>.Grow()"></a>
### void Grow()

[`src/libraries/z42.collections/src/Stack.z42:86`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.collections/src/Stack.z42#L86)

扩容容量（容量管理入口，正常使用无需调用）。

