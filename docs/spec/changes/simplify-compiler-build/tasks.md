# Tasks: 简化编译器构建

> 状态：🟡 进行中 | 创建：2026-07-05
> 占用子系统：`compiler` + `toolchain`（ACTIVE.md 已登记）

## 进度概览
- [x] 阶段 1: z42c build(exe) 复制非 stdlib 依赖（compiler）✅
- [ ] 阶段 2: xtask 自建/编 stdlib 改自包含，删 selfbuild-runlibs + dogfood（toolchain）
- [ ] 阶段 3: `artifacts/build/z42c` → `compiler` 全量改名（compiler+toolchain）
- [ ] 阶段 4: env 收拢（删 Z42C_DIR、Z42_TOOLCHAIN 语义收进 SDK 根）（toolchain）
- [ ] 阶段 5: 文档

## 阶段 1: z42c build(exe) 自包含（compiler）✅
- [x] 1.1 定位落点：`z42c.driver/src/Main.z42` `_build` 写出 exe zpkg 后（非 z42c.project）；依赖分类 = 名不以 `z42.` 开头 = 非 stdlib
- [x] 1.2 `_bundleExeDeps`（镜像 z42b `_pubBundleProjectDeps`）：kind=exe 时把非 stdlib 直接依赖的 zpkg（+.zsym）从 libsDirs 复制到输出 dist；stdlib（z42.*）不拷
- [x] 1.4 验证 ✅：新 z42c build z42c.driver → dist 含 6 个 z42c.*（不含 z42.io）；`Z42_LIBS=stdlib` 跑 bundle 后 driver 从自身目录解析兄弟包成功（未 bundle 对照 undefined）；gen2/gen3 字节不动点 7/7 identical
- [ ] 1.3 自动化 e2e 断言 —— 折入阶段 2（xtask 全线用新 z42c 后，`_testCompilerE2e` 断言 z42c.driver/dist 含兄弟包 + 自包含跑通）

## 阶段 2: xtask 去 scratch 目录（toolchain）
- [ ] 2.1 `_buildCompilerViaZ42c` → `z42c build --workspace`（CWD=src/compiler；Z42_LIBS=运行种子 driver 的 {stdlib+种子7包}，种子来自 SDK programs/z42c）
- [ ] 2.2 删 `selfbuild-runlibs/` 相关代码
- [ ] 2.3 `_buildStdlibCore`：删 `dogfood/run-` 拼接；直接跑 `compiler/z42c.driver/dist/z42c.driver.zpkg`（阶段1 后自带兄弟包），`Z42_LIBS=stdlib dist`
- [ ] 2.4 验证：`xtask build compiler` → 无 selfbuild-runlibs；`xtask build stdlib` → 无 dogfood；`xtask test compiler` 7/7 + 不动点绿

## 阶段 3: 目录改名 z42c → compiler（compiler+toolchain）
- [ ] 3.1 `src/compiler/z42.workspace.toml` `[workspace.build] output_dir` z42c→compiler
- [ ] 3.2 全部 scripts/ 引用改名（xtask_compiler/stdlib/common/regen/bench/golden/compiler_e2e/bootstrap_check/test_lib/test_cross/test_platform/package_desktop）
- [ ] 3.3 `.github/actions/ci-bootstrap/action.yml` 种子 stage 目标路径
- [ ] 3.4 验证：`rm -rf artifacts/build && xtask build compiler`（cold 供种 + --workspace）→ 落 compiler/；`xtask test` 全绿

## 阶段 4: env 收拢（toolchain）—— 决策：折叠进 Z42_HOME（选项 b）
- [ ] 4.1 `xtask_common.z42`：删 `_seedDriverHome` 的 `Z42C_DIR` 分支；种子 z42c 从 `_seedSdkDir()/programs/z42c` 派生
- [ ] 4.2 **`Z42_TOOLCHAIN` 折进 `Z42_HOME`**：`_seedSdkDir` 用 `Z42_HOME` 作显式 SDK 根（`--toolchain <dir>` 设 `Z42_HOME`）；`Z42_PORTABLE_VM` 反推、`./.z42` 保留为回退
- [ ] 4.3 **detailed 日志打印设置源头**：`_seedSdkDir`/vm 定位选中后，`_vDetailed` 明确打印"SDK root = <path>（来源：Z42_HOME / apphost Z42_PORTABLE_VM / ./.z42）"
- [ ] 4.4 CI `ci-bootstrap`：`Z42_TOOLCHAIN=` → `Z42_HOME=`（同步 launcher 语义：Z42_HOME=SDK 根）
- [ ] 4.5 scripts/README env 段更新
- [ ] 4.6 验证：cold build compiler + CI ci-bootstrap 绿

## 阶段 5: 文档
- [ ] 5.1 `docs/design/compiler/self-hosting.md`：自建改 --workspace + 自包含 exe，删 runlibs/dogfood 描述
- [ ] 5.2 `docs/workflow/building/compiler.md` + `docs/book/src/dev/build.md` 产物路径/命令
- [ ] 5.3 `artifacts/build/` 布局说明（放 docs/workflow 或 scripts/README）
- [ ] 5.4 ACTIVE.md 释放 compiler + toolchain 锁；归档

## 备注
- 运行时零改动（依赖既有 `search_dirs = [entry dir, libs]`）。
- 「E0402 wrinkle」注释过时，阶段 2 顺带删。
- Problem 2（单 member 直接 build 缺兄弟包）Out of Scope，文档提示用 `--workspace`。
