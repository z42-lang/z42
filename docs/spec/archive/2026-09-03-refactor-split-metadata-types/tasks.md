# Tasks: 拆分 metadata/types.rs（refactor-split-metadata-types）

> 状态：🟢 已完成 | 完成：2026-09-03 | 创建：2026-09-03 | 类型：refactor（runtime；三面评审 V-10 / 文本结构阶段 ③）
**变更说明：** `metadata/types.rs` 2436 行 god file（Value + TypeDesc + ScriptObject + ArrayObj + 布局 + 编解码，55 处 unsafe）
按职责拆为 `types/` 下 9 个子模块（96–401 行），`types.rs` 变 32 行 hub，全量 `pub use` 使既有 `crate::metadata::types::X`
路径零改动（60 个引用文件不动）。代码**逐行搬移**，仅：子模块内 `super::` 改 `crate::metadata::`（父模块变了）、
array.rs 的 8 个私有 helper 改 `pub(super)`（供 array_access 用）、ArrayObj 340 行 impl 按访问/视图切两块（200 行类型限制）。
**原因：** code-organization.md 500 行文件硬限 + 200 行 impl 限；god file 让 GC / JIT / interp 三方改动都撞同一文件。
**文档影响：** `src/runtime/src/README.md`（核心文件表）；hub 文件头自带子模块表。

- [x] 1.1 切分脚本按顶层条目边界切 9 段；hub + `pub use`；`cargo check` 0 error
- [x] 1.2 `cargo test --lib`（含 types_tests）：1032 + 21 passed
- [x] 2. `xtask test` GREEN（runtime 变了：先 `xtask build runtime`）+ `xtask test e2e --mode jit`
- [x] 3. 文档同步 + 归档
