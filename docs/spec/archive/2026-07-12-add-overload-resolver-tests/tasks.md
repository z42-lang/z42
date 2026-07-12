# Tasks: 补 OverloadResolver 单测（review P3-5，test）

> 状态：🟢 已完成 | 完成：2026-07-12

**变更说明：** compiler-review §五/P3-5 测试缺口——OverloadResolver（重载决议核心算法）无单测。
补 `Resolve`（适用性/最具体/歧义/无匹配）+ `MangleKey`（签名键 + Canon 别名归一）8 个 [Test]。
**原因：** review 记账「OverloadResolver 无单测/golden 覆盖」；防重载决议回归。
**文档影响：** compiler_review.md §五测试缺口 + §七 P3-5 状态。

- [x] 1. `tests/overload/overload_tests.z42`（8 [Test]）+ 单元 toml
- [x] 2. 运行验证：8 passed（z42b 跑 [Test]，genB + flat31）
- [x] 3. compiler_review.md P3-5 状态更新

## 备注
- **P3-5 另一半（WorkspaceBuild 环检测）已由既有 `tests/workspace_topo/topo_tests.z42`
  `test_topo_cycle_throws` 覆盖**（review 2026-07-05 时该测试尚未加，账目已 stale）→ P3-5 整体清零。
- 纯加测试、零源码变化、无格式 bump。
