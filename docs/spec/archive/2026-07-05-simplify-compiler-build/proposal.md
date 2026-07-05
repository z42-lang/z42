# Proposal: 简化编译器构建 —— exe 自包含输出，消除拼接目录

## Why

`xtask build compiler` 的产物布局有一堆"潜规则"，用户/新接手者看不懂：

1. 输出落 `artifacts/build/z42c/`，名字 ≠ 源码目录 `src/compiler/`（stdlib 是 `libraries/` 镜像源码，compiler 不一致）。
2. 两个 scratch 目录 `selfbuild-runlibs/` 和 `dogfood/run-<profile>/` —— 手动执行根本不知道干嘛。二者本质相同：**把 `libraries/dist/`（stdlib）+ `z42c/<各 member>/dist`（编译器 7 包）两处已有产物，复制汇总到一个目录**，纯粹因为运行 `z42c.driver`（自己是 z42 程序）需要它的兄弟包 + stdlib 在同一个 `Z42_LIBS` 目录里。
3. `_buildCompilerViaZ42c` 手写 per-member topo 循环 + 累积（注释声称避开 `build --workspace` 的 "E0402 wrinkle"）。

用户定了两条**铁律**：① 编译输出目录只放编译产物，保持干净（需要中间态就复制到独立目录）；② env 尽量复用、不新增。

关键事实（本次 review 验证）：
- **z42vm 加载 zpkg 时，解析依赖的顺序就是 `[zpkg 自己所在目录, Z42_LIBS]`**（`src/runtime/src/main.rs:498-509` `search_dirs`）—— 和 .NET `bin/` 一致，**零运行时改动**即可支持"自包含 exe 输出"。
- `z42c build --workspace` 建 compiler **已可用且不动点成立**（本次实测 gen2/gen3 7 包逐字节 identical）；"E0402 wrinkle" 注释过时。
- publish（`restructure-publish-output-dirs`, 2026-06-19）**已经**为 exe 自动复制依赖 —— 本变更把它提前到 build 默认行为。

## What Changes

