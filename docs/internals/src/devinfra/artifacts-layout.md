# 产物目录布局（`artifacts/`）

> 对齐：2026-09-30（change `tidy-artifacts-tmp-and-clean`）｜ 代码：`scripts/common/xtask_layout.z42`（路径 SoT）、`src/libraries/z42.workspace.toml` 与 `src/compiler/z42.workspace.toml` 的 `[workspace.build]`、`.cargo/config.toml`
>
> 构建步骤本身见[构建编排](build.md)；打包见[打包引擎](packaging.md)。

`artifacts/` 是仓库里**唯一**的构建输出根，整棵目录被 `.gitignore` 忽略，删了都能重生。
**要加一个新产物、或者在找「某个东西被写到哪去了」，读这页。**

## 1. 顶层桶

| 桶 | 装什么 | 谁写 |
|---|---|---|
| `build/` | 编译产物与 per-component 输出，子目录**镜像 `src/`** | `xtask build *`、cargo |
| `packages/` | 组装好的发行包（`z42-<...>-<rid>-<profile>/` 及归档）| `xtask package *` |
| `xtask/` | xtask 自己的 zpkg / zsym / cache —— **不在 `build/` 里面** | `z42 publish scripts/xtask.z42.toml` |
| `bench/` | `e2e.json` / `ab.json` 等测量结果 | `xtask bench` |
| `profile/<name>/` | 火焰图、dhat 报告、counter 摘要、`report.md` | `xtask profile` |
| `tools/` | 构建**下载**的第三方工具（`node`、`android-sdk`、`playwright-browsers`）| `xtask deps install` 与按需自动安装 |
| `test-reports/<platform>/` | `junit.xml`（平台三段测试）| `xtask test platform *` |
| `tmp/<name>/` | 构建与测试各命令的**工作目录**，可重生、不进任何包，整桶可删（见 §3）| 各 stage |
| `.z42` | `xtask build sdk` 默认组装出的 SDK 布局（`programs/` + `libs/` + `bin/`）| `xtask build sdk` |

这个划分是常规的**中间态 / 输出 / vendored** 三分：`build/` = 「我们编出来的」，
`packages/` = 「我们要发的」，`tools/` = 「别人给我们的」。

> **`xtask/` 为什么是 `build/` 的兄弟而不是它的子目录**：xtask **先于**并且**驱动**所有构建。
> 它的产物路径由 `scripts/xtask.z42.toml` 的 `[build]` 段决定
> （`output_dir = "../artifacts/xtask"`、`dist_dir = "${output_dir}"`，所以是扁平的
> `artifacts/xtask/xtask.zpkg` 而不是 `.../dist/...`；`publish_dir = ".."` 把 apphost 送到仓库根 `./xtask`）。

> **没有 `deps/` 这个桶。** 早期布局文档写过一个「留给第三方二进制」的 `artifacts/deps/`，
> 代码里从未出现过；真正的落点是 `artifacts/tools/`。

## 2. `build/` 镜像 `src/`

| `src/` | `artifacts/build/` | 内容 |
|---|---|---|
| `src/runtime/` | `build/runtime/<profile>/` | cargo target-dir：`z42vm`、`libz42.*`、`z42` trampoline |
| `src/libraries/<lib>/` | `build/libraries/<lib>/<profile>/{dist,cache}/` | **per-lib** 编译，构建私有 |
| （聚合拷出）| `build/libraries/dist/<profile>/` | 全部 stdlib `.zpkg` 的**扁平单目录视图** = `Z42_LIBS` 查找点 |
| `src/compiler/<member>/` | `build/compiler/<member>/<profile>/{dist,cache}/` | 编译器后端各成员 |
| （边界检查）| `build/compiler/bootstrap-check/` | `xtask test bootstrap` 的双轨隔离工作目录 |
| `src/toolchain/<comp>/` | `build/toolchain/<comp>/{dist,.cache,publish}/` | launcher / builder / devtools / interactive 等：清单只配 `output_dir`，三个子目录走级联默认 |
| `src/tests/<rel>` | `build/tests/<rel>` | golden 编译出的 `.zbc` 镜像 |
| （wasm 测试）| `build/wasm-test/` | wasm deployable（agent + bundle + libs）|

