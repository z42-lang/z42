# Tasks: refactor-arc-heap-modularization

> 状态：🟢 已完成 | 创建：2026-08-23 | 完成：2026-08-23 | 类型：refactor（最小化模式）

**变更说明：** 把 `src/runtime/src/gc/arc_heap.rs`（2734 行，超 500 硬限 5.5×）按职责拆成
协调器 + 6 个 concern 子模块 + trait 接口薄层 + 调试/测试辅助层，每文件 <500 行。纯代码搬移，
零行为变更。

**原因：** code-organization.md 500 行硬限；runtime_review.md H2（#2 项）。arc_heap.rs 是 27 个
超限文件里最大的之一，god-file 阻碍 review 与维护。

**文档影响：** `src/runtime/src/gc/README.md`（核心文件表新增子模块）。无 book 机制页变更（纯
内部结构，行为/算法不变）。

## 拆分方案（Rust 约束：单个 trait impl 块不可跨文件 → 重方法体下沉为 inherent，trait impl 保留薄委托）

- `arc_heap.rs`（core）：模块 doc、imports、`HandleEntry`/`HandleSlab`、`RcHeapInner`、类型别名、
  `ContextReclaim` trait、`value_heap_ptr`、`var_drop_glue`、`ArcMagrGC` struct、`Default`/`Debug`、
  `BarrierEvent`/`BarrierObserver`（tests 经 `crate::gc::arc_heap::` 引用，须留 core）、`new()`、mod 声明。
- `arc_heap/alloc.rs`：分配尾部 + OOM + 压力 + size 估算 + `object_size_bytes`。
- `arc_heap/collect.rs`：mark/sweep/cycle-collection/finalizer/soft-ref + `collect_cycles*`/`force_collect`。
- `arc_heap/generational.rs`：minor/major/promotion/card/gen_age + write barriers。
- `arc_heap/roots.rs`：`snapshot_roots_into_mark_queue`? 归 collect；roots 放 `scan_marked_contexts` +
  `build_retention_graph`。
- `arc_heap/observe.rs`：barrier fire/observer + `fire_event`/`now_us`/`type_name_of` + `take_snapshot`/`stats`。
- `arc_heap/interface.rs`：`impl MagrGC for ArcMagrGC`（薄委托）。
- `arc_heap/debug.rs`：`#[cfg(test)]`/`#[cfg(debug_assertions)]` 辅助（test accessors + `debug_validate_invariants`）。

## 进度概览
- [x] 1. 生成 9 个文件（coordinator + alloc/collect/control/generational/roots/observe/interface/debug）
- [x] 2. cargo build 通过（`self.mode()` 需 MagrGC in scope → control/generational 加 import；清 unused）
- [x] 3. 每文件 <500 行核对（458/345/334/413/232/346/424/141/143）
- [x] 4. README 同步（核心文件表列全 9 文件）
- [x] 5. GREEN（cargo --lib 941+21/0 + gc:: 247/0 + 集成编译 + `xtask test all` ✅ GREEN 全 stage 通过 C#-free：e2e interp + cross-zpkg + stdlib + 自举 5/5 gen1==gen2 + vscode-syntax）

## 备注
- 关键约束：子模块私有 inherent 方法对 sibling 子模块不可见 → 跨模块调用的 inherent 方法一律
  `pub(super)`（在 arc_heap 子树内可见，外部 test 走 trait 委托）。
- `super::X`（原指 `gc::X`）在下沉代码里改写为 `crate::gc::X`。
