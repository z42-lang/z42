# z42.collections —— 进阶集合容器

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.collections/`；命名空间 `Std.Collections`

`Stack<T>`（LIFO）、`Queue<T>`（FIFO）、`LinkedList<T>` / `LinkedListNode<T>`（双向链表）、
`PriorityQueue<T>`（最小堆）、`SortedSet<T>`（有序去重集合）。

最基础的 `List<T>` / `Dictionary<K,V>` / `HashSet<T>` **不在本包**，它们随 `z42.core` 隐式可用，
见 [基础泛型集合](collections-core.md)。本包的五个类型共用同一个 `Std.Collections` 命名空间，
但要**显式声明包依赖**才能用。

## 直接用：`using Std.Collections;` 就够了

本包随工具链分发、**自动可用**，`z42.toml` 里**不需要**声明
（`[dependencies]` 只写第三方包，见[工程清单](../toolchain/z42-toml.md)）：

```z42
using Std.Collections;

Stack<int> s = new Stack<int>();
```

**单文件脚本同样可用**：`z42 run foo.z42` 里 `new Stack<int>()` 能编译也能跑。

> 📜 **2026-09-25 之前这一节写的是反的**，而且三处文档互相矛盾：
> 本节说「必须在清单里写上」「单文件用不了本包」，`z42-toml.md` 说「`z42.*` 始终可用、不要声明」。
>
> 真相是**两边都没执行**：能不能用取决于该包的命名空间有没有被 `z42.core` 抢先占住
> （nsMap first-wins）——`Std.Text` 没被占，未声明照样能用；`Std.Collections` 被占了，
> 于是 `new Stack<int>()` 编得过、跑起来 `MissingSymbolException`，声明与否都不影响判决。
> 那是一条实现细节冒充的规则。`fix-crosspkg-ns-reachability` 让 DEPS 取**全部**提供包，
> 这条不对称随之消失。

## 公共形状

五个类型都实现 `Std.IBasicCollection<T>`，因此都有这四个成员：

```z42
public interface IBasicCollection<T> {
    int  Count();          // 注意是方法，不是字段
    bool IsEmpty();
    void Clear();
    void AddOne(T item);   // 契约用的统一插入入口，委托给各自的自然 add
}
```

- **`Count()` 是方法**（写 `q.Count()`），与 `List<T>.Count` 那个**字段**不同。
- `AddOne` 只是给通用断言用的统一入口，日常代码用各自的 `Push` / `Enqueue` / `AddLast` / `Add`。
- `Clear()` 幂等。

**空容器上取元素一律抛 `Exception`，可以 `catch`**：

```z42
try { stack.Pop(); } catch (Exception e) { Console.WriteLine(e.Message); }
// Stack.Pop: stack is empty
```

各消息：`Stack.Pop: stack is empty` / `Stack.Peek: stack is empty` /
`Queue.Dequeue: queue is empty` / `Queue.Peek: queue is empty` /
`PriorityQueue.Dequeue: queue is empty` / `PriorityQueue.Peek: queue is empty` /
`LinkedList is empty` / `SortedSet: empty`。

## `Stack<T>`

后进先出栈，摊还 O(1) `Push` / `Pop`。**对 `T` 无约束**。

```z42
public class Stack<T> : IBasicCollection<T> {
    public Stack();

    public int  Count();
    public bool IsEmpty();
    public void Push(T item);
    public T    Pop();
    public T    Peek();
    public void Clear();
    public T[]  ToArray();
    public void AddOne(T item);
}
```

| 成员 | 说明 |
|---|---|
| `Push` | 压栈 |
| `Pop` | 弹出并返回栈顶；空栈抛 `Exception` |
| `Peek` | 只看栈顶不弹出；空栈抛 `Exception` |
| `ToArray()` | 快照，**栈顶在前**（LIFO 顺序）；不改动栈本身 |

```z42
var s = new Stack<int>();
s.Push(1); s.Push(2); s.Push(3);
int[] a = s.ToArray();   // { 3, 2, 1 }
```

## `Queue<T>`

先进先出队列，摊还 O(1) `Enqueue` / `Dequeue`。**对 `T` 无约束**。

```z42
public class Queue<T> : IBasicCollection<T> {
    public Queue();

