# Tasks: `ReadOnlyCollection<T>` + `Array.AsReadOnly<T>`

> 状态：🟢 完成 | 创建：2026-09-02 | 归档：2026-09-02

## 进度概览
- [x] 阶段 1: 实现
- [x] 阶段 2: 测试
- [x] 阶段 3: 验证 + 文档同步

## 阶段 1: 实现
- [x] 1.1 `Collections/ReadOnlyCollection.z42`：sealed 泛型类，`Count` + get 索引器 + `Contains`/`IndexOf`/`CopyTo`/`ToArray`
- [x] 1.2 `Array.z42`：`using Std.Collections` + `AsReadOnly<T>(T[])`

## 阶段 2: 测试
- [x] 2.1 `tests/readonly_collection.z42`：覆盖 spec S1–S8（含 foreach、按引用可见、string 元素）

## 阶段 3: 验证 + 文档同步
- [x] 3.1 `xtask test stdlib z42.core` 快信号绿（40 文件全过；ReadOnlyCollection 8/8 PASS）
- [x] 3.2 完整 `xtask test` 全绿（GREEN — all stages passed，含自举不动点 3/3 gen1==gen2）
- [x] 3.3 README：Array 行加 `AsReadOnly`；Collections 表加 `ReadOnlyCollection.z42` 行
- [x] 3.4 spec scenario 逐条覆盖确认（S1–S8 对应 8 个 [Test]）

## 备注
- 纯脚本 stdlib feat，无格式 bump。只读接口层次（IReadOnlyList 等）Out of scope（z42.core 无该接口族）。
