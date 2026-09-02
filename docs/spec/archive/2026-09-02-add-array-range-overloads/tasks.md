# Tasks: Std.Array range / offset / paired 重载

> 状态：🟢 完成 | 创建：2026-09-02 | 归档：2026-09-02

## 进度概览
- [x] 阶段 1: Array.z42 实现
- [x] 阶段 2: 测试
- [x] 阶段 3: 验证 + 文档同步

## 阶段 1: Array.z42 实现
- [x] 1.1 私有 `_checkRange<T>(T[], int index, int length)` 统一边界校验
- [x] 1.2 `Sort<T>(T[], int index, int length)`（复用 `_mergeSort`）
- [x] 1.3 `Reverse<T>(T[], int, int)` / `Fill<T>(T[], T, int, int)`
- [x] 1.4 `Copy<T>(T[], int, T[], int, int)`（重叠区处理）
- [x] 1.5 `IndexOf<T>(T[], T, int)` / `IndexOf<T>(T[], T, int, int)` / `LastIndexOf<T>(T[], T, int)`
- [x] 1.6 `BinarySearch<T>(T[], int, int, T)` / `BinarySearch<T>(T[], T, Func<T,T,int>)`
- 配对排序 `Sort<TKey,TValue>` 移出本 change（决议歧义，见 proposal Out of Scope / design Decision 2）

## 阶段 2: 测试
- [x] 2.1 `tests/array_range_overloads.z42` 覆盖 spec 全部 scenario（正常 + 边界/异常）

## 阶段 3: 验证 + 文档同步
- [x] 3.1 `xtask test stdlib z42.core` 快信号绿
- [x] 3.2 完整 `xtask test` 全绿（GREEN — all stages passed，含自举不动点 3/3 gen1==gen2）
- [x] 3.3 `README.md` 功能索引 Array 行同步新重载
- [x] 3.4 spec scenario 逐条覆盖确认

## 备注
- overload 决议歧义（配对 Sort vs comparer Sort）实测确认；若歧义按 design Decision 5 调整。
