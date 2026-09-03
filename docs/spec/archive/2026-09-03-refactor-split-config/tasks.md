# Tasks: 拆分 runtime/config.rs（refactor-split-config）

> 状态：🟢 已完成 | 完成：2026-09-03 | 创建：2026-09-03 | 类型：refactor（runtime；文本结构阶段 ⑦）
**变更说明：** `config.rs` 1082 行：① 内联 `mod tests`（470 行）按 runtime-rust.md「测试独立文件」搬到 `config_tests.rs`
（`#[path]` 挂钩，`use super::*` 不变）；② `KnobSpec` + `KNOWN_KNOBS` 旋钮表 → `config/knobs.rs`；③ `parse_*` / toml 键映射
→ `config/parse.rs`（私有 fn 改 `pub(super)`）。hub 留 `RuntimeConfig` 本体 / Default / from_env / toml 加载 / 全局单例，
全量再导出，`crate::config::X` 路径零改动。
**原因：** code-organization.md 500 行文件硬限 + runtime-rust.md 测试分文件规则。
**文档影响：** `src/runtime/README.md`（核心文件表）；`scripts/test/line-limit-baseline.txt`（config.rs 剔除）。

- [x] 1.1 切分 + hub；`cargo check` 0 error；`cargo test --lib config` 52 passed
- [x] 2. `xtask test lines --update`（只降）+ `xtask test` GREEN
- [x] 3. 文档同步 + 归档
