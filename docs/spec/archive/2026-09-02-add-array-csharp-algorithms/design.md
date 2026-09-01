# Design: Std.Array C# 静态算法补齐

## Architecture
纯脚本增量：全部方法作为 `public static` 泛型方法追加到既有 `sealed class Array`（`Array.z42`），
基于既有原语（`arr[i]` 索引、`arr.Length`、`new T[n]`）与委托类型（`Predicate<T>`/`Func<T,R>`/`Action<T>`）
实现。无 native、无 IR、无 VM 改动。

## Decisions

### Decision 1: 谓词类型用 `Predicate<T>` 精确对标 C#
**问题：** Find/Exists 等用 `Predicate<T>` 还是 `Func<T,bool>`？
**决定：** 用 `Predicate<T>`（`delegate bool Predicate<T>(T arg)`，Delegates.z42 已定义），与 C#
`System.Array.Find(T[], Predicate<T>)` 逐字对齐。`ConvertAll` 用 `Func<T,R>`（映射），`ForEach` 用 `Action<T>`。

### Decision 2: `Resize` 返回新数组（不用 ref）
**问题：** C# 是 `void Resize<T>(ref T[], int)` 原地改调用方变量。
**决定：** `T[] Resize<T>(T[] array, int newSize)` 返回新数组。数组不可变长 → 返回新数组更贴合事实、更函数式、
更安全；`ref`-of-array-param 在 z42 验证少。调用方写 `xs = Array.Resize<int>(xs, 5)`。（User 裁决）
增长：尾部填 `default(T)`；收缩：截断；`newSize == 原长` 返回内容相同的新数组（拷贝语义）。

### Decision 3: `BinarySearch` 返回 C# 补码插入点
**问题：** 找不到时返回 -1 还是插入点？
**决定：** 对标 C#：命中返回下标；未命中返回 `~lo`（`lo` 为应插入位置），调用方 `if (r < 0) insertAt = ~r`。
`~` 运算符 z42 支持（crypto/BigInt 已用）。要求数组已按 `CompareTo` 升序（与 `Sort<T>()` 同序）。

### Decision 4: `Find`/`FindLast` 未命中返回 `default(T)`
对标 C#：无匹配返回 `default(T)`（引用类型 null、值类型零值）。`default(T)` z42 支持（MulticastFunc 已用）。
`FindIndex`/`FindLastIndex`/`LastIndexOf` 未命中返回 -1。

### Decision 5: 单文件放置（依 String/List 先例）
**问题：** 加满后 `Array` 类超 200 行类型软限。
**决定：** 仍放单一 `Array.z42`（加后约 260 行，文件 < 300 软限）。stdlib API 面大类超 200 行类限有既有先例
（`String.z42` 455 行、`List.z42` 225 行单类），不为此单独拆 partial（避免过度工程 + 保持 `Array.X` 调用点
与 C# 一致）。

## Implementation Notes
- 边界：`Clear(array, index, length)` 越界（`index<0 || length<0 || index+length>Length`）抛 `Exception`
  （与既有 `Copy` 越界抛错一致）。
- `FindAll`/`ConvertAll` 先用与源等长临时缓冲，`FindAll` 命中计数后拷贝到精确长度返回。
- `Contains<T>` 复用既有 `Array.IndexOf<T>`（`>= 0`）。
- `Empty<T>()` 直接 `return new T[0];`（不做 C# 的静态缓存，pre-1.0 保持最简）。

## Testing Strategy
- 新增 `src/libraries/z42.core/tests/array_csharp_algorithms.z42`：每方法 ≥1 正常 + ≥1 边界/未命中用例
  （空数组、无匹配、越界、`BinarySearch` 命中/未命中/补码、`Resize` 增/减/等长、`default(T)` 返回）。
- GREEN：`xtask test` 完整门禁（stdlib stage 覆盖本 `[Test]` 文件 + e2e/compiler 自举/vscode-syntax）。
