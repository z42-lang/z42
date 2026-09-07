# z42.collections — 集合库

## 职责

z42 **次级集合类型**（FIFO / LIFO / 有序 / 专用场景容器）。

> 最基础的三件套 `List<T>` / `Dictionary<K,V>` / `HashSet<T>` 已上提到
> [`z42.core/src/Collections/`](../z42.core/src/Collections/)（2026-04-25
> reorganize-stdlib-packages W1），与核心类型共享隐式 prelude 包；本包专注
> 于"需要显式 `using Std.Collections;` 触发加载"的进阶容器。

## src/ 核心文件

| 文件 | 类型 | 说明 |
|------|------|------|
| `Queue.z42` | `Queue<T>` | 队列（FIFO） |
| `Stack.z42` | `Stack<T>` | 栈（LIFO） |
| `LinkedList.z42` | `LinkedList<T>` / `LinkedListNode<T>` | 双向链表（O(1) 端点 / O(n) Find） |
| `PriorityQueue.z42` | `PriorityQueue<T> where T: IComparable` | 最小堆优先队列（O(log n) Enqueue/Dequeue / O(1) Peek） |

## Namespace

所有类型位于 `Std.Collections` namespace；用户代码需要 `using Std.Collections;`
才能无限定访问。

> 注意：`List<T>` / `Dictionary<K,V>` 虽然物理位于 `z42.core` 包，但 namespace
> 仍是 `Std.Collections`（与 C# BCL 对齐：`System.Collections.Generic.List<T>`
> 物理在 `System.Private.CoreLib` assembly）。

## 未来扩展（按需补齐）

| 类型 | 说明 | 阶段 |
|------|------|------|
| `SortedDictionary<K,V>` | 有序映射（红黑树） | L2 |
| `PriorityQueue<T>` | 优先队列（二叉堆） | L2 |