    public int  Count();
    public bool IsEmpty();
    public void Enqueue(T item);
    public T    Dequeue();
    public T    Peek();
    public void Clear();
    public T[]  ToArray();
    public void AddOne(T item);
}
```

| 成员 | 说明 |
|---|---|
| `Enqueue` | 入队（队尾） |
| `Dequeue` | 出队并返回队首；空队列抛 `Exception` |
| `Peek` | 只看队首；空队列抛 `Exception` |
| `ToArray()` | 快照，**队首在前**（FIFO 顺序）；不消耗队列 |

## `LinkedList<T>` / `LinkedListNode<T>`

双向链表：O(1) 的两端插入 / 删除与端点访问，O(n) 的按值查找。
`where T: IEquatable`（`Find` / `Contains` / `Remove` 要比相等）。

```z42
public class LinkedListNode<T> {
    public T Value;                                  // 可读可写字段

    public LinkedListNode(T value);
    public LinkedListNode<T> Next();
    public LinkedListNode<T> Previous();
    public void SetNext(LinkedListNode<T> n);
    public void SetPrevious(LinkedListNode<T> p);
}

public class LinkedList<T> : IBasicCollection<T> where T: IEquatable {
    public LinkedList();

    public int  Count();
    public bool IsEmpty();
    public LinkedListNode<T> First();
    public LinkedListNode<T> Last();
    public LinkedListNode<T> AddFirst(T value);
    public LinkedListNode<T> AddLast(T value);
    public T    RemoveFirst();
    public T    RemoveLast();
    public bool Remove(T value);
    public void Clear();
    public bool Contains(T value);
    public LinkedListNode<T> Find(T value);
    public T[]  ToArray();
    public void AddOne(T item);
}
```

| 成员 | 说明 |
|---|---|
| `First()` / `Last()` | 头 / 尾**节点**；**空链表返回 `null`**（不抛） |
| `AddFirst` / `AddLast` | 头插 / 尾插，**返回新建的节点** |
| `RemoveFirst` / `RemoveLast` | 摘掉头 / 尾并返回其 `Value`；**空链表抛 `Exception`** |
| `Remove(value)` | 删除**第一个**值相等的节点；删掉返回 `true`，没找到返回 `false` |
| `Find(value)` | 第一个值相等的节点；**没找到返回 `null`** |
| `Contains(value)` | 等价 `Find(value) != null` |
| `ToArray()` | 头 → 尾顺序的快照数组；不改动链表 |

节点的 `Next()` / `Previous()` 用于手写遍历；`SetNext` / `SetPrevious` 虽然是 `public`，
但链表的接合由 `LinkedList<T>` 自己负责，**外部调用会破坏链表的 `Count` 与端点不变式**。

```z42
var node = list.First();
while (node != null) {
    Console.WriteLine(node.Value);
    node = node.Next();
}
```

`LinkedList<T>` 不支持 `foreach`（无 `GetEnumerator()`、无索引器）——用上面的节点遍历，或 `ToArray()`。
直接写 `foreach (var v in list)` 能编过，但运行期崩，见页尾「不支持」。

## `PriorityQueue<T>`

最小堆优先队列：O(log n) `Enqueue` / `Dequeue`，O(1) `Peek`。
`where T: IComparable`——**`CompareTo` 小的先出队**。

```z42
public class PriorityQueue<T> : IBasicCollection<T> where T: IComparable {
    public PriorityQueue();

    public int  Count();
    public bool IsEmpty();
    public void Enqueue(T item);
    public T    Dequeue();
    public T    Peek();
    public void Clear();
    public void AddOne(T item);
}
```

| 成员 | 说明 |
|---|---|
| `Enqueue` | 入堆 |
| `Dequeue` | 取出并返回**最小**元素；空队列抛 `Exception` |
| `Peek` | 只看最小元素；空队列抛 `Exception` |

```z42
var pq = new PriorityQueue<int>();
pq.Enqueue(5); pq.Enqueue(1); pq.Enqueue(3);
pq.Dequeue();   // 1
pq.Dequeue();   // 3
```

- **只有最小堆形态**。要最大堆，把排序键取负，或用一个 `CompareTo` 反向的包装类型。
- **单类型形参**：优先级就是元素本身，没有 `PriorityQueue<TElement, TPriority>` 那种
  「元素 + 独立优先级」形态。
- **没有 `ToArray()`**，也没有任何非破坏性遍历手段——只能反复 `Dequeue`（会清空队列）。
- 相同优先级元素之间的出队顺序**不保证**。

## `SortedSet<T>`

按升序维护的去重集合：O(log n) 查找，`Add` / `Remove` 的元素搬移是 O(n)。
`where T: IComparable + IEquatable`。

```z42
public class SortedSet<T> : IBasicCollection<T> where T: IComparable + IEquatable {
    public SortedSet();