**per-member 的产物路径不是硬编码的**：`scripts/common/xtask_layout.z42` 读各 workspace toml 的
`[workspace.build].output_dir` / `cache_dir` 模板（正是 z42c 的 `WorkspaceBuild.PlanLayout` 消费的
同一份）再展开。改 toml 模板，xtask 自动跟上。cargo 侧同理由 `.cargo/config.toml` 的
`target-dir = "artifacts/build/runtime"` 决定——**注意它不带 `<cargo-target>` 这一层**，profile
直接挂在 `runtime/` 下。

xtask 自己发明、没有 toml 归属的路径，**全部在 `xtask_layout.z42` 里各有一个单一定义**：
扁平 stdlib dist（`_libsFlatDist`）、cargo target 目录（`_cargoTargetDir` / `_runtimeOut`），以及 §1 的
每个顶层桶（`_scratchDir(root, name)` / `_tmpDir` / `_toolsDir` / `_devSdkDir` /
`_packagesDir` / `_releaseDir` / `_testReportsDir` / `_benchDir` / `_profileDir`）。使用点只写
「桶 + 自己的子目录名」，不再写 `"artifacts/…"` 字面量——挪一个桶只改一处。

> 2026-09-30 前，一次性目录（`.scratch/*`（现 `tmp/*`）、`tools/*`）是**故意不集中**的，理由是「没有
> 布局意义」。整理布局时这条理由不成立：要挪的恰恰是它们，散在 ~35 个使用点就得逐个去找。

### 查询：`xtask layout`

```bash
xtask layout            # 列出全部 key  path
xtask layout libs       # 只打印一条，给脚本用：libs=$(xtask layout libs)
```

**key 是对外契约，路径不是**：CI / 脚本按 key 取路径，布局整理时 key 不变、值跟着变。
仍然写死在 xtask 之外的有：`.github/ci/xtask`（CI 垫片，跑在 `.z42` SDK 上、要先能启动 xtask，鸡生蛋）、
`scripts/hooks/hooks.z42`（z42b publish 时单独编译的 hooks 工程，调不到 xtask 的函数）、
Rust 测试里的若干 cwd 相对路径。

### `build/libraries/dist/<profile>` 为什么必须存在

每个 stdlib 库私有地编进 `build/libraries/<lib>/<profile>/`，但 **z42vm 与打包不能依赖那些 per-lib
子目录**——它们需要一个扁平单目录。所以编完之后把每个库的 `.zpkg` / `.zsym` **hard-link**
（零拷贝）汇聚到聚合目录。它是：

- z42vm 的 dev-mode `Z42_LIBS` 回落点；
- `xtask package` 整体拷进包内 `libs/` 的来源；
- z42c 编译 / 运行时解析兄弟包与 stdlib 的单一查找点。

**没有 namespace index**：VM 扫目录、读每个 zpkg 的 `NSPC` section 认领 namespace，嵌入解析器同理。
索引会是一份需要同步的冗余状态。

🔴 **hard-link 的代价：聚合目录里的每个文件与它的 per-lib 源是同一个 inode**（实测
`links=2`）。而 `File.Copy` / `File.WriteAllBytes` 都是 **truncate 语义、保留 inode** ⇒
往聚合目录里写一个同名文件，会把 per-lib 源**一起改掉**（实测：拷一份种子 zpkg 进聚合目录，
per-lib 那份的内容当场变成种子字节）。可达路径就在供种里：`_ensureSeed` 把种子 stdlib
拷进聚合目录，而且**只拷 `.zpkg` 不拷 `.zsym`** ⇒ 留下撕开的配对，症状是一条
`build_id mismatch` WARN 污染全部 golden（relocate-compiler-domain-libs 第 6 轮吃过）。

