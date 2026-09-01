# Tasks: Std.Array 补齐 C# 静态算法

> 状态：🟢 已完成 | 创建：2026-09-02 | 完成：2026-09-02

## 进度概览
- [x] 阶段 1: 实现 15 个静态方法（Array.z42）
- [x] 阶段 2: 测试（array_csharp_algorithms.z42，14 用例全绿）
- [x] 阶段 3: 文档同步 + GREEN + 归档

## 阶段 1: 实现（src/libraries/z42.core/src/Array.z42）
- [x] 1.1 谓词查找：Find / FindLast / FindIndex / FindLastIndex / FindAll / Exists / TrueForAll
- [x] 1.2 查找：BinarySearch（CompareTo + ~插入点）/ Contains（复用 IndexOf）/ LastIndexOf
- [x] 1.3 变换工具：ConvertAll / ForEach / Clear（default(T)）/ Resize（返回新数组）/ Empty

## 阶段 2: 测试（src/libraries/z42.core/tests/array_csharp_algorithms.z42）
- [x] 2.1 每方法 ≥1 正常 + ≥1 边界/未命中用例（z42.core stdlib 37 文件全绿）

## 阶段 3: 验证与归档
- [x] 3.1 cargo build (z42vm)
- [x] 3.2 xtask test —— 完整 GREEN
- [x] 3.3 spec scenarios 逐条覆盖（测试逐条对应）
- [x] 3.4 README 功能索引 Array 行补列新算法
- [x] 3.5 归档 changes/→archive/

## 备注
- Resize 返回新数组（非 ref）、BinarySearch 返回 ~插入点、Find 未命中 default(T)——见 design.md 裁决。
- 实现期发现：泛型 `new T[n]` 对值类型 T 不零初始化（尾部槽为 Null，element_type deferred 限制）→
  Resize 显式用 `default(T)` 填尾部（与 Clear 同款）绕过，非本变更 Scope 的 VM 修复。
