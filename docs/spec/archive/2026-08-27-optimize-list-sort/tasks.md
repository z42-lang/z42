# Tasks: optimize-list-sort

> 状态：🟡 进行中 | 创建：2026-08-26
> 类型：perf（最小化模式）；属「stdlib 性能改善程序」PR1

**变更说明：** z42.core 核心集合的性能改进——List.Sort 插入排序 O(n²) → 归并排序
O(n log n)；补 List(int capacity) 构造函数 + AddRange 预扩容；Dictionary.Remove
backfill 复用已存 hash 免重算 GetHashCode。
**原因：** List 是 prelude、最广泛使用；Sort 对上千元素退化成 O(n²)。其余为常数因子/分配优化。
**文档影响：** z42.core README 功能索引（List 加 capacity ctor）；List.z42 头注（去掉「Sort 插入排序」描述）。

## 进度概览
- [x] 1 List.Sort 归并排序（stable，O(n log n)）
- [x] 2 List(int capacity) ctor + AddRange 预扩容
- [x] 3 Dictionary.Remove inline 重哈希
- [x] 4 bench + 测试
- [ ] 5 GREEN + 文档同步

## 阶段 1: List.Sort
- [x] 1.1 `z42.core/src/Collections/List.z42`：`Sort()` 换成 top-down 归并排序（scratch 缓冲一次分配），保持 stable；更新头注 :6

## 阶段 2: List 容量 / AddRange
- [x] 2.1 加 `public List(int capacity)` ctor（capacity<1 回落 1）
- [x] 2.2 `AddRange` 一次预扩容到 `Count+items.Length` 再拷（不逐个 Add）
- [x] 2.3 内部 `EnsureCapacity(int)` 辅助（供 AddRange / 未来复用）

## 阶段 3: Dictionary.Remove
- [x] 3.1 `Dictionary.z42`：`Remove` backfill 用 `hashes[j]` inline 再插（照抄 Grow :176-185 思路），不再 `Set()`

## 阶段 4: bench + 测试
- [x] 4.1 `z42.core/bench/core_bench.z42` 加 `bench_list_sort`（乱序 int n=256）
- [x] 4.2 List.Sort 正确性回归测试（空/单元素/已排序/逆序/含重复且验 stable）——`z42.core/tests/`
- [x] 4.3 List capacity ctor + AddRange 结果不变测试；Dictionary.Remove 探测链完整性测试

## 阶段 5: 验证 + 文档
- [ ] 5.1 `xtask test stdlib z42.core`（--no-build 单跑，完整 GREEN 交 CI）
- [x] 5.2 z42.core README 功能索引 + List.z42 头注同步
- [ ] 5.3 归档 + PR

## 备注
- 零格式 bump（纯 stdlib 源码）。
- 本机 z42vm 退出挂起风险 → 完整 `xtask test` 交 PR CI 门禁。
