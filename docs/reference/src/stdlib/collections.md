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

`src/libraries/z42.collections/src/LinkedList.z42:4`

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

### T Value

`src/libraries/z42.collections/src/LinkedList.z42:6`

节点值（可读可写字段）。

### LinkedListNode(T value)

`src/libraries/z42.collections/src/LinkedList.z42:14`

创建持有 value 的节点。

### LinkedListNode<T>? Next()

`src/libraries/z42.collections/src/LinkedList.z42:21`

后继节点；尾节点返回 null。

### LinkedListNode<T>? Previous()

`src/libraries/z42.collections/src/LinkedList.z42:23`

前驱节点；头节点返回 null。

### void SetNext(LinkedListNode<T> n)

`src/libraries/z42.collections/src/LinkedList.z42:26`

设置后继节点（通常由 LinkedList<T> 维护）。

### void SetPrevious(LinkedListNode<T> p)

`src/libraries/z42.collections/src/LinkedList.z42:28`

设置前驱节点（通常由 LinkedList<T> 维护）。

## LinkedList

`src/libraries/z42.collections/src/LinkedList.z42:33`

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

### LinkedList()

`src/libraries/z42.collections/src/LinkedList.z42:39`

创建空链表。

### int Count()

`src/libraries/z42.collections/src/LinkedList.z42:46`

返回元素个数（方法，不是字段）。

### bool IsEmpty()

`src/libraries/z42.collections/src/LinkedList.z42:48`

是否为空。

### LinkedListNode<T>? First()

`src/libraries/z42.collections/src/LinkedList.z42:51`

头节点；空链表返回 null（不抛）。

### LinkedListNode<T>? Last()

`src/libraries/z42.collections/src/LinkedList.z42:53`

尾节点；空链表返回 null（不抛）。

### T[] ToArray()

`src/libraries/z42.collections/src/LinkedList.z42:56`

头 → 尾顺序的快照数组；不改动链表。

### LinkedListNode<T> AddFirst(T value)

`src/libraries/z42.collections/src/LinkedList.z42:71`

头插，返回新建的节点。

### LinkedListNode<T> AddLast(T value)

`src/libraries/z42.collections/src/LinkedList.z42:87`

尾插，返回新建的节点。

### void AddOne(T item)

`src/libraries/z42.collections/src/LinkedList.z42:102`

IBasicCollection 契约的统一插入入口，委托给 AddLast。

### T RemoveFirst()

`src/libraries/z42.collections/src/LinkedList.z42:107`

摘掉头节点并返回其 Value；空链表抛 Exception。

### T RemoveLast()

`src/libraries/z42.collections/src/LinkedList.z42:124`

摘掉尾节点并返回其 Value；空链表抛 Exception。

### bool Remove(T value)

`src/libraries/z42.collections/src/LinkedList.z42:141`

删除第一个值相等的节点；删掉返回 true，没找到返回 false。

### void Clear()

`src/libraries/z42.collections/src/LinkedList.z42:166`

清空（幂等）。

### bool Contains(T value)

`src/libraries/z42.collections/src/LinkedList.z42:175`

是否存在值相等的节点，等价 Find(value) != null。

### LinkedListNode<T>? Find(T value)

`src/libraries/z42.collections/src/LinkedList.z42:178`

第一个值相等的节点；没找到返回 null。

## PriorityQueue

`src/libraries/z42.collections/src/PriorityQueue.z42:9`

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

### PriorityQueue()

`src/libraries/z42.collections/src/PriorityQueue.z42:15`

创建空优先队列。

### int Count()

`src/libraries/z42.collections/src/PriorityQueue.z42:22`

返回元素个数（方法，不是字段）。

### bool IsEmpty()

`src/libraries/z42.collections/src/PriorityQueue.z42:24`

是否为空。

### void Enqueue(T item)

`src/libraries/z42.collections/src/PriorityQueue.z42:27`

入堆，O(log n)。

### void AddOne(T item)

`src/libraries/z42.collections/src/PriorityQueue.z42:35`

IBasicCollection 契约的统一插入入口，委托给 Enqueue。

### T Dequeue()

`src/libraries/z42.collections/src/PriorityQueue.z42:38`

取出并返回最小元素；空队列抛 Exception。O(log n)。

### T Peek()

`src/libraries/z42.collections/src/PriorityQueue.z42:52`

只看最小元素不取出；空队列抛 Exception。O(1)。

### void Clear()

`src/libraries/z42.collections/src/PriorityQueue.z42:60`

