# Design: `ReadOnlyCollection<T>` + `Array.AsReadOnly<T>`

## 关键约束调研（z42 现状，2026-09-02 explore）

| 特性 | 现状 | 影响 |
|------|------|------|
| 索引器 `this[int] { get }` | **支持**（`z42c.syntax/MemberParser.z42`；`List`/`Dictionary` 已用） | 只读索引器可行，省略 `set` |
| foreach 协议 | 类有 `get_Item` + `Count`（字段或方法）即走**索引快路径**（`StmtBinder.z42:61-63`，#365 后仍作 fallback 保留） | 只需 `Count` + get 索引器，**无需 `GetEnumerator`** |
| 只读接口 | `IReadOnlyList`/`ICollection` 等**不存在**；仅 `IEnumerable<T>`/`IEnumerator<T>` | 不实现接口，做具体类 |
| 泛型无约束 `.Equals` | 可用（`Object.Equals`，#380/#382 已验证无约束调用） | `Contains`/`IndexOf` 不需 `where T: IEquatable` |
| `NotSupportedException` | 存在 | 本设计不用（无变更 API，无需抛） |

## Decisions

### D1：具体类而非接口实现
不引入 `IReadOnlyList<T>` 等接口（z42.core 无）。只读性由「无变更 API」在类型层面保证，而非
C# 那样实现 `IList<T>` 再在 `Add`/`Insert`/`set` 上抛 `NotSupportedException`。更简、无接口债。

### D2：按引用包装，不复制（C# 语义）
`ReadOnlyCollection(T[] array)` 存 `this.items = array`（引用），非拷贝。对底层数组的变更**透过视图可见**——
视图只阻止「透过视图变更」，不冻结底层。与 C# `Array.AsReadOnly` 一致。需要防御性快照时用 `ToArray()`。

### D3：无泛型约束
`ReadOnlyCollection<T>` 不加 `where T: ...`（不同于 `List<T>` 的 `IEquatable + IComparable`），
使 `Array.AsReadOnly` 能包装任意 `T[]`。`Contains`/`IndexOf` 用无约束 `.Equals`。

### D4：foreach 走索引快路径
提供 `public int Count`（字段，同 `List`）+ get-only `this[int]` → z42 索引快路径自动可 foreach，
不写 `GetEnumerator`（也就不依赖未在本类实现的 `IEnumerable<T>`）。

### D5：越界交底层数组
索引器 `get { return this.items[index]; }` 越界由底层 `T[]` 抛 `IndexOutOfRangeException`，不重复校验。

## API

```
// Collections/ReadOnlyCollection.z42
public sealed class ReadOnlyCollection<T> {
    T[] items;
    public int Count;
    public ReadOnlyCollection(T[] array);        // 按引用包装，Count = array.Length
    public T this[int index] { get; }            // 只读
    public bool Contains(T value);               // Object.Equals 线性
    public int IndexOf(T value);                 // 首个匹配下标，未命中 -1
    public void CopyTo(T[] array, int arrayIndex);
    public T[] ToArray();                        // 防御性副本
}

// Array.z42（using Std.Collections）
public static ReadOnlyCollection<T> AsReadOnly<T>(T[] array);
```

## 数据流

`Array.AsReadOnly<T>(xs)` → `new ReadOnlyCollection<T>(xs)`（存引用 + Count）。
读经索引器/`Contains`/`IndexOf`/foreach 直接打到 `items`。`ToArray` 复制出新数组。

## 依赖 / 循环

`Array`（`Std`）新 `using Std.Collections` 引用 `ReadOnlyCollection`；两者同在 `z42.core` 单一 zpkg，
包内引用无编译序问题（z42.core 整体一编译单元）。`ReadOnlyCollection` 自身只依赖 `T[]` + `Object.Equals`，
不回指 `Array` 的静态算法，无真实循环。
