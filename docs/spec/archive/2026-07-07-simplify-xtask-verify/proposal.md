# Proposal: 收敛 xtask 验证命令面（去 replace-csharp 时代冗余）

## Why

`xtask` 的验证命令面积累了 replace-csharp / pre-simplify 时代的冗余，命令数多而覆盖重叠：

1. **`test compiler-stdlib`** 是 replace-csharp S2.1 的 dogfood——加它是为了**证明 z42c（而非 C#）能编 stdlib**。C# 已于 2026-06-26 移除、z42c 是唯一编译器后，"z42c 编 stdlib" 就是 `build stdlib` 本身（`build sdk` / 每个 GREEN gate 的 regen 波 / CI 每条腿 ci-bootstrap 都在做）；它的隔离再编一遍 + emit 探针对 `build stdlib` + `test stdlib` 无独立覆盖。`docs/workflow/testing/bootstrap.md:172` 已把该 CI job 标记「覆盖已在 compile job + test 阶段，评估删」。
2. **`test packages-config` / `packages-staging` / `packages-assemble`** 是 packages.toml 流水线三层各一个自检，三个独立子命令，实为同一主题（打包系统自检）。
3. **`bootstrap-check`** 是顶层命令，语义上属于「验证」，应归 `test` 组。

> **实施发现（2026-07-07）**：原计划让 bootstrap-check 的双轨改走一句 `build --workspace --output-dir <flat>`（零拷贝）。实测**不可行**——编译器 7 成员有深度互依赖（z42c.ir 引用 z42c.core 类型），单一扁平 `--output-dir` 破坏兄弟包类型解析（两轨齐炸 `E0402: member access on non-class string`）。per-member `--output-dir` + runlibs 累积才是正确的隔离机制（`build compiler` 生产路径用无 `--output-dir` 的 `--workspace` 写 per-member canonical dist，但边界检查要隔离、不能污染 repo 构建）。故保留 per-member 累积；命令面仍收归 `test bootstrap`。**顺带修一个 pre-existing bug**：SDK 包布局把 z42vm 挪到 `bin/z42vm`，而 extract 检查仍找根目录 `nightly/z42vm` → 命令在 extract 阶段即死（早于任何编译）。

不做的话：命令面持续膨胀、覆盖重叠、新接手者要理解 4 个几乎等价的验证入口。

## What Changes

- **删 `test compiler-stdlib`**：删 `_testCompilerStdlib`、router 注册、dispatch 分支、CI `compiler-stdlib` job（`test-compiler-stdlib(linux-x64)`）、`.scratch/dogfood`。覆盖由 `build stdlib`（z42c 编 stdlib）+ `test stdlib`（功能）保住。
- **合 `packages-{config,staging,assemble}` → `test packages`**：一个子命令顺序跑三个自检（`_testPackagesConfig` / `_testPackagesStaging` / `_testPackagesAssemble` 三个函数保留，只收命令面 3→1）。
- **`bootstrap-check` → `test bootstrap`**：移到 `test` 组（删顶层命令）。`_bcRunWorkspace` 保留 per-member + runlibs（零拷贝方案实测破坏兄弟解析，见 Why）。修 nvm 路径 `nightly/z42vm`→`nightly/bin/z42vm`（pre-existing SDK 布局漂移 bug）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `scripts/xtask_cli.z42` | MODIFY | test router：删 compiler-stdlib / packages-{config,staging,assemble} 注册，加 `packages` + `bootstrap`；删顶层 `bootstrap-check`；dispatch 同步 |
| `scripts/build/xtask_compiler.z42` | MODIFY | 删 `_testCompilerStdlib`（166-234） |
| `scripts/build/xtask_bootstrap_check.z42` | MODIFY | 命令名/proc 标签对齐 `test bootstrap`；修 nvm `bin/z42vm` 路径（pre-existing bug）；`_bcRunWorkspace` 注释补「flat --output-dir 破坏兄弟解析」教训 |
| `scripts/package/xtask_test_packages.z42` | NEW | 新 `_testPackages()` 顺序跑三自检（合并 config/staging/assemble 调度） |
| `scripts/package/xtask_test_packages_config.z42` | MODIFY | 保留 `_testPackagesConfig`（被 `_testPackages` 调用） |
| `scripts/package/xtask_test_stage_components.z42` | MODIFY | 保留 `_testPackagesStaging`（被 `_testPackages` 调用） |
| `scripts/package/xtask_test_package_assemble.z42` | MODIFY | 保留 `_testPackagesAssemble`（被 `_testPackages` 调用） |
| `.github/workflows/ci.yml` | MODIFY | 删 `compiler-stdlib` job（785-818）；下游 needs / summary 引用同步；`bootstrap-check`→`test bootstrap` 注释 |
| `scripts/README.md` | MODIFY | 命令清单：bootstrap-check→test bootstrap；packages-*→packages |
| `docs/book/src/dev/build.md` | MODIFY | bootstrap-check→test bootstrap（命令名 + 机制页锚点）；零拷贝机制更新 |
| `docs/book/src/dev/packaging.md` | MODIFY | 自检命令 packages-*→packages |
| `docs/design/compiler/self-hosting.md` | MODIFY | 本地快门命令名 bootstrap-check→test bootstrap |
| `docs/workflow/ci.md` | MODIFY | 删 test-compiler-stdlib job 引用；bootstrap-check→test bootstrap |
| `docs/workflow/testing/bootstrap.md` | MODIFY | compiler-stdlib job 删除落地（P5 项）；命令名同步 |
| `docs/workflow/testing/verify-by-change.md` | MODIFY | bootstrap-check→test bootstrap；packages-*→packages |
| `docs/agent/rules/doc-system.md` | MODIFY | §5.1 示例 bootstrap-check 命名对齐（如需） |
| `docs/spec/changes/ACTIVE.md` | MODIFY | toolchain 锁登记 / 归档释放 |

**只读引用**：

- `scripts/build/xtask_compiler.z42` 的 `_buildCompilerViaZ42c` / `_testSelfHostByteIdentical` — 参考 `build --workspace` 调用范式（零拷贝改造对标）
- `scripts/xtask_cli.z42` 的 `_packageRouter` / `_testRouter` — 参考 router 注册范式

## Out of Scope

- 不动 `_testPackagesConfig` / `_testPackagesStaging` / `_testPackagesAssemble` 三个函数的内部断言逻辑（只收命令面，不动实现）。
- 不动 CI `bootstrap-no-csharp` / `verify-selfhost` 等其它 bootstrap 相关 job（只删 compiler-stdlib job）。
- 不动 `build stdlib` / `test stdlib` 本身。

## Open Questions

- 无（设计在会话中已与 User 逐条确认：三部分方向 + 零拷贝走 --workspace 更贴近真实冷启动）。
