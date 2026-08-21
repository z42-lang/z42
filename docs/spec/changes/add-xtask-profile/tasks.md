# Tasks: add-xtask-profile（脚本性能分析 P0）

> 状态：🟡 进行中 | 创建：2026-08-22
> 类型：toolchain（轻量变更，直接 IMPL → GREEN → PR）。属 `plan-script-profiling` 程序的 **P0** 阶段。

**变更说明：** 新增 `xtask profile <script.z42>` 统一入口，集成外部 profiler（samply CPU / dhat heap）+
封装现成 knob（hyperfine wall-clock + peak-RSS + counter stats），并把 z42vm `--print-stats-on-exit`
输出结构化为 JSON（`--stats-format=json`）便于汇总。

**原因：** e2e 只有 wall-clock、无 RSS；零外部 profiler 集成；stats 只有人读文本。P0 零/轻 VM 改动即得
「对任意 .z42 出 CPU 火焰图 + 堆报告 + peak-RSS + counter 摘要」。

**文档影响：** `scripts/README.md`（功能索引 + 基础用法）；`plan-script-profiling/proposal.md` OQ3 已裁决。

## Scope（允许改动的文件）

| 文件 | 类型 | 说明 |
|------|------|------|
| `scripts/xtask_profile.z42` | NEW | `xtask profile` 实现（`_profile` + 四维 handler + report 汇总） |
| `scripts/xtask_cli.z42` | MODIFY | `_cliRoot()` 注册 `profile`；`_dispatch()` 路由 |
| `src/runtime/src/counters.rs` | MODIFY | `Snapshot::to_json()` + 单测 |
| `src/runtime/src/app.rs` | MODIFY | `RunOpts.stats_json`；print_stats 分支 JSON |
| `src/runtime/src/main.rs` | MODIFY | `--stats-format` clap 枚举 + plumb；dhat 全局 allocator 门控（`dhat-heap`）+ Profiler |
| `src/runtime/Cargo.toml` | MODIFY | optional `dhat` dep + `dhat-heap` feature |
| `src/runtime/src/{host/mod.rs,corelib/reflection.rs}` + `crates/z42-host/src/lib.rs` | MODIFY | RunOpts 构造点补 `stats_json: false` |
| `src/toolchain/workload/wasm/platform/src/lib.rs` | MODIFY | RunOpts 构造点补 `stats_json: false`（CI 揭示的第 5 个 caller；不在默认 workspace build，`cargo build`/`verify-features` 不覆盖，仅 `package-wasm` 编译） |
| `scripts/README.md` | MODIFY | 功能索引 + 基础用法 |

只读引用：`scripts/xtask_bench.z42`（compile/exec 模式参照）、`scripts/common/xtask_common.z42`（`_have`/`_exec`/`_builtVm`/`_assembleAllLibs` 等）。

## 阶段 1: VM 侧（JSON stats + dhat feature）
- [x] 1.1 `counters.rs`：`Snapshot::to_json()` 单行 JSON + 单测（`to_json_is_single_line_with_all_fields`）
- [x] 1.2 `app.rs`：`RunOpts.stats_json`；`print_stats` 时按 format 选 Display / JSON
- [x] 1.3 `main.rs`：`--stats-format text|json`（clap ValueEnum，default text）→ RunOpts
- [x] 1.4 `Cargo.toml`：optional `dhat` + `dhat-heap` feature
- [x] 1.5 `main.rs`：`dhat-heap` 全局 allocator（与 `mimalloc-alloc` 互斥 cfg）+ `dhat::Profiler::new_heap()`
- [x] 1.6 `cargo build --release`（默认）✓ + `cargo build --release --features dhat-heap` ✓（隔离 target-dir）

## 阶段 2: toolchain 侧（xtask profile）
- [x] 2.1 `xtask_profile.z42`：`_profile(ParseResult)` — 编译脚本 + 分维度分派 + `--all` report
- [x] 2.2 `--e2e`：hyperfine wall-clock + `/usr/bin/time` peak-RSS + counter JSON 摘要
- [x] 2.3 `--cpu`：samply（缺则 install hint + skip，不崩）
- [x] 2.4 `--heap`：独立 target-dir 建 dhat-heap VM → 跑 → 收 `dhat-heap.json` + counter 摘要
- [x] 2.5 `--threads`：counter JSON 摘要 +（有 samply 时）分线程时间线提示
- [x] 2.6 `xtask_cli.z42`：注册 `profile` 命令树 + dispatch

## 阶段 3: 验证
- [x] 3.1 rebuild xtask.zpkg（种子 z42c 编译 51 文件无错——z42 源合法）+ `cargo build`（z42vm）✓
- [x] 3.2 `xtask -h` 列出 profile + `xtask profile -h` 渲染全部 flags（CLI 接线正确）
- [x] 3.3 `cargo test --lib counters`：7 pass（含新 `to_json`）；dhat-heap 构建通过（dhat dep + allocator 互斥 + Profiler API）
- [ ] 3.4 端到端 `xtask profile --e2e/--all` 实跑 + 完整 `xtask test` GREEN —— **本地被 #240 zbc 格式-bump 两代自举墙挡住**（见备注），CI 权威
- [x] 3.5 文档同步（scripts/README 命令表 + 源码结构；plan OQ3 裁决）

## 备注（格式-bump 两代自举墙 —— 必读）
- **本机 seed 是 0.40（Aug-15 nightly，pre-#240），origin/main 源已是 zpkg 0.41**（#240 泛型方法
  bump 格式）。本 change 的 cargo z42vm = 0.41 writer，读 0.40 seed 产的 z42c/stdlib 时 strict-pin
  拒绝 → `build stdlib` self-build 失败（`|tail` 曾吞退出码，实际非 0）。这是 **pre-existing 环境墙，
  与本 change 无关**（本 change 纯增量、不碰 zbc/zpkg 格式）。
- **可用 seed 都不合格**：sibling `.z42` 要么仍 0.40，要么 **ahead of origin/main**（z42-genreflect
  的 z42c emit `__ctor_invoke`——origin/main VM 没有的 builtin）。合格 seed = 发布 nightly（≥0.41 且
  ≤origin/main），或 ci-bootstrap 的两代自举。
- **本地已验（格式无关面，全绿）**：① cargo default build ② cargo `--features dhat-heap` build
  ③ `cargo test --lib counters`（含 to_json）④ xtask.zpkg 编译（0.40+0.41 两种 seed 各编一次，均无
  z42 语法/类型错）⑤ `xtask -h`/`xtask profile -h` 接线。
- **CI 权威**（bootstrap-seed.md：格式-bump 的 build-and-test 路径 CI 两代自举自动过）跑
  `xtask profile` 实跑 + 完整 GREEN。本 change 不动格式 → CI 现有两代自举照常处理 0.41，增量代码已本地
  编译通过。
- samply/dhat CLI 本机未装：`--cpu` 走 install-hint 分支（不崩）；`--heap` 靠 Cargo `dhat-heap`
  feature（非 CLI），已实测构建通过。dhat build 用独立 `--target-dir`，不覆盖发布 VM。
</content>
</invoke>