⇒ **不变式：往可能有别名的目录写之前先 `File.Delete(dst)` 断链**（unlink 只断这一条链）。
`scripts/common/xtask_fs.z42` 的 `_copyAll` / `_linkAll` 都这么做，行为门在
`xtask test e2e --dir cross-zpkg` 每轮开跑前真跑一遍。保留 hard-link 本身——它省的是
cross-zpkg 一轮 ≈198MB 的纯拷贝；有害的是「写到别名上」，不是「有别名」。

这样 `build/` 仍完整镜像 `src/`（每条路径都能映回一个 `src/` 位置），同时给 VM 与打包一个稳定的聚合点。

## 3. `tmp/`：各命令的工作目录

判据：**它会不会被别的步骤当作「产物」消费？** 会 → `build/`；只是某个命令自己用 → `tmp/<name>/`。

| 目录 | 谁用 | 是什么 |
|---|---|---|
| `tmp/stdlib-run/<profile>` | `build stdlib` 阶段二 | stdlib 的稳定快照，供正在重编 stdlib 的 driver 当 `Z42_LIBS` |
| `tmp/seed-run-libs/<profile>` | 种子 driver 调用 | 与种子 driver 同代的运行期 libs 快照 |
| `tmp/selfhost-gen1` | `test compiler` | 不动点验证的 gen1 快照 |
| `tmp/e2e` / `tmp/xpkg-driver` / `tmp/fs-writethrough` | e2e / cross-zpkg | 用例工作区 |
| `tmp/xpkg-fixtures` / `tmp/multi-exe-fixtures` | `test e2e` 的 cross-zpkg / multi-exe | `src/tests/{cross-zpkg,multi-exe}` 的**拷贝**，每轮重建；夹具在这里编 / 跑（`_stageFixtureTree`），源码树零写入 |
| `tmp/targets/<proj>` | `test targets` | manifest target fixture 输出 |
| `tmp/incr-reconcile` | `test incremental` | 增量 vs 全量对账的两份产物 |
| `tmp/fingerprint` | `test fingerprint` | base 与本树编译器各编一份 stdlib 的对比场地 |
| `tmp/exec-profile` | bench / profile | 执行画像探测 |
| `tmp/install-test-*` | `test packages` | 打包自检的一次性目录 |

> 2026-09-30 前这里是 `.scratch/`（跨步骤复用）与 `tmp/`（自检一次性）两个桶，生命周期没有实质差别
> （都可重生、都不进包、都没被 `clean` 覆盖），合成一个。

### 开发树里编译器包从哪来：没有 alllibs

开发树的 `Z42_LIBS` 只是 **stdlib flat**（`build/libraries/dist/release`）。编译器域的包（`z42c.*` / `z42.project` /
`z42.build` / `z42.package` / `z42.scripting`）分两种场合：

| 场合 | 从哪解析 |
|---|---|
| **编译期**（driver 编工具链程序 / 编译器单元测试 / 带 SDK 库的工程）| 按名声明的 SDK 库，driver 从编译器目录解析（`CompilerDomain` 开发树档）|
| **运行期**（z42b 自身依赖 `z42.build` / `z42.project` + 注入 `z42c.pipeline`；它为编译器单元测试 fork 的子 VM）| xtask 给 z42b 进程挂 `Z42_PROBING_PATHS` = 各编译器成员的 dist（`_withCompilerProbing`）|

两条不变式，都由代码守着而不是靠约定：

- **probing 里不能有 stdlib**。probing 排在 `Z42_LIBS` **之前**，有 stdlib 就会把 flat 里的新版本遮蔽成旧的。
  所以不含 `z42c.driver` 的 dist（它是自包含闭包，带整套 stdlib 副本），其余成员 dist 逐目录断言只有它自己
  （`_compilerProbingPaths`，违反即抛）。