清空所有元素，保留已分配容量。

## Queue

`src/libraries/z42.collections/src/Queue.z42:5`

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

### Queue()

`src/libraries/z42.collections/src/Queue.z42:14`

创建空队列。

### int Count()

`src/libraries/z42.collections/src/Queue.z42:23`

返回元素个数（方法，不是字段）。

### bool IsEmpty()

`src/libraries/z42.collections/src/Queue.z42:25`

是否为空。

### void Enqueue(T item)

`src/libraries/z42.collections/src/Queue.z42:28`

入队（队尾）。

### void AddOne(T item)

`src/libraries/z42.collections/src/Queue.z42:36`

IBasicCollection 契约的统一插入入口，委托给 Enqueue。

### T Dequeue()

`src/libraries/z42.collections/src/Queue.z42:39`

出队并返回队首；空队列抛 Exception。

### T Peek()

`src/libraries/z42.collections/src/Queue.z42:50`

只看队首；空队列抛 Exception。

### void Clear()

`src/libraries/z42.collections/src/Queue.z42:58`

清空（幂等）。

### T[] ToArray()

`src/libraries/z42.collections/src/Queue.z42:65`

快照数组，队首在前（FIFO 顺序）；不消耗队列。

### void Grow()

`src/libraries/z42.collections/src/Queue.z42:78`

扩容容量（容量管理入口，正常使用无需调用）。

## SortedSet

`src/libraries/z42.collections/src/SortedSet.z42:5`

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

### SortedSet()

`src/libraries/z42.collections/src/SortedSet.z42:12`

创建空集合。

### int Count()

`src/libraries/z42.collections/src/SortedSet.z42:19`

返回元素个数（方法，不是字段）。

### bool IsEmpty()

`src/libraries/z42.collections/src/SortedSet.z42:21`

是否为空。

### bool Add(T item)

`src/libraries/z42.collections/src/SortedSet.z42:24`

新元素返回 true；已存在返回 false 且不改变集合。

### void AddOne(T item)

`src/libraries/z42.collections/src/SortedSet.z42:41`

IBasicCollection 契约的统一插入入口，委托给 Add。

### bool Contains(T item)

`src/libraries/z42.collections/src/SortedSet.z42:44`

二分查找是否存在。

### bool Remove(T item)

`src/libraries/z42.collections/src/SortedSet.z42:50`

删除并返回 true；本就不在返回 false。

### T Min()

`src/libraries/z42.collections/src/SortedSet.z42:65`

最小元素；空集合抛 Exception。

### T Max()

`src/libraries/z42.collections/src/SortedSet.z42:71`

最大元素；空集合抛 Exception。

### T[] ToArray()

`src/libraries/z42.collections/src/SortedSet.z42:77`

升序快照数组。

### void Clear()

`src/libraries/z42.collections/src/SortedSet.z42:84`

清空（幂等）。

### int LowerBound(T item)

`src/libraries/z42.collections/src/SortedSet.z42:89`

第一个 >= item 的元素下标；全都小于 item 时返回 Count()。

### void Grow()

`src/libraries/z42.collections/src/SortedSet.z42:104`

扩容容量（容量管理入口，正常使用无需调用）。

## Stack

`src/libraries/z42.collections/src/Stack.z42:24`

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

### Stack()

`src/libraries/z42.collections/src/Stack.z42:31`

创建空栈。

### int Count()

`src/libraries/z42.collections/src/Stack.z42:38`

返回元素个数（方法，不是字段）。

### bool IsEmpty()

`src/libraries/z42.collections/src/Stack.z42:40`

是否为空。

### void Push(T item)

`src/libraries/z42.collections/src/Stack.z42:43`

压栈。

### void AddOne(T item)

`src/libraries/z42.collections/src/Stack.z42:50`

IBasicCollection 契约的统一插入入口，委托给 Push。

### T Pop()

`src/libraries/z42.collections/src/Stack.z42:53`

弹出并返回栈顶；空栈抛 Exception。

### T Peek()

`src/libraries/z42.collections/src/Stack.z42:62`

只看栈顶不弹出；空栈抛 Exception。

### void Clear()

`src/libraries/z42.collections/src/Stack.z42:70`

清空（幂等）。

### T[] ToArray()

`src/libraries/z42.collections/src/Stack.z42:75`

快照数组，栈顶在前（LIFO 顺序）；不改动栈本身。

### void Grow()

`src/libraries/z42.collections/src/Stack.z42:86`

扩容容量（容量管理入口，正常使用无需调用）。

