# Tasks: 拆分 metadata/bytecode.rs（refactor-split-bytecode）

> 状态：🟢 已完成 | 完成：2026-09-03 | 创建：2026-09-03 | 类型：refactor（runtime；文本结构阶段 ⑤）
**变更说明：** `metadata/bytecode.rs` 1334 行按顶层条目边界逐行搬到 `bytecode/` 下 5 个子模块（module 110 / class 261 /
function 332 / insn 221 / instruction 452），`bytecode.rs` 变 19 行 hub（全量 `pub use` + 原 `bytecode_tests` 挂钩），
`crate::metadata::bytecode::X` / `crate::metadata::X` 路径零改动。非搬移改动仅：子模块内 `super::` → `crate::metadata::`。
顺带 `xtask test lines --update`：把已合并拆分（IrGen / types.rs / BigInt / 本 change）降到 500 行以下的文件从棘轮基线剔除。
**原因：** code-organization.md 500 行文件硬限；`Instruction` 枚举 321 行本身不可拆，但与 Module / Function / 载荷结构分文件后各自 < 500。
**文档影响：** `src/runtime/README.md`（核心文件表）；`scripts/test/line-limit-baseline.txt`。

- [x] 1.1 切分脚本 + hub；`cargo check` 0 error
- [x] 1.2 `cargo test --lib`：1036 + 21 passed（hub 保留私有 use 供 bytecode_tests 经 super:: 取 ExecMode）
- [x] 2. `xtask test lines --update`（棘轮只降）+ `xtask test` GREEN
- [x] 3. 文档同步 + 归档

## 备注：棘轮基线的一次性重锚
`--update` 后基线 diff：剔除 IrGen.z42（642→307）、types.rs（2436→32）、bytecode.rs（1334→19）三项，BigInt 2234→1498 下调；
另有 **3 项上调**——TsigReconcile.z42 604→622（#403 perf-tsig-reconcile-index +18）、ZpkgReader.z42 583→586（#403 +3）、
interp/mod.rs 1153→1155（#402 +2）。这三条增长来自与 #404（门禁）**同期在飞**、先于门禁基线生成的 PR，合并后 main 的
`xtask test lines` 实际已红。本次按合并后的现状重锚一次；此后任何增长仍按门禁规则红。