1. **`z42c build` 编 exe 项目时，默认把非标准库依赖（workspace 本地兄弟包，如 6 个 `z42c.*`；stdlib `z42.*` 不拷）复制进输出 dist** —— .NET 式自包含。lib 项目输出不变（仍只有自己 1 个 zpkg）。
2. **xtask 编译器自建**：手写循环 → 一句 `z42c build --workspace`。**删 `selfbuild-runlibs/`**。
3. **xtask 编 stdlib**：直接跑 `compiler/z42c.driver/dist/z42c.driver.zpkg`（已自带兄弟包），`Z42_LIBS=<stdlib dist>`。**删 `dogfood/`**。
4. **目录名**：`artifacts/build/z42c/` → `artifacts/build/compiler/`（镜像源码，与 `libraries/` 一致）。
5. **env 收拢（铁律 ②）**：删本会话新加的 `Z42C_DIR`；`Z42_TOOLCHAIN` 语义并入"SDK 根"（复用现有 `Z42_HOME`/`Z42_TOOLCHAIN`，不新增）；种子解析从单一 SDK 根派生。
6. 文档：`artifacts/build/` 布局 README + env 说明 + `self-hosting.md` 更新。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.driver/src/Main.z42` | MODIFY | ✅ 已落地：`_build` 写出 exe zpkg 后 `_bundleExeDeps` 复制非 stdlib 依赖（名不以 `z42.` 开头）到 dist（镜像 z42b `_pubBundleProjectDeps`） |
| `src/compiler/z42.workspace.toml` | MODIFY | `[workspace.build] output_dir` 的 `z42c` → `compiler` |
| `scripts/build/xtask_compiler.z42` | MODIFY | `_buildCompilerViaZ42c` → `z42c build --workspace`；删 selfbuild-runlibs；路径 z42c→compiler |
| `scripts/build/xtask_stdlib.z42` | MODIFY | 删 dogfood/run-；跑 driver 自身 dist；路径 |
| `scripts/common/xtask_common.z42` | MODIFY | `_ensureSeed`/`_seed*` 路径 z42c→compiler；env：删 `Z42C_DIR`、`Z42_TOOLCHAIN` 收进 SDK 根、派生 vm |
| `scripts/xtask_regen.z42` | MODIFY | `artifacts/build/z42c` → `compiler` |
| `scripts/xtask_bench.z42` | MODIFY | 同上 |
| `scripts/common/xtask_golden.z42` | MODIFY | 同上 |
| `scripts/build/xtask_compiler_e2e.z42` | MODIFY | 同上 |
| `scripts/build/xtask_bootstrap_check.z42` | MODIFY | 同上 |
| `scripts/test/xtask_test_lib.z42` | MODIFY | 同上 |
| `scripts/test/xtask_test_cross.z42` | MODIFY | 同上 |
| `scripts/test/xtask_test_platform.z42` | MODIFY | 同上 |
| `scripts/package/xtask_package_desktop.z42` | MODIFY | 同上（打包读 z42c 种子路径） |
| `scripts/README.md` | MODIFY | 冷启动/种子段路径 + 全局 env 段 |
| `.github/actions/ci-bootstrap/action.yml` | MODIFY | 种子 stage 目标路径 z42c→compiler；env（Z42_TOOLCHAIN 保持或改 Z42_HOME） |
| `docs/design/compiler/self-hosting.md` | MODIFY | 自建机制更新（--workspace + 自包含 exe，去 runlibs/dogfood 描述） |
| `docs/workflow/building/compiler.md` | MODIFY | 构建命令/产物路径更新 |
| `docs/book/src/dev/build.md` | MODIFY | 同步 |
| `artifacts/build/README.md` | NEW | 说明 compiler/ libraries/ runtime/ xtask/ 各子目录（gitignore 目录加说明文件？改放 docs） |
| `src/compiler/z42c.project/tests/<name>/` | NEW | exe 自包含输出单测（产物含非 stdlib 依赖；lib 不含） |

**Scope 扩展（阶段 3 改名/回归揭示，2026-07-05）**：
| `src/runtime/build.rs` | MODIFY | ✅ Phase2 删 dogfood 的回归修复：driver-home 从 `dogfood/run-release` 改指自包含 `compiler/z42c.driver/dist` + `Z42_LIBS=libraries/dist`（**新占 runtime 锁**） |
| `src/compiler/*/tests/*.z42.toml`（18 个）+ `src/compiler/README.md` | MODIFY | ✅ 测试项目 output_dir 及文档的 `build/z42c` → `build/compiler`（`manifest_tests.z42` 的 PathTemplate 测试数据保留） |

**Scope 扩展（阶段 4 折叠 Z42_HOME 的布局守卫，2026-07-05）**：User 裁决把 `Z42_TOOLCHAIN` 折进 `Z42_HOME`；因两者布局不同（managed `runtimes/` vs SDK `programs/`+`bin/`），消费端需加守卫防错位。
| `scripts/xtask_cli.z42` | MODIFY | ✅ `--toolchain` 设 `Z42_HOME`（原 `Z42_TOOLCHAIN`） |
| `scripts/common/xtask_common.z42` | MODIFY | ✅ `_toolchainDir` 读 `Z42_HOME`；`_activeVm`/`_z42vm` 加 `bin/z42vm` 布局守卫；`_seedSdkDir` 删冗余 `Z42_HOME` 块 |
| `scripts/test/xtask_test_vm.z42` | MODIFY | ✅ golden libs 改经 `_toolchainDir` + libs 存在守卫（原直读 `Z42_TOOLCHAIN`） |

**只读引用**：
- `src/runtime/src/main.rs`（`search_dirs` = entry dir + libs，理解解析顺序；不改）
- `docs/spec/archive/2026-06-19-restructure-publish-output-dirs/`（publish 已有的 exe 依赖复制，参考实现）

## Out of Scope

- **`Z42_LIBS` 多目录（原 C2）**：本方案靠"运行时已搜 entry 目录 + 自包含 exe 输出"达成同效，不需要改 `Z42_LIBS` 单→多。若未来仍需要，独立 change。
- **launcher 侧 `Z42_HOME`/`Z42_PORTABLE_VM`/`Z42VM` 三合一**（env 第 2 层）：跨 install 模式，深、独立 change；本变更只收拢 xtask/种子侧（本会话我新加的债）。
- **stdlib（libraries）构建布局**：已是干净的 per-member，不动。
- Problem 2（直接 `z42 build <单 member>.toml` 缺兄弟包）：暂不处理（用 `--workspace` 是正解，文档提示即可）。

## Open Questions

- [x] env 第 1 层：`--toolchain` 折进 `Z42_HOME`（User 裁决，2026-07-05）。因 `Z42_HOME`（managed）与 `Z42_TOOLCHAIN`（SDK）布局不同，消费端加"是否 SDK-toolchain 布局"守卫（`programs/`/`bin/z42vm` 存在）——不碰 launcher 层（那仍是独立 env 第 2 层 change）。CI `ci-bootstrap` 同步 `Z42_HOME=`。
- [ ] `artifacts/build/README.md` 放哪（artifacts 是 gitignore；说明文档是否改放 `docs/workflow/`）
