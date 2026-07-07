# Design: xtask test 分类重构

## Architecture

```
xtask test <sub>
├── runtime            cargo test src/runtime --test-threads=1   ← NEW（Rust VM 自身单测 + zbc/zpkg format 基线）
├── e2e [--dir <cat>] [--file <path>] [--mode interp|jit]        ← 合 vm + cross-zpkg
│     └─ 按 category 分派：
│          cross-zpkg  → _testCrossZpkgCore（多包 runner）
│          其余 golden → _runVmGoldens（单源 golden runner）
│     默认（无 --dir/--file）= 全跑
├── stdlib / compiler / packages / bootstrap / dist            （不变）
├── changed / platform / all                                   （all 组成更新）
└── (regen 命令删除；golden 资产改由 `build test`)
```

## Decisions

### Decision 1: `test e2e` 内部保留两套 harness，按 category 分派

**问题：** golden（单源）与 cross-zpkg（多包）是两套机制，合成一个命令后怎么组织。
**决定：** `_testE2e` 遍历 `src/tests/<cat>`：`cat == "cross-zpkg"` → `_testCrossZpkgCore`；否则 golden runner。`--dir <cat>` 只跑该类（cross-zpkg 也走这条）；`--file <path>` 定位单用例。默认全跑。两个 core 函数**不动**，只加一层分派 + 过滤。zbc-format/zpkg-format/errors/parse 仍按 `_isNonRunnableCat`/`_isNonRegenCat` 跳过（它们不是可跑 golden）。

### Decision 2: `test runtime` 硬编 `--test-threads=1`

**问题：** cargo test 并行 SIGSEGV（pre-existing 跨线程内存损坏 race，macOS 亦复现）。
**决定：** `_testRuntime` 固定 `cargo test --locked --manifest-path src/runtime/Cargo.toml -- --test-threads=1` + `RUST_MIN_STACK=8388608`——与 CI Windows 现用配置一致（736/736 通过）。race 本身不在本 change root-cause（ci.yml TODO 已登记，另立 issue）。

### Decision 3: `test runtime` **不进** gate；改每条 CI 腿单独一步（2026-07-07 修订）

**问题：** 原打算 `test runtime` 进 `_testAll` 补本地缺口。实测发现 `signal_handler_e2e`
的 crash-helper（raise SIGABRT/SIGSEGV…）在**信号受限沙箱**里不终止 → cargo test 挂死
（本地实测挂 1h51m）。若 runtime 在 `test all` 内，`xtask test` 在这类环境直接卡死。
**决定：** `test runtime` **移出 `_testAll`**，改为：
- **standalone 命令** `xtask test runtime`（按需本地跑，真机/CI 非沙箱下正常）
- **CI 每条 test-host 腿一步** `xtask test runtime`（gated on `changes.vm`）——达成 User 要的
  "全腿覆盖"，但不塞进会本地/沙箱跑的 gate。Windows 腿不跑 `test all`（goldens 在 linux/macos），
  故 Windows 先 `build test` 备 zbc_compat 的 golden 输入，再 `test runtime`。
- **`test changed`**：`src/runtime/` → `test runtime`。

本地 `xtask test` gate 回到 e2e + stdlib + compiler（不含 cargo test），沙箱可用、不挂。

### Decision 4: build test + regen 合并——`build test` 为存活命令

**问题：** `regen` 与 `build test` 都编 golden 资产。
**决定：** 删 `regen` **命令**（`_regen` wrapper + router + dispatch），保留 `_regenCore`（gate build wave `_regenForTest` 用）+ `_regenGolden`（`build test` 用）。`build test`（`_buildTest` = `_ensureToolchainDeps` build-if-missing + `_regenGolden`）成为唯一 golden 资产命令。format bump 场景（需 fresh stdlib）文档改 `build stdlib && build test`。CI 2 处 `regen --no-stdlib` → `build test`（CI warm 下 `_ensureToolchainDeps` ~no-op，等价）。

## Implementation Notes

- `test changed` 映射（`_classifyFile`）：`src/tests/cross-zpkg/` → `test e2e --dir cross-zpkg`；`src/tests/` → `test e2e`；`src/runtime/` → `test runtime`（+ `test e2e` 保留，VM 行为经 golden 覆盖）。
- `--dir`/`--file`：`--dir` 匹配 `src/tests/<cat>` 目录名；`--file` 取绝对/相对路径定位单个 source.z42（golden）或 cross-zpkg 用例目录。二者互斥，都空 = 全跑。
- CI `test-vm-jit` 腿命令 `test vm jit` → `test e2e --mode jit`（job key `vm-jit-consistency` 保留，仅命令变）。

## Testing Strategy

- 命令面 smoke：`test runtime`（cargo test 串行通过）/ `test e2e`（全跑）/ `test e2e --dir classes` / `test e2e --dir cross-zpkg` / `test e2e --file <one>` / `test e2e --mode jit` / `build test`（golden 重生）。
- 旧命令消失：`test vm` / `test cross-zpkg` / `regen` → 返回 2。
- 完整 GREEN：`xtask test`（现含 runtime + e2e）全 stage 绿。
- `test changed` dry-run 验映射。
