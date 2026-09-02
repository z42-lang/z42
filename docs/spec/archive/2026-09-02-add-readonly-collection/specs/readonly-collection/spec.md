# Spec: `ReadOnlyCollection<T>` + `Array.AsReadOnly<T>`

## `Std.Array.AsReadOnly<T>(T[] array) -> ReadOnlyCollection<T>`

包装 `array` 为只读视图（按引用）。

## `Std.Collections.ReadOnlyCollection<T>`

### 构造 `ReadOnlyCollection(T[] array)`
存 `array` 引用；`Count == array.Length`。

### `int Count`（字段）
元素个数。

### `T this[int index] { get }`
返回 `array[index]`。越界抛 `IndexOutOfRangeException`（底层数组）。无 setter。

### `bool Contains(T value)`
`value` 是否在视图中（`Object.Equals` 线性比较）。

### `int IndexOf(T value)`
首个等于 `value` 的下标（`Object.Equals`）；未命中返回 `-1`。

### `void CopyTo(T[] array, int arrayIndex)`
把全部 `Count` 个元素拷到 `array` 从 `arrayIndex` 起。

### `T[] ToArray()`
返回含全部元素的**新数组**（对底层的防御性快照，修改结果不影响视图）。

### foreach
`foreach (T v in ro)` 顺序枚举 `[0, Count)`（走 `Count` + 索引器快路径）。

## Scenarios

| # | 场景 | 期望 |
|---|------|------|
| S1 | `AsReadOnly([10,20,30])` → `Count` / `[i]` | `Count==3`；`[0]==10`,`[1]==20`,`[2]==30` |
| S2 | `Contains` 命中 / 未命中 | `Contains(2)==true`；`Contains(9)==false` |
| S3 | `IndexOf` 首个匹配 / 未命中 | `[5,6,7,6].IndexOf(6)==1`；`IndexOf(99)==-1` |
| S4 | `CopyTo(dst, 1)` | dst `[0,1,2,3,0]` |
| S5 | `ToArray()` 是副本 | 改副本不影响视图；`length==Count` |
| S6 | 底层数组变更透过视图可见（按引用） | `xs[0]=100` 后 `ro[0]==100` |
| S7 | foreach 求和 | `[2,4,6]` → 12 |
| S8 | 引用类型元素（string） | `Count`/`Contains`/`IndexOf` 正确 |
