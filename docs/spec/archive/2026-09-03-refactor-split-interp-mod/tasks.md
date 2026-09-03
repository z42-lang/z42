# Tasks: 拆分 interp/mod.rs（refactor-split-interp-mod）

> 状态：🟢 已完成 | 完成：2026-09-03 | 创建：2026-09-03 | 类型：refactor（runtime；文本结构阶段 ⑥）
**变更说明：** `interp/mod.rs` 1155 行拆为 `entry.rs`（公开入口 / ExecOutcome / 静态初始化）、`frame.rs`（Frame + 池化、
ref 解引用/写回、行号解析）、`exec_support.rs`（exec_function 族、FrameGuard、OSR / 原生分流、异常事件、handler 查找、
ref 写回）；mod.rs 只留模块表与执行主循环 `exec_function_body`（302 行，热循环不动）。mod.rs 全量再导出，兄弟模块
`super::X` 路径与 crate 外 `interp::run*` 路径零改动。非搬移改动仅：子模块内对兄弟模块的裸引用加 `super::`、搬出的
私有 fn/struct/const 改 `pub(super)`。
**原因：** code-organization.md 500 行文件硬限；解释器入口 / 帧 / 主循环三类改动此前都撞同一文件。
**文档影响：** `src/runtime/src/interp/README.md`（核心文件表）；`scripts/test/line-limit-baseline.txt`（interp/mod.rs 剔除）。

- [x] 1.1 切分脚本 + hub；`cargo check` 0 error
- [x] 1.2 `cargo test --lib`：1036 + 21 passed
- [x] 2. `xtask test lines --update`（只降）+ `xtask test` GREEN + `e2e --mode jit`
- [x] 3. 文档同步 + 归档
