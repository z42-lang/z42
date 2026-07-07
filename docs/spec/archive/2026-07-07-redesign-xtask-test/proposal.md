# Proposal: 重构 xtask test 分类（e2e 统一 + runtime 补齐 + golden 资产合并）

## Why

`xtask test` 的子命令分类与"测的是什么"不对齐，且有覆盖缺口：

1. **`test vm` 名不副实**：它跑的是 `src/tests/` 单源 golden（源码 → z42c → zbc → z42vm → 比对 stdout），测的是**整条编译→执行 pipeline**，不是"VM 自己的测试"。名字误导。
2. **`test cross-zpkg` 与 `test vm` 割裂**：两者都在跑 `src/tests/` 下的用例，只是机制不同（单源 golden vs 多包 e2e）。用户要跑"tests 目录下的用例"却要记两个命令，且无法按子目录/单文件 narrow。
3. **Rust VM 单测（cargo test）没有命令名、不在本地 gate**：`cargo test`（VM 自身 736 单测 + zbc/zpkg format 基线校验）今天只在 CI Windows 腿跑、本地 `xtask test` **根本不跑**——本地 gate 有真实覆盖缺口。
4. **`regen` 与 `build test` 重叠**：两个命令都以 `_regenGolden` 结尾编 golden 资产（见 simplify-xtask-verify 归档记录 + [ci.md](../../../workflow/ci.md) 的 `build test-assets` 意图）。

## What Changes

