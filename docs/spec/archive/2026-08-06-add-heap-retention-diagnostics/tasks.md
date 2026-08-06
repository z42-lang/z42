# Tasks: 通用堆保留诊断（Heap Retention Diagnostics）

> 状态：🟢 已完成 | 创建：2026-08-06 | 完成：2026-08-06 | User 6.5 已确认
> 变更类型：`vm`（完整流程）| 分支：`worktree-heap-retention-diag`（off origin/main）

## 验证报告（GREEN）
- ✅ Rust 单测 `retention_tests`：7/7（反向图 / L1 直接引用者 / L2 反向 BFS 根 / 类别去重有序 / 环处理）
- ✅ e2e `heap_retention` interp+jit：L1 找到引用者 + 空对象 0 引用者 + L2 找到 StaticField 根
- ✅ **完整 `xtask test` 全绿**：e2e + cross-zpkg + stdlib（含 `test_callback_exception_does_not_kill_timer` 通过——命名空间冲突已修）+ compiler 自举 5/5 gen1==gen2 + vscode
- **实施期修一坑**：`Std.Diagnostics` 命名空间归 z42.diagnostics 库，最初误放 z42.core → z42.threading 的 `Std.Diagnostics.Log.Error` 跨 zpkg 解析冲突（gate 实测 undefined）；4 个类移入 z42.diagnostics（拥有者）修复，Log 与 Heap 同命名空间共存无冲突。

## 进度概览
- [ ] 阶段 1: 结果类型 + 反向图 + 查询（gc/retention.rs）
- [ ] 阶段 2: 分类根 scanner（trait + arc_heap + vm_context 接线）
- [ ] 阶段 3: MagrGC 查询接口 + arc_heap 实现
- [ ] 阶段 4: builtins + Std.Diagnostics stdlib 类
- [ ] 阶段 5: 测试
- [ ] 阶段 6: 验证 + 文档

## 阶段 1: 反向图 + 查询（gc/retention.rs）
- [ ] 1.1 结果类型：`RetainerInfo` / `RetainerKind` / `RootInfo` / `RootKind`
- [ ] 1.2 反向图构建器：喂 (存活对象迭代 + trace_children) → `rev: HashMap<usize, Vec<RetainerInfo>>`
- [ ] 1.3 L1 `direct_referrers(rev, roots_direct, target) -> Vec<RetainerInfo>`
- [ ] 1.4 L2 `retaining_roots(rev, roots_of, target) -> Vec<RootInfo>`（反向 BFS + 根类别去重）

## 阶段 2: 分类根 scanner
- [ ] 2.1 `gc/heap.rs`：`RootKind` + `CategorizedRootScanner` 类型 + trait setter（默认 no-op）
- [ ] 2.2 `gc/arc_heap.rs`：`categorized_root_scanner` 字段 + setter
- [ ] 2.3 `vm_context.rs`：接线——按类别枚举 static_fields/帧/func_ref_slots/pinned

## 阶段 3: 查询接口
- [ ] 3.1 `gc/heap.rs`：MagrGC `retention_direct_referrers` / `retention_roots`（默认空）
- [ ] 3.2 `gc/arc_heap.rs`：impl——force_collect → iterate 两区建 rev + 分类根 → L1/L2
- [ ] 3.3 `gc/mod.rs`：`pub mod retention;`

## 阶段 4: builtins + stdlib
- [ ] 4.1 `corelib/diagnostics.rs`：`__heap_direct_referrers` / `__heap_retaining_roots`（target ptr → 查询 → 建 z42 数组）
- [ ] 4.2 `corelib/mod.rs`：`pub mod diagnostics;` + BUILTINS 追加
- [ ] 4.3 `Diagnostics/Retainer.z42` + `RootRef.z42`（+ `RootKind` enum）+ `Heap.z42`

## 阶段 5: 测试
- [ ] 5.1 `gc/retention_tests.rs`：反向图 / L1 / L2 / 浮动垃圾不报
- [ ] 5.2 `src/tests/reflection/heap_retention/`：e2e（object/array/static 链断言）
- [ ] 5.3 spec scenarios 逐条覆盖

## 阶段 6: 验证 + 文档
- [ ] 6.1 cargo build + Rust 单测
- [ ] 6.2 完整 `xtask test` 全绿（含自举 gen1==gen2）
- [ ] 6.3 book `heap-diagnostics.md`（NEW）+ SUMMARY + design load-context.md §5 对齐
- [ ] 6.4 归档 + PR

## 备注
- 决策：D1 按需堆扫描不建常驻注册表 · D2 触发 full GC 保准确 · D4 分类根 scanner（类别级，具体名延后）· L3 完整链条延后。
- 零回归铁律：分类根 scanner 仅诊断时调用，mark 热路径不变。
