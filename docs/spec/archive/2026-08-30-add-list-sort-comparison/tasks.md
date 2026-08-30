# Tasks: List.Sort 委托版排序（Func<T,T,int>）

> 状态：🟢 已完成 | 创建：2026-08-30 | 完成：2026-08-30

## 进度概览
- [x] 阶段 1: 委托形态选型（Func<T,T,int>，剔除 C# Comparison<T> 名义冗余）
- [x] 阶段 2: List.Sort(Func<T,T,int>) 重载
- [x] 阶段 3: 测试与验证（完整 GREEN 全 stage 通过，含 self-host gen1==gen2）

## 阶段 1: 委托形态选型
- [x] 1.1 事实校正：z42c 不支持泛型用户委托 → `Comparison<T>` 不可作 stdlib 委托；改用内建 `Func<T,T,int>`（同时剔除 C# `Comparison<T>` vs `Func` 名义重复）

## 阶段 2: List.Sort(Func<T,T,int>) 重载
- [x] 2.1 `List.z42` 加 `public void Sort(Func<T, T, int> comparison)`：n<2 早退 + 分配 scratch + 调 `MergeSortCmp`
- [x] 2.2 `List.z42` 加私有 `void MergeSortCmp(T[] scratch, int lo, int hi, Func<T, T, int> cmp)`：镜像 MergeSort，比较从 `items[j].CompareTo(items[i])` 换成 `cmp(items[j], items[i])`（保稳定：ties 取左）

## 阶段 3: 测试与验证
- [x] 3.1 `tests/list_sort.z42` 加：降序 + 与 Sort() 一致的升序 + 空/单元素 + 稳定性（4 个 [Test]，全 PASS）
- [x] 3.2 完整 GREEN gate（`xtask test` 全 stage 通过：e2e / cross-zpkg / stdlib 20 文件 / 编译器自举 gen1==gen2 / vscode-syntax）
- [x] 3.3 spec 场景逐条覆盖确认（降序/升序一致/空单元素/稳定性 4 场景对应 4 [Test]）

## 备注
- 纯 stdlib，无新语言特性/无格式 bump/无 bootstrap 越界（不新增委托类型，复用内建 Func）。
- 类体非空非注释 171 行 < 200 硬限，无需拆分。
