# Tasks: refactor-reflection-split

> 状态：🟢 已完成 | 创建：2026-08-24 | 完成：2026-08-24

**变更说明：** 把 `src/runtime/src/corelib/reflection.rs`（2840 行，H2 超 500 行硬限 5.6×）
按职责拆成 `reflection/` 下 11 个 concern 子模块（全 <500），mod.rs 作薄 hub；顺带补 corelib
目录 README（review L3）。纯代码搬移，零行为变更、零格式 bump。

**原因：** `docs/runtime_review.md` H2（文件超 500 行硬限）+ L3（corelib 缺 README）；
`refactor-reflection-split`（review §四 顺序 #7）。

**文档影响：** `src/runtime/src/corelib/README.md`（NEW，六段制）；`docs/runtime_review.md`
（H2 reflection 行 + §四 #7 + §三 L3 标 done）。反射机制文档 `docs/design/language/reflection.md`
无需改（文件布局是内部组织，机制/行为未变）。

## 任务

- [x] 1.1 `git mv corelib/reflection.rs reflection/mod.rs` + `reflection_tests.rs → reflection/reflection_tests.rs`
- [x] 1.2 拆 11 concern 子模块（type_object / type_query / fields / methods / properties / attributes /
      generics / enums / invoke / accessors / module_load），全 <500 行（脚本 scratchpad/split_reflection.py）
- [x] 1.3 mod.rs = header + imports + 共享常量 + `mod X;` + 私有 re-glob（`use super::*` 互见）+
      `pub use self::X::*`（保留 `builtin_*` / `make_type_*` / `read_obj_slot` 对外接口）+ tests 声明
- [x] 1.4 子模块内私有 `fn`/`struct` 提升 `pub(super)`；`super::struct_reflect` / `super::convert`
      → `crate::corelib::struct_reflect` / `crate::corelib::convert`
- [x] 1.5 补 `src/runtime/src/corelib/README.md`（六段制，含 `reflection/` 逐文件表）—— review L3
- [x] 1.6 `docs/runtime_review.md`：H2 reflection 行 + §四 #7 + §三 L3 标 ✅ done
- [x] 2.1 GREEN：`cargo build --release` clean（0 error；3 warning 均 pre-existing）
- [x] 2.2 `cargo test --lib`：z42 1003 passed / 0 failed（含 17 个 reflection 单测经 `super::*` 通过）
      + z42-compression 21/0

## 备注

- **本地环境限制**：本机 z42vm 退出期挂起（见 memory `runtime-review-improvement-program` 恢复环境），
  完整 `xtask test` 跑不了 → 以 `cargo test --lib`（含 reflection_tests 全绿）+ PR CI 为门禁。
  本改动是纯 Rust 文件搬移、无格式 bump、无 z42 侧接口变化，cargo 全绿即高置信度正确。
- 技法沿用 arc_heap / zbc_reader / translate / vm_context 拆分：marker/line-range 脚本切片 +
  子模块 `use super::*` + 私有 `fn`→`pub(super)` + mod.rs 私有 re-glob 作 hub。
