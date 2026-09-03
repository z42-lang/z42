# Tasks: 拆分 gc/var_region.rs（refactor-split-var-region）

> 状态：🟢 已完成 | 完成：2026-09-03 | 创建：2026-09-03 | 类型：refactor（runtime；文本结构阶段 ⑨）
**变更说明：** `gc/var_region.rs` 1105 行，四个互相独立的关注点挤在一个文件里。按关注点搬到
`var_region/block.rs`（`BlockType` / `GcBlockHeader` / `payload_ptr_of` / `PayloadDropGlue`）、
`var_region/chunk.rs`（尺寸类常量 + `class_for` + `Chunk` + `VarChunkClaim` + `VarRegion` 的
chunk 增长/借出/退还/回收方法）、`var_region/var_ref.rs`（`VarGcRef` 句柄）；父文件留模块文档 +
`VarRegion` 本体（alloc / resolve / tombstone / sweep / Drop）+ `pub use` 再导出，
`crate::gc::var_region::{BlockType, GcBlockHeader, VarGcRef, VarRegion, VarChunkClaim, class_for,
OVERSIZED_CLASS}` 路径零改动。代码逐行搬移，仅补 `use` 与放宽被跨模块引用项的可见性
（`pub(super)` / `pub(crate)`），无逻辑改动。
**原因：** code-organization.md 500 行文件硬限。
**文档影响：** `src/runtime/src/gc/README.md`（var_region 行拆成四行）；
`scripts/test/line-limit-baseline.txt`（var_region.rs 剔除）。

- [x] 1.1 切分 + 父模块 re-export；`cargo build --release` 0 error；`cargo test --lib gc::` 266 passed
- [x] 2. `xtask test lines --update`（只降，剔除 var_region.rs）+ `xtask test` GREEN（全 stage 通过）
- [x] 3. 文档同步（gc/README.md + docs/workflow/testing/README.md 的 lines job 归属修正）
