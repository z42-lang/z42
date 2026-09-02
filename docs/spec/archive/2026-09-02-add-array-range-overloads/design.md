# Design: Std.Array range / offset / paired 重载

## Architecture

纯脚本泛型算法，全部作为 `Std.Array`（`sealed class Array`）的静态方法，复用既有私有归并排序骨架
（`_mergeSort` 已按 `[lo, hi)` 参数化，range 排序可直接复用）。无 VM / codegen / IR 改动。

## Decisions

### Decision 1: range 排序复用现有 `_mergeSort`

**问题：** `Sort<T>(T[], int index, int length)` 是否新写归并逻辑？
**决定：** 复用。现有 `_mergeSort<T>(a, scratch, lo, hi)` 已按绝对区间 `[lo, hi)` 工作，
`Sort(array, index, length)` 只需 `scratch = new T[array.Length]` 后调 `_mergeSort(array, scratch, index, index+length)`。
scratch 用满长度（而非 length），使 `_mergeSort` 内按绝对下标 `scratch[k]`（k∈[lo,hi)）读写一致。

### Decision 2: 配对排序移出本 change（决议歧义，实测证实）

**问题：** `Sort<TKey,TValue>(TKey[], TValue[])`（配对排序）与既有 `Sort<T>(T[], Func<T,T,int>)`
均为 2 参、仅类型参数个数不同，能否共存？
**实测结论：** **不能。** 加入 paired Sort 后，`Sort<int>(arr, lambda)`（既有 comparator 调用）被误绑
——`test_sort_comparator_descending` 从降序变升序、comparator 被丢。根因在编译器 overload 决议器对
「相同参数数、不同类型参数个数」候选的选择，属 stdlib scope 之外。
**决定：** 配对排序移出本 change，另起独立 change（先修/评估决议器）。本 change 只保留按 arg-count
与既有重载天然区分、无歧义的 range/offset 重载。

### Decision 3: 偏移 Copy 处理重叠区

**问题：** `Copy` 偏移版是否处理 src==dst 且区间重叠（如自左向右平移）？
**决定：** 处理，对齐 C# `System.Array.Copy` 语义。`destinationIndex > sourceIndex` 时从高位向低位
拷贝（backward），否则正向——保证同数组重叠移动结果正确。

### Decision 4: 未命中 / 越界约定沿用 #380

- range `BinarySearch` 未命中返回 `~插入点`（与既有 `BinarySearch<T>(T[], T)` 一致）。
- range 越界（`index < 0 || length < 0 || index+length > Length`）抛 `Exception`（私有 `_checkRange` 统一校验）。
- `IndexOf(startIndex[, count])` / `LastIndexOf(startIndex)` 未命中返回 -1。

### Decision 5: 保留重载均按 arg-count 与既有区分（无歧义）

本 change 保留的重载相对既有同名方法均**参数数不同**，走既有 arg-count 区分，无决议歧义：
`Sort` range 版 3 参（既有 1/2 参）；`Reverse` range 4... 实为 3 参（既有 1 参）；`Fill` range 4 参
（既有 2 参）；`Copy` offset 5 参（既有 3 参）；`IndexOf` 3/4 参（既有 2 参）；`LastIndexOf` 3 参
（既有 2 参）；`BinarySearch` range 4 参 / comparer 3 参（既有 2 参）。实测全部正确绑定
（唯一歧义来自已移出的 paired Sort，见 Decision 2）。

## Implementation Notes

- 所有 range 版先调 `_checkRange<T>(array, index, length)` 统一边界校验。
- 新增算法**均不读 `new T[n]` 的未写入槽**：排序 scratch 在 merge 内先写满区间再读回；配对 scratch 同理；
  故不踩泛型数组值类型零初始化坑（那是独立 vm change，见 proposal Out of Scope）。
- 类体积：Array.z42 现 297 行，本批加约 120 行 → ~420 行（< 500 文件硬限）。类超 200 行沿用
  String（455 行）/ #380 既有 BCL-mirror 偏差，可接受。

## Testing Strategy

- 单元 [Test]：`tests/array_range_overloads.z42`，每个重载 1 正常 + 1 边界/异常。
  - range 排序：区间内有序、区间外不动。
  - 配对排序：keys 有序、items 同步；两条 Sort 重载各测一次确认决议不歧义。
  - 偏移 Copy：不同数组、同数组重叠（前移/后移）。
  - range Fill/Reverse：区间外不动。
  - IndexOf/LastIndexOf range：命中 / 未命中 / 越界收敛。
  - BinarySearch range/comparer：命中 / ~插入点。
  - 越界抛异常（`_checkRange`）。
- GREEN：`xtask test stdlib z42.core` 快信号；commit 前完整 `xtask test`。