- **只给 z42b 进程挂，不放进 xtask 自身环境**。`Z42_PROBING_PATHS` 会**覆盖**程序侧车的 `probing-paths`，全局一设，
  测 probing / `deploy = "sdk"` 的那些用例就被干扰了。

工具链程序（launcher / z42b / z42d / z42i）用到的编译器包在各自清单里写 `deploy = "shared"`：不复制进发布包，运行期经
`probing-paths = "../z42c"`。

> **历史：alllibs（`build/views/<profile>/all`，2026-10-01 删除）**。此前因为 VM 的 `Z42_LIBS` 只能是一个目录，xtask
> 把 stdlib flat 与全部编译器成员 dist **拷**进一个目录当唯一的 `Z42_LIBS`。代价有两个：
> ① 编译器 dist 里的 stdlib 副本会把新 stdlib 遮蔽成旧的。PR #955 实测：命令显式喂了 flat，跑的却是旧 stdlib；
> 只能靠「先 flat 后成员、不覆盖」的拷贝顺序加事后逐字节对账兜住。② 工具链程序在它下面编译时，编译器包被当成
> 「框架」不复制——发布包里有没有它们取决于构建环境，而不是清单。只编译、且被编的东西只用 stdlib 的地方
> （profile / bench / GC 压力 / embedded golden）其实一直不需要它。

## 4. 清理

| 命令 | 删什么 |
|---|---|
| `xtask clean` | 生产 cache/dist：各 stdlib 成员的 `<lib>/<profile>/{cache,dist}` + 扁平 `libraries/dist/` |
| `xtask clean tests` | golden `.zbc` 镜像（`build/tests`、`build/{libraries,compiler}/<m>/tests`）+ z42b 的 test 目标输出（`<工程目录>/artifacts/test-targets`）|
| `xtask clean bench` | z42b 的 bench 目标输出（`<工程目录>/artifacts/bench-targets`）|
| `xtask clean tmp` | `tmp/`（+ 旧名 `.scratch/` 与已删的打包暂存 `publish/`）|
| `xtask clean all` | `build/` + `tmp/` + 旧名 `.scratch/` / `publish/` + **源码树里**各 z42 工程旁的 `artifacts/`、`dist/`（+ cross-zpkg 用例的 `libs/`）|

`clean all` **保留** `xtask/`（驱动自身，正在运行）、`tools/`（下载的第三方工具）、
`packages/` `release/` `.z42/`（成品）与 `bench/` `profile/` `test-reports/`（报告）。

> **源码树里为什么会有产物**（实测一次完整 GREEN 后约 450 个目录）：
> - 单独编一个 workspace 成员（xtask 的 path 依赖 `z42.project` / `z42.build`、z42b dev 目标的父包）时
>   不继承 `[workspace.build].output_dir`，走单工程默认布局写进 `<工程目录>/{artifacts,dist}`；
> - `src/tests/**` 下的 manifest-targets / z42b 夹具工程按单工程默认布局原地构建（cross-zpkg / multi-exe
>   自 isolate-xtask-fixture-builds 起改在 `tmp/` 的拷贝上编，不再写源码树）。
>
> 它们都被 `.gitignore` 忽略、仓库里没有任何入库文件在其下。`clean all` 只删「带清单（`<name>.z42.toml`
> 或 `z42.toml`）的工程目录」旁边的 `artifacts/` `dist/`，`libs/` 只在 `src/tests/cross-zpkg/` 下删
> （不做全树通配，免得碰到 wasm / node 工程的同名目录）。根治是让这些构建写进 `artifacts/`，属于后续 change。

`tmp/`、`tools/` 任何时候 `rm -rf` 都安全（前者可重生，`tools/` 会被下次
`deps install` / 按需安装补回）。
`artifacts/` 整棵删掉之后是**冷启动**路径：需要网络下载 nightly 种子，见[xtask](xtask.md) §5。
