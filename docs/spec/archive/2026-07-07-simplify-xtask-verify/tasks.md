# Tasks: 收敛 xtask 验证命令面

> 状态：🟢 已完成 | 创建：2026-07-07 | 完成：2026-07-07 | 类型：refactor（收敛命令面，去冗余）

**变更说明：** 删 `test compiler-stdlib`（+CI job）、合 `packages-{config,staging,assemble}`→`test packages`、`bootstrap-check`→`test bootstrap`（零拷贝走 --workspace）。
**原因：** 去 replace-csharp/pre-simplify 时代冗余，收敛验证命令面 4→更少，覆盖不减。
**文档影响：** README / build.md / packaging.md / self-hosting.md / ci.md / bootstrap.md / verify-by-change.md（命令名 + CI job 同步）。

## 进度概览
- [x] 阶段 1: 删 test compiler-stdlib（代码 + CI job）
- [x] 阶段 2: 合 packages-* → test packages
- [x] 阶段 3: bootstrap-check → test bootstrap（+修 nvm bin/ pre-existing bug；零拷贝实测回退）
- [x] 阶段 4: 文档同步
- [x] 阶段 5: GREEN 验证（✅ 全 stage 绿；test packages + test bootstrap 双轨本地验证）+ 归档

> **GREEN 验证结果**：`xtask test` ✅ 全 stage（vm + cross-zpkg + stdlib + compiler）；
> `test packages` PASS（三自检）；`test bootstrap` ✅ 双轨全过（nightly 无越界 + repo self-build OK）。
> **build test + regen 合并 + test 分类重构（vm→runtime + e2e）挪到后续独立 change**（与 golden/test 命令面紧耦合，同批做避免重复改动）。

## 阶段 1: 删 test compiler-stdlib
- [ ] 1.1 删 `scripts/build/xtask_compiler.z42` 的 `_testCompilerStdlib`（166-234）
- [ ] 1.2 `scripts/xtask_cli.z42`：删 router 注册（289-290）+ dispatch 分支（445）
- [ ] 1.3 `.github/workflows/ci.yml`：删 `compiler-stdlib` job（785-818）+ 下游 needs/summary 引用

## 阶段 2: 合 packages-* → test packages
- [ ] 2.1 新建 `scripts/package/xtask_test_packages.z42`：`_testPackages()` 顺序跑三自检
- [ ] 2.2 `scripts/xtask_cli.z42`：router 注册 3→1（`packages`）+ dispatch 分支 3→1

## 阶段 3: bootstrap-check → test bootstrap
- [x] 3.1 `scripts/xtask_cli.z42`：删顶层 `bootstrap-check`，注册 `test bootstrap` + dispatch
- [x] 3.2 `scripts/build/xtask_bootstrap_check.z42`：命令名/proc 标签对齐 `test bootstrap`
- [x] 3.3 修 pre-existing bug：nvm 路径 `nightly/z42vm`→`nightly/bin/z42vm`（SDK 布局把 z42vm 挪 bin/ → extract 阶段即死）
- [x] 3.4 零拷贝方案（`--workspace --output-dir` 双轨）实测破坏兄弟包类型解析（E0402），**回退**保留 per-member + runlibs；`_bcRunWorkspace` 注释补该教训

## 阶段 4: 文档同步
- [ ] 4.1 `scripts/README.md`：命令清单同步
- [ ] 4.2 `docs/book/src/dev/build.md`：命令名 + 零拷贝机制
- [ ] 4.3 `docs/book/src/dev/packaging.md`：packages-*→packages
- [ ] 4.4 `docs/design/compiler/self-hosting.md`：本地快门命令名
- [ ] 4.5 `docs/workflow/ci.md`：删 test-compiler-stdlib 引用 + 命令名
- [ ] 4.6 `docs/workflow/testing/bootstrap.md`：compiler-stdlib job 删除落地
- [ ] 4.7 `docs/workflow/testing/verify-by-change.md`：命令名同步

## 阶段 5: GREEN 验证 + 归档
- [ ] 5.1 `xtask build compiler`（编 xtask.zpkg 自身，验证脚本编译无误）
- [ ] 5.2 `xtask test bootstrap`（需 gh 登录；本地烟测零拷贝路径）
- [ ] 5.3 `xtask test packages`（三自检合一）
- [ ] 5.4 `xtask test`（完整 GREEN gate）
- [ ] 5.5 归档 + ACTIVE.md 释放 toolchain 锁 + commit + push + 观察 CI

## 备注
- 零拷贝对标 `_buildCompilerViaZ42c`：`build --workspace --release`，`Z42_LIBS=<nightly stdlib>`，`WorkingDirectory=src/compiler`（nightly track）；repo track 用 repo stdlib + repo driver。
