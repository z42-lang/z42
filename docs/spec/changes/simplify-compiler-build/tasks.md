# Tasks: 简化编译器构建

> 状态：🟡 进行中 | 创建：2026-07-05
> 占用子系统：`compiler` + `toolchain`（ACTIVE.md 已登记）

## 进度概览
- [x] 阶段 1: z42c build(exe) 复制非 stdlib 依赖（compiler）✅
- [x] 阶段 2: xtask 自建/编 stdlib 改自包含，删 selfbuild-runlibs + dogfood（toolchain）✅
- [x] 阶段 3: `artifacts/build/z42c` → `compiler` 全量改名（compiler+toolchain+**runtime**）✅
- [ ] 阶段 4: env 收拢（删 Z42C_DIR、Z42_TOOLCHAIN 语义收进 SDK 根）（toolchain）

## 阶段 3: 改名 z42c → compiler ✅
- [x] 3.1 `z42.workspace.toml` output_dir → compiler
- [x] 3.2 全部 scripts/ 引用（14 文件）+ 18 个 compiler 测试 toml + src/compiler/README（sed；manifest_tests.z42 的 PathTemplate 测试数据保留 build/z42c）
- [x] 3.3 CI ci-bootstrap 种子 stage 路径
- [x] 3.4 **build.rs 回归修复**（Phase 2 删 dogfood 后，`src/runtime/build.rs` 仍引 `dogfood/run-release` → C7/embedding e2e fixture 静默跳过）：改指自包含 driver `compiler/z42c.driver/dist` + `Z42_LIBS=libraries/dist`（新占 **runtime** 锁）
- [x] 3.5 验证 ✅：cold build compiler 落 `artifacts/build/compiler/`（7 member），无旧 z42c 目录；build.rs 路径修正（cargo/e2e 由 CI 验）
- [ ] 阶段 5: 文档

## 阶段 1: z42c build(exe) 自包含（compiler）✅
- [x] 1.1 定位落点：`z42c.driver/src/Main.z42` `_build` 写出 exe zpkg 后（非 z42c.project）；依赖分类 = 名不以 `z42.` 开头 = 非 stdlib
- [x] 1.2 `_bundleExeDeps`（镜像 z42b `_pubBundleProjectDeps`）：kind=exe 时把非 stdlib 直接依赖的 zpkg（+.zsym）从 libsDirs 复制到输出 dist；stdlib（z42.*）不拷
- [x] 1.4 验证 ✅：新 z42c build z42c.driver → dist 含 6 个 z42c.*（不含 z42.io）；`Z42_LIBS=stdlib` 跑 bundle 后 driver 从自身目录解析兄弟包成功（未 bundle 对照 undefined）；gen2/gen3 字节不动点 7/7 identical
- [ ] 1.3 自动化 e2e 断言 —— 折入阶段 2（xtask 全线用新 z42c 后，`_testCompilerE2e` 断言 z42c.driver/dist 含兄弟包 + 自包含跑通）

## 阶段 2: xtask 去 scratch 目录（toolchain）✅
- [x] 2.1 `_buildCompilerViaZ42c` → 一句 `z42c build --workspace`（CWD=src/compiler；driver 自包含直跑，`Z42_LIBS=stdlibFlat`）+ `_ensureDriverSelfContained`（把兄弟包 bundle 进 driver dist，对种子/旧版 z42c 兜底）
- [x] 2.2 **`selfbuild-runlibs/` 彻底删除**（--workspace 内部解析兄弟）
- [x] 2.3 `_buildStdlibCore`：**删 `dogfood/`**；直接跑自包含 `z42c.driver`；`dogfood` 瘦成 `.stdlib-run` **只 stdlib 快照**（driver 运行期 Std.* 需稳定副本，因 stdlib 正被重建）——隔离在编译输出**外**
- [x] 2.4 验证 ✅：cold build compiler 无 selfbuild-runlibs + driver dist 自包含；build stdlib 22/22 无 dogfood；`artifacts/build/z42c/` 仅 7 member 目录；gen2/gen3 不动点 7/7 identical
- [ ] 2.5 （折自 1.3）exe-bundle 自动化 e2e 断言 —— 待补：`_testCompilerE2e` 断言 z42c.driver/dist 含兄弟包
- 备注：`.stdlib-run`（stdlib 快照）未完全消除——driver 编 stdlib 时自身 Std.* 需稳定副本。已从 `{stdlib+z42c}` 瘦身为只 stdlib、移出编译输出目录、命名自解释。完全去除需 driver 不依赖被重建的 stdlib（更深，记后续可选）。

## 阶段 3: 目录改名 z42c → compiler（compiler+toolchain）
- [ ] 3.1 `src/compiler/z42.workspace.toml` `[workspace.build] output_dir` z42c→compiler
- [ ] 3.2 全部 scripts/ 引用改名（xtask_compiler/stdlib/common/regen/bench/golden/compiler_e2e/bootstrap_check/test_lib/test_cross/test_platform/package_desktop）
- [ ] 3.3 `.github/actions/ci-bootstrap/action.yml` 种子 stage 目标路径
- [ ] 3.4 验证：`rm -rf artifacts/build && xtask build compiler`（cold 供种 + --workspace）→ 落 compiler/；`xtask test` 全绿