    public int  Count();
    public bool IsEmpty();
    public bool Add(T item);
    public bool Contains(T item);
    public bool Remove(T item);
    public T    Min();
    public T    Max();
    public T[]  ToArray();
    public void Clear();
    public int  LowerBound(T item);
    public void AddOne(T item);
}
```

| 成员 | 说明 |
|---|---|
| `Add(item)` | 新元素返回 `true`；**已存在返回 `false` 且不改变集合** |
| `Contains` / `Remove` | 二分查找；`Remove` 删掉返回 `true`，本就不在返回 `false` |
| `Min()` / `Max()` | 最小 / 最大元素；**空集合抛 `Exception`** |
| `ToArray()` | **升序**快照数组 |
| `LowerBound(item)` | 第一个 `>= item` 的元素下标；全都小于 `item` 时返回 `Count()` |

```z42
var s = new SortedSet<int>();
s.Add(5); s.Add(1); s.Add(3); s.Add(5);   // 最后一个返回 false
s.ToArray();        // { 1, 3, 5 }
s.Min();            // 1
s.LowerBound(4);    // 2
```

`SortedSet<T>` 同样不支持 `foreach`——用 `ToArray()`。

## 用法

```z42
using Std.IO;
using Std.Collections;

void Main() {
    var work = new Queue<string>();
    work.Enqueue("a"); work.Enqueue("b");
    while (!work.IsEmpty()) { Console.WriteLine(work.Dequeue()); }

    var pq = new PriorityQueue<int>();
    pq.Enqueue(5); pq.Enqueue(1);
    Console.WriteLine(pq.Peek());              // 1

    var ll = new LinkedList<int>();
    ll.AddLast(1); ll.AddLast(2); ll.AddFirst(0);
    foreach (var v in ll.ToArray()) { Console.WriteLine(v); }

    try {
        new Stack<int>().Pop();
    } catch (Exception e) {
        Console.WriteLine(e.Message);          // Stack.Pop: stack is empty
    }
}
```

## 不支持

- **五个类型都不能直接 `foreach`**（都没有 `GetEnumerator()`，也没有索引器）。
  写了**能通过编译**，但运行期以 `ArrayLen: expected array, got Object(...)` 终止。
  `Stack` / `Queue` / `LinkedList` / `SortedSet` 请用 `ToArray()`；`PriorityQueue` 连 `ToArray()` 都没有。
- **没有预分配容量的构造器**：五个类型都只有无参构造器。
- **不能从既有集合 / 数组构造**：没有 `new Queue<T>(items)` 这类构造器，只能逐个加。
- **没有自定义比较器**：顺序固定由元素的 `CompareTo`（`PriorityQueue` / `SortedSet`）、
  相等固定由 `Equals`（`LinkedList` / `SortedSet`）决定，构造器不接受比较器。
- **`SortedSet<T>` 没有区间视图**（`GetViewBetween` / `Reverse()` / 前驱后继查询）。
- **没有 `SortedDictionary<K,V>`**、没有并发集合、没有 `Deque` 两端队列。
- `Grow()`（`Stack` / `Queue` / `SortedSet`）虽然是 `public`，属于容量管理入口，正常使用不需要调用。

## 相关

- [基础泛型集合](collections-core.md)——`List` / `Dictionary` / `HashSet` 及其附属类型
- [泛型约束（`where` 子句）](../language/generic-constraints.md)——`IComparable` / `IEquatable` 的写法
- [迭代（foreach）](../language/iteration.md)——`foreach` 走哪条路径的判定
- [工程清单 z42.toml](../toolchain/z42-toml.md)——`[dependencies]` 怎么写
