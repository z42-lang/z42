# Tasks: 文件行数硬上限 lint（add-line-count-lint）

> 状态：🟢 已完成 | 完成：2026-09-03 | 创建：2026-09-03 | 类型：test/docs + xtask（三面评审 L-10 / C-10 文本结构阶段第一项）
**变更说明：** 新 GREEN stage `xtask test lines`：扫 `src/` 下非测试 `.z42` / `.rs`，>500 行即越界；对照棘轮基线
`scripts/test/line-limit-baseline.txt`——新越界或比基线更长 → 红，基线内未增长 → warn；`--update` 重写基线（只降不升的约定）。
**原因：** code-organization.md 的 500 行硬限此前无机械执行，现存 48 个越界文件（最大 BigInt 2234 / types.rs 2436）；
拆分是后续文本结构阶段的工作，本 stage 先止住继续膨胀、并让每次拆分可以被基线记账。
**文档影响：** `docs/book/src/dev/test-gate.md`、`docs/workflow/testing/README.md`、`.claude/rules/code-organization.md`、`scripts/README.md`。

- [x] 1.1 `scripts/test/xtask_test_lines.z42`：`_testLines(update)` + 文件收集/排除（tests 目录、`_tests` 目录、`*_tests.rs` / `tests.rs`、bench、target、artifacts）+ 基线读写
- [x] 1.2 注册：`xtask_cli.z42`（子命令表 + 分发）、`xtask_test.z42` `_testAll` stage 7（skip 键 `lines`）
- [x] 1.3 生成基线（48 项）；`xtask test lines` 0.77 s 通过
- [x] 1.4 文档同步（test-gate / testing README / code-organization / scripts README）
- [x] 2. `xtask test` GREEN（含新 stage）
- [x] 3. 归档