## 阶段 4: env 收拢（toolchain）—— 决策：折叠进 Z42_HOME（选项 b，带布局守卫）
> **事实校正落点**：`Z42_HOME`（managed 安装：`runtimes/`+`config.toml`）与 `Z42_TOOLCHAIN`
> （.z42 SDK：`programs/`+`libs/`+`bin/z42vm`）布局不同。User 裁决仍折进 `Z42_HOME`，
> 但为避免 `_activeVm`/`_z42vm` 对 managed 布局 `Z42_HOME` 解析错位，**消费端加布局守卫**
> （`programs/` 或 `bin/z42vm` 存在才当 SDK-toolchain 根，否则回退 build-tree）。不碰
> launcher/apphost/install（那仍是独立的 env 第 2 层 change）。
- [x] 4.1 `xtask_common.z42`：删 `_seedDriverHome` 的 `Z42C_DIR` 分支；种子 z42c 从 `_seedSdkDir()/programs/z42c` 派生 ✅
- [x] 4.2 **`Z42_TOOLCHAIN` 折进 `Z42_HOME`**：`_toolchainDir` 读 `Z42_HOME`；`--toolchain` 设 `Z42_HOME`（`xtask_cli._applyToolchainOpt`）；`_seedSdkDir` 删冗余 `Z42_HOME` 块（`_toolchainDir` 已覆盖）✅
- [x] 4.2b **布局守卫（扩展 Scope）**：`_activeVm` 仅当 `<tc>/bin/z42vm` 存在才用工具链 vm；`_z42vm` 探 `<home>/bin/z42vm`（SDK）再 `launcher/z42vm`（managed）；`xtask_test_vm` golden libs 改经 `_toolchainDir` + libs 存在守卫 ✅
- [x] 4.3 **detailed 日志打印源头**：`_seedSdkDir` 逐候选打印 `sdk: Z42_HOME/--toolchain → …`；`seed: z42c ← <path>`（normal）✅
- [x] 4.4 CI `ci-bootstrap`：`Z42_TOOLCHAIN=` → `Z42_HOME=`（步骤 3/4 + 注释；解析等价，仅换 var 名）✅
- [x] 4.5 scripts/README env 段更新 ✅
- [x] 4.6b **根因修复 `_copyAll` self-copy**（验证期发现）：`File.Copy(X,X)` 先截断 dst 再读空 src → 把文件清零。当 caller 的 src/dst 同目录（`_ensureSeed` 在 `Z42_LIBS` 已指向 flat dist 时 stage stdlib）→ 整个 stdlib dist 被清零。`_copyAll` 加 `hits[i] != dstPath` 守卫（保护全部 11 个调用点）✅
- [ ] 4.6 验证：warm/cold build compiler + `xtask test` 全绿（本地）+ CI ci-bootstrap 绿

## 阶段 2 回归修复：自举不动点 gen2 改走 --workspace（验证期发现，2026-07-05）
> Phase 2 把 `build compiler`（gen1）切到 `z42c build --workspace`，但不动点测试
> `_testSelfHostByteIdentical` 的 gen2 仍走旧的逐包 `build <toml> --output-dir` +
> `Z42_LIBS=flat`（胖目录）。二者分歧：单包胖-flat 构建拉入更大 + 非确定的依赖闭包
> （目录扫描顺序，common-pitfalls §1）→ gen2>gen1 且逐次漂移 → **Phase 3 CI（d18fa09c）
> verify-selfhost + test-host 全红**。User 裁决（2026-07-05）：gen2 改走 --workspace。
- [x] R.1 `_testSelfHostByteIdentical` 重写：快照 gen1（canonical dist）→ 用 gen1 的自包含
  driver 再跑 `build --workspace`（与 `_buildCompilerViaZ42c` 同参）→ 覆盖 canonical dist =
  gen2 → 逐段比对。gen1/gen2 同路径 → 真正「driver 复现自身」不动点。签名去掉 `flat` 参数。
- [ ] R.2 验证：本地 `xtask test compiler` 不动点 7/7 绿 + CI verify-selfhost/test-host 绿

## 阶段 4 备注 / 后续
- **`_testCompilerStdlib`（`xtask test compiler-stdlib`，CI-only stage）仍建 `artifacts/build/compiler/dogfood/{run,stdlib,verify}-release` scratch 目录**——与 Phase 2 删的 `_buildStdlibCore` dogfood 是不同函数。非本次 GREEN 失败诱因；清理需把它改自包含（同 Phase 2 手法），工作量独立 → 记为后续（待 User 定是否并入本 change）。

## 阶段 5: 文档
- [ ] 5.1 `docs/design/compiler/self-hosting.md`：自建改 --workspace + 自包含 exe，删 runlibs/dogfood 描述
- [ ] 5.2 `docs/workflow/building/compiler.md` + `docs/book/src/dev/build.md` 产物路径/命令
- [ ] 5.3 `artifacts/build/` 布局说明（放 docs/workflow 或 scripts/README）
- [ ] 5.4 ACTIVE.md 释放 compiler + toolchain 锁；归档

## 备注
- 运行时零改动（依赖既有 `search_dirs = [entry dir, libs]`）。
- 「E0402 wrinkle」注释过时，阶段 2 顺带删。
- Problem 2（单 member 直接 build 缺兄弟包）Out of Scope，文档提示用 `--workspace`。