- **`test vm` + `test cross-zpkg` → `test e2e`**：统一 `src/tests/` 端到端用例。默认全跑（单源 golden 各类 + cross-zpkg）；`--dir <cat>` / `--file <path>` 子选择；`--mode interp|jit` 保留。内部两套 harness（golden runner / 多包 runner）保留为函数，`test e2e` 按 category 分派。
- **新增 `test runtime`**：`cargo test --locked --manifest-path src/runtime/Cargo.toml -- --test-threads=1`（+ `RUST_MIN_STACK=8MB`）——Rust VM 自身单测/集成，含 zbc/zpkg format 基线校验（`zbc_compat` / `lazy_loader`）。**`--test-threads=1` 是硬约束**：VM `--lib` 单测有 pre-existing 跨线程内存损坏 race，并行会 SIGSEGV（macOS 亦可复现），串行 736/736 通过。加入默认 gate（补本地缺口）。
- **build test + regen 合并**：删 `regen` 命令（router + `_regen` wrapper + dispatch），保留 `_regenCore`/`_regenGolden` 函数（gate build wave `_regenForTest` + `build test` 仍用）。`build test` 成为唯一 golden 资产命令。CI 2 处 `regen --no-stdlib` + 文档迁 `build test`。
- **删 `audit` 命令**（顺带清鸡肋，User 裁决 2026-07-07）：`xtask_audit.z42`（224 行正则启发式补 `using`）零耦合（不在 gate/CI/build，无其他调用）、价值低（z42c strict-using 报错已精确点名缺失 using，手动补零成本）。删文件 + router + dispatch + 文档；补 using 指引改「按 z42c 报错手动补」。
- **文件归位**（命令折叠后的文件名/目录对齐；纯移动，flat namespace 零调用点改动）：
  - `scripts/xtask_regen.z42` → `scripts/build/xtask_test_assets.z42`（`regen` 命令并入 `build test` → 归 build/）
  - `scripts/xtask_release.z42` → `scripts/package/xtask_release.z42`（`release` 命令并入 `package`（merge-package-release 遗留）→ 归 package/；顺带修其内 stale `xtask release …` usage 串 → `xtask package workload/index`）

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `scripts/xtask_cli.z42` | MODIFY | test router：`vm`+`cross-zpkg`→`e2e`（+`--dir`/`--file`），加 `runtime`；删顶层 `regen` + `audit`；dispatch 全同步 |
| `scripts/xtask_audit.z42` | DELETE | `audit` 命令（正则补 using，鸡肋，零耦合） |
| `docs/design/language/namespace-using.md` | MODIFY | 删 `xtask audit` 迁移工具引用 → 改「按 z42c 报错手动补 using」 |
| `scripts/test/xtask_test.z42` | MODIFY | `_testVm`/`_testVmCore`→`_testE2e`/`_testE2eCore`（+dir/file 过滤 + 合入 cross-zpkg 分派）；新增 `_testRuntime`；`_testAll` gate 组成加 runtime |
| `scripts/test/xtask_test_cross.z42` | MODIFY | `_testCrossZpkgCore` 保留为函数，由 `_testE2e` 在 `--dir cross-zpkg`/默认全跑时调用 |
| `scripts/test/xtask_test_changed.z42` | MODIFY | 映射：`src/tests/cross-zpkg/`→`test e2e --dir cross-zpkg`；`src/tests/`→`test e2e`；`src/runtime/`→`test runtime`（+`test e2e`）|
| `scripts/xtask.z42` | MODIFY | 删 `_regen` wrapper（`_regenCore`/`_regenForTest` 保留）；命令树注释同步 |
| `.github/workflows/ci.yml` | MODIFY | Windows cargo-test step → `xtask test runtime`（或删，gate 覆盖）；`test vm jit` 腿→`test e2e --mode jit`；`regen --no-stdlib`×2→`build test`；job 注释同步 |
| `scripts/README.md` | MODIFY | 命令树/流程图/命令清单刷新 |
| `docs/book/src/dev/xtask.md` | MODIFY | test 章节：新分类表（runtime/e2e/…）+ `--dir`/`--file` 用法 |
| `docs/book/src/dev/build.md` | MODIFY | GREEN gate 步骤表命令名（vm→e2e，加 runtime）；regen→build test |
| `docs/workflow/testing/vm-tests.md` | MODIFY | 命令名 vm→e2e；`./xtask regen`→`build test` |
| `docs/workflow/testing/verify-by-change.md` | MODIFY | 改动类型→命令映射（runtime/e2e） |
| `docs/workflow/testing/README.md` | MODIFY | scope 说明如涉及 |
| `docs/workflow/ci.md` | MODIFY | job→阶段映射命令名；`build test-assets` 意图落地（regen 并入 build test） |
| `docs/design/testing/testing.md` | MODIFY | `./xtask regen`→`build test`；命令名 |
| `docs/design/runtime/zbc.md` / `compiler-architecture.md` | MODIFY | `./xtask regen`→`build test`（strict-pin 恢复命令名） |
| `.claude/rules/version-bumping.md` | MODIFY | `xtask regen`→`build stdlib && build test`（fixture 重生命令）|
| `docs/spec/changes/ACTIVE.md` | MODIFY | 锁登记/释放 |

**只读引用**：`scripts/test/xtask_test_vm.z42`（golden runner 内部，`_runVmGoldens`）、`scripts/common/xtask_golden.z42`（`_isNonRunnableCat` 等枚举 helper）、`scripts/xtask_regen.z42`（`_regenGolden`/`_isNonRegenCat`）。

## Out of Scope

- 不动 golden runner / 多包 runner 的内部执行逻辑（只改命令面 + 分派 + 过滤）。
- 不 root-cause 修 cargo test 的跨线程 race（用 `--test-threads=1` 现有 stopgap；race 本身另立 issue，见 ci.yml TODO）。
- 不动 `test stdlib` / `test compiler` / `test packages` / `test bootstrap` / `test dist`。
- zbc-format/zpkg-format 归 `test runtime`（Rust 校验），不搬进 `test e2e`。

## Open Questions

- [ ] **`test runtime` 是否进默认 gate + CI 全腿**（推荐：进本地 gate 补缺口；CI 删 Windows-only cargo step 让 gate 全腿覆盖 → +3 腿串行 cargo test ~1-2min/腿，换跨平台 VM 单测覆盖）。若 CI 时间敏感 → 备选：runtime 仅本地 gate + CI 单腿。**待 User 拍。**
