# scripts

## 职责

仓库的开发 CLI 与启动引导。绝大多数开发命令（build / test / package / setup /
bench）都已收敛到一个自举的 z42 程序 **xtask**：源码在本目录的 `src/`
（按职责分目录组织，见末尾），构建为**原生 apphost 可执行**（仓库根 `./xtask`，
内嵌 launcher + `xtask.zpkg`）：

```
z42 publish scripts/xtask.z42.toml             # 构建+部署 → 仓库根 ./xtask
xtask <command> [args]                         # 直接运行
```

> 改了 `scripts/src/**/*.z42` 后**直接重跑上面这条 publish 就够了**：publish 每次都过一遍 z42c
> 的增量编译，改了什么重编什么、没改就空转（~0.5s，一个字节都不重写），无需先 `z42 build`。

xtask 是独立的 z42 应用——它不是通用 `z42` launcher 的一部分（launcher 保持通用
运行时）。冷启动如何先产出 xtask 见下文「冷启动 bootstrap」。

`src/` 下的 `.z42` 除 `hooks.z42`（z42b build hook，`namespace Build`）外全部是 xtask 模块（含 stdlib 构建逻辑 `src/build/stdlib.z42`）。
安装脚本运行在「还没有 z42 工具链」的最前端，故保持 shell / PowerShell：

- **`install-z42.{sh,ps1}`** —— 唯一的 SDK 安装器，同一文件两套默认值，只看脚本自身位置（不看当前目录）：
  - 仓库模式（以 `<repo>/scripts/install-z42.*` 的路径运行）：版本取 `versions.toml` 的 `[toolchain.z42].launcher`，
    装进 `<repo>/.z42`，不改 PATH，不读 `Z42_HOME` / `Z42_VERSION`；固定版本缺失或读不出时报错，不降级为用户安装。
  - 用户模式（下载的副本、`curl … | sh`、`irm … | iex`）：`Z42_VERSION` 或 nightly，`Z42_HOME` 或用户目录 `.z42`，配置用户 PATH。

  显式 `--version` / `--dest`（PowerShell：`-Version` / `-Dest`）覆盖两套默认值；另有 `--no-modify-path` /
  `--archive <本地包>` / `--force` / `--dry-run`。安装按归档自带的顶层条目整项替换，安装目录里的 `runtimes/`
  （已装 workload）、`cache/`、`install.toml` 保留——所以脚本里没有 SDK 顶层清单。发布在
  `https://z42-lang.github.io/z42/install-z42.{sh,ps1}` 与每个 release 的附件，重新运行即更新。
- **`install-z42.{bat,command}`** —— Windows CMD / macOS Finder 的启动转发，只传参数与退出码，不含安装策略。

安装器的默认值 / 参数 / 转发回归是 `package sdk --verify` 的自检（`src/test/sdk/installer_selfcheck.z42`，
dry-run 跑真实脚本，不联网）；真实 SDK 的离线安装由 CI package-host 的 Installer smoke 覆盖。

> **冷启动 bootstrap（鸡生蛋的真正破解点）**：`xtask.zpkg` 依赖 stdlib 才能编译，且
> 编译它的 z42c 自身也是 z42 写的。冷树上**下载上一版已发布 nightly 的 z42c 种子**
> （`z42c.driver.zpkg` + stdlib dist），用它 `z42c build scripts/xtask.z42.toml` 产出
> `xtask.zpkg` → 再 `xtask build stdlib`（即 `src/build/stdlib.z42`，z42c 从源码
> 重编 stdlib + 自建 z42c）。z42c 只读 `.zsym`、VM 扫目录（读各 zpkg 的 NSPC section 认领
> namespace），都不需要任何 namespace 索引即可编译/运行 xtask，所以这个次序无死锁。
> **工具链全程 z42 自举**。

> **种子从哪来（`build compiler` / `build stdlib` 共用一套解析，CI = 本地）**：冷树无
> in-tree 种子时，`_ensureSeed`（`src/build/seed.z42`）按 **`Z42_HOME`
> （`--toolchain` 设它，或 launcher/install 设）→ 运行 xtask 的 apphost SDK
> （`Z42_PORTABLE_VM` 反推）→ `./.z42`** 顺序找 SDK-toolchain 布局的根
> （`programs/z42c` + `libs`），把 `programs/z42c` + `libs` 拷进 in-tree 再自建；
> warm 树（已有 in-tree 种子）直接复用——**gen2 字节不动点靠它**，故不覆盖。所以
> `install-z42.sh` 之后本地 `xtask build compiler` 开箱即用；CI 只需设
> `Z42_HOME=<下载的 SDK>`（`.github/actions/ci-bootstrap`）。
> managed 布局的 `Z42_HOME`（`runtimes/`，无 `programs/`）不符 SDK-toolchain 布局 →
> 跳过（不误当种子源）；`Z42_LIBS` 显式覆盖仅在其确实含 `z42.core.zpkg` 时生效。

> 所有版本号的唯一真相源是 `scripts/versions.toml`（读它的几乎全是 xtask，故放这里；入口 `_versionsPath`）（xtask 经 `Std.Toml` 原生解析，
> 见共享模块 `src/common/versions.z42`）。

## 全局选项

子命令前后皆可（根命令 `XtaskCommand` 的全局 option，见 `src/main.z42`）：

| 选项 | 作用 |
|------|------|
| `--toolchain <dir>` | 指定 `.z42` 布局的工具链（`programs/z42c` + `libs` + `bin`）；写入 `Z42_HOME`（单一「SDK 根」变量），被 seed / VM / libs 定位器读取 |
| `--verbosity` / `-v <level>` | 输出详略：`q[uiet]` \| `m[inimal]`（默认）\| `n[ormal]` \| `d[etailed]` \| `diag[nostic]`；写入 `Z42_VERBOSITY`，子进程继承 |

**verbosity 级别（累进，MSBuild 风格；实现见 `src/common/logging.z42`）：**

| 级别 | 打印 |
|------|------|
| `quiet` | 仅错误 |
| `minimal`（默认） | + `▶`/`✔` 每个流程的开始/结束标记（跟踪进度） |
| `normal` | + 每步最终选定的结果（如 `seed: z42c ← <path>`） |
| `detailed` | + 逐候选路径的选择过程（如 SDK 依次试 `Z42_HOME`→`Z42_PORTABLE_VM`→`./.z42`）+ 子工具逐文件输出 |
| `diagnostic` | + 每个流程的耗时（`⏱ … — N ms`） |

示例：`xtask -v detailed build compiler` 会打出冷启动供种的完整候选路径选择过程。

CI 调度使用 `ci supersede` 按资产差异取消旧 main 运行，`ci check-publication` 阻止 nightly 回退；边界与验证见 [CI 调度](../docs/internals/src/devinfra/ci.md)。

## 命令一览

| 命令 | 触发时机 | 关键依赖 | 主要产物 |
|------|---------|---------|---------|
| `setup [--os P] [--force]` | **首次 clone / 平台版本变动** | `versions.toml` | host 基础检查；`--os` 装该平台必备（rust targets + cargo-ndk / wasm-pack / NDK + 构建 SDK …） |
| `setup vscode` / `setup check [--os P]` | 装编辑器扩展 / 看本机缺什么 | `versions.toml` | 工作区 VSCode 扩展 / 在场报告（`--os` 时缺即非 0） |
| `check runtime-cargo` | 改 runtime Cargo 调用 / 构建配方后 | `cargo` | 最小 workspace 实编 alias / R0 / legacy 三类配置、压缩库部署、doctest 和 macOS 独立探针和子进程依赖加载；GREEN 的 runtime 单测阶段也会执行 |
| `check versions` | 改 `versions.toml` 后对账（也是 GREEN 的一个 stage） | `versions.toml` + 投影文件 | versions.toml ↔ Cargo.toml / build.gradle.kts / Package.swift 一致性 |
| `build stdlib` | 改了 stdlib `.z42` 源 | warm z42c 种子 | `artifacts/intermediate/libraries/flat/release/<lib>.zpkg`（扁平视图，无 namespace 索引） |
| `build compiler` | 改了 z42c 编译器源 | warm z42c 种子 | `artifacts/build/compiler/<member>/release/dist/*.zpkg`（编译器域全部成员，见 `src/compiler/z42.workspace.toml`） |
| `build workload` | 改了 `src/toolchain/workload` 源 | z42c/stdlib（缺则自建） | 4 个 workload lib → `artifacts/intermediate/libraries/flat/release/z42.workload.*.zpkg`（launcher 依赖） |
| `build toolchain [z42b]` | 改了 launcher/z42b/z42d/z42i 源 / 要可运行的 SDK | 同上 + 自动 `build workload` | z42 / z42c / z42b / z42d / z42i 五个 apphost `publish <toml>` → **各 toml 的 `[platform.desktop].publish_dir`**（路径从 toml 读，不硬编码）；`z42b` = 只按当前 z42c 编 z42b |
| `build runtime` / `build all` | 改了 Rust VM / 一次构建 runtime + compiler + stdlib | `cargo` | `z42vm` + libz42；`all` 另含 compiler、stdlib |
| `build docs [index]` | 读 / 预览 reference 的 stdlib 页、改了 `///` 或渲染器；`index` = `z42 doc` 要查 Rust 符号 | z42.doc 渲染器（xtask 自带）/ `cargo` | `docs/reference/src/stdlib/` 生成页（不入库）+ SUMMARY 区段；`index` → `artifacts/doc/rust.idx` |
| `build test` | 改了 golden 测试源 | z42c/stdlib（缺则自建） | `src/tests/**` → `.zbc` 镜像到 `artifacts/build/tests/`（golden 编译，不重建工具链） |
| `package dev-sdk [--out D]` | 组装开发态 SDK（只组装，不编译） | `build compiler` + `build stdlib` + `build runtime`（+ `build toolchain` 才有 apphost） | `.z42` 布局：`bin/{z42vm,z42c,z42b,z42d,z42i}` + `z42`(launcher 根) + `programs/*` + `libs/*`；apphost 从各 publish_dir 合并，没发布的跳过并列出 |
| `package ci <dest>` | 已建好构建树的 zpkg 快照 → CI 的 `toolchain-<os>` artifact（只组装；下游 job 原地还原） | z42c / stdlib / xtask + `build toolchain z42b` | `<dest>` 下按仓库相对位置的 zpkg / zsym |
| `package sdk [--profile] [--no-build] [--archive]` | 打 host SDK 发行包 | `cargo` + z42c | `artifacts/packages/z42-<ver>-<host>-release/{bin,libs,native}`（末尾 source-identity 门） |
| `package runtime [--rid R] [--archive]` | runtime 包（native+stdlib，平台随 rid） | `cargo` + z42c | host: `z42-runtime-<ver>-<rid>`；平台: `z42-<ver>-<rid>-release` |
| `package workload [--rid R \| test] [--archive]` | per-RID desktop workload / payload-only test workload | `cargo` | workload 包目录 |
| 上面三条带 `--archive [--label L]` | 同时出发布归档（CI 各 package job / 本地发布；只限 release profile） | 本次产出的包目录 | 包目录旁的 `artifacts/packages/z42-{sdk,runtime,workload}-<L>-….{tar.gz,zip}` |
| `package release <label> [--dir D] …` | 发布汇总：合并 desktop workload → SHA256SUMS → release-index.json → release-notes.md | 9 个 RID 的归档（默认在 `artifacts/packages/`） | 合并后的 desktop workload + `SHA256SUMS` + `release-index.json` + `release-notes.md`（release 页正文：安装命令 + 下载表，按 versions.toml 生成，不作附件） |
| `bench [--tier T] [--diff\|--ab]` | 性能基准 / 回归对比（场景在 `src/benches/`） | z42c + hyperfine | 各场景编译/执行耗时；`--diff` 比对两组结果，`--ab` 同 runner A/B 判红 |
| `profile <script> [--cpu\|--heap\|--threads\|--e2e\|--all]` | 深挖某个 `.z42` 脚本的性能 | z42c +（可选）samply/dhat/hyperfine | `artifacts/reports/profile/<name>/`：CPU 火焰图 / dhat 堆报告 / peak-RSS / counter 摘要 + `report.md` |
| `test` | **每次 commit / 合并前必跑** | 下面各 stage | 串联全部验证 stage（`test --list` 查询清单；机制见 internals/devinfra/test-gate.md；完整 runtime 测试独立运行） |
| `test runtime` | 改了 Rust VM (`src/runtime/`) | `cargo` | Rust VM 单测/集成（`cargo test --test-threads=1`；含 zbc/zpkg format 基线）。**不在 `test` gate 内**（signal 测试在受限沙箱会挂）；CI 每腿单独一步 + 按需本地跑 |
| `test e2e [--dir <cat>] [--file <p>] [--mode interp\|jit]` | 跑 `src/tests/` 端到端（golden + cross-zpkg；最常用） | `cargo build` + golden 产物 | 默认全跑；`--dir`/`--file` narrow |
| `test stdlib [lib]` | stdlib 源 / 编译器变动 | `build stdlib` + z42b（z42.builder.zpkg） | 各 stdlib lib 的 `[Test]` 通过率 |
| `test compiler [--no-build]` | z42c 编译器变动 | 自建工具链，或通过 `--no-build` 使用已构建的工具链 | 全成员自举不动点（gen1==gen2）+ [Test] units + e2e |
| `check docs` / `check release-archives` | 文档契约 / 发布归档实现或自检改动 | 种子 SDK，归档检查另需系统归档工具 | 链接、诊断码、布局 / 全 RID 归档与真实解包，不构建产品 |
| `package sdk --verify` | 改打包系统 / 发行版相关后，验证打包出的 SDK 能独立工作 | 组装出的这份 SDK（缺本机桌面 workload 就先打） | packages.toml 自检（解析 / 组件安装 / 发布归档）+ 发行包夹具 + packaged z42c/z42vm 跑 golden |
| `flow plan / run / graph / check` | 本地与 CI 按改动选择流程 | `flows.toml` + Git | 可解释计划、JSON 与 Mermaid；详见[流程规划](../docs/internals/src/devinfra/flows.md) |
| `test changed [base]` | 增量自测（按改动文件挑 stage） | 上述各命令（in-process 调度） | 仅跑受影响的 stage |
| `test docs` / `examples` / `targets` / `embedded` / `platform` / `packages` / `bootstrap` 等 | 各专项门（完整清单 `xtask test -h`） | 视 stage | 文档死链、学习手册示例、manifest target、设备嵌入测试、平台 3 段测试…… |
| `clean [tests\|bench\|all]` / `layout [entry]` / `run <zpkg\|.z42>` | 清产物 / 打印 `artifacts/` 树或某项绝对路径 / 经 launcher 直跑 | — | — |

> **构建输出约定**：
> - `artifacts/build/` **只放编译产物**（含测试的编译产物，成员内 profile 在外、`tests` 在内）；
>   其余中间物（夹具暂存拷贝、harness 工作目录、设备测试 bundle / 宿主副本、编译器自举快照、xtask 自检目录）落
>   `artifacts/intermediate/` 下 owner 的镜像（规则见 artifacts-layout.md §1、§3 与 test-layout.md §5）。
> - **toolchain 组件的输出/publish 路径一律从各 `z42.toml` 读**（`[build].dist_dir`/`output_dir`、
>   `[platform.desktop].publish_dir`，级联默认见 `docs/reference/src/toolchain/z42-toml.md`）——xtask 不硬编码，
>   改路径只动 toml。定位 helper：`src/common/components.z42` 的 `_desktopPublishDir` / `_toolchainZpkg`。
> - **workspace per-member 产物路径同样单源自 `z42.workspace.toml`**：`artifacts/build/{libraries,compiler}/<member>/<profile>/dist` 由
>   `[workspace.build].output_dir` 模板经 `src/common/layout.z42` 的 `_memberDist`
>   （`ManifestLoader.LoadWorkspace` + `PathTemplate.Expand`，与 `WorkspacePlan.PlanLayout`
>   同一份布局真相）展开——xtask 不字面拼接。flat dist / runtime out / build root 等 xtask 约定
>   （无 toml 归属）也集中到该模块单点定义。顶层桶（`tools` / `packages` / …）与两种镜像（编译产物 `_buildMirror`、中间物 `_workOut` / `_workRootOf` / `_compilerWsWork` 等）同样在该模块单点定义。

## 各命令处理流程

> 据源码逻辑绘制；函数名标在节点上，可对照源文件（z42c + z42vm 全程自举）。

### `test`（完整 GREEN gate，`src/test/gate.z42 :: _testAll`）

```
test ──► _testAll
  │  ⓪ 读取 _gateStages、校验 skip 键        未知键在构建前报错
  │  ① 构建波 (一次)                      _buildDebugVmAndCompression → _regenForTest
  │       └ cargo debug z42vm + compression cdylib → build stdlib + z42c
  │         + cargo release z42vm + golden .zbc
  └─► 依序跑各验证 stage，任一失败立即停 ──► GREEN
  # CI 只为并行用 `--skip` 把部分 stage 下放到独立 shard job（见 internals/devinfra/ci.md）。
```

> `xtask test --list` 从执行表打印完整 stage 顺序与 skip 键，不构建、不运行测试。
> 各类检查的职责见[测试门禁](../docs/internals/src/devinfra/test-gate.md)，源码入口见下方「源码结构」。

### `build stdlib`（`src/build/stdlib.z42 :: _buildStdlib`）

```
build stdlib ──► _buildStdlib
  ① 校验 warm 种子 (z42c.driver.zpkg + stdlib dist 存在)   缺 → _ensureSeed 冷启动供种 (SDK)
  ② z42c 自建编译器域全部成员    _buildCompilerViaZ42c (src/build/compiler.z42)
       └ z42c build --workspace（driver dist 自包含全部编译器成员包）
  ③ 快照 stdlib → .stdlib-run (只 stdlib；driver 运行期 Std.* 需稳定副本)   _copyAll(flatDir, .stdlib-run)
  ④ 直跑自包含 z42c.driver build --workspace --release    CWD=src/libraries, interp, Z42_LIBS=.stdlib-run
       └ per-member dist 覆盖 canonical 布局
  ⑤ verify 产物 + flat view (hard-link)    _assembleStdlibFlatView → intermediate/libraries/flat/release
```

### `build test`（`src/build/golden_assets.z42 :: _buildTest → _regenGolden`）

```
build test ──► _buildTest
  ① _ensureToolchainDeps: 缺则自建 z42c/stdlib/z42vm（build-if-missing）
  ② _regenGolden:
       枚举 golden 三种布局 (src/tests/<cat>/<name>/source.z42 · stdlib tests · flat *.z42)
       └ 排除 errors/parse (预期失败) + [Test]/[Benchmark] 目录
       并行批量 (8/批) z42vm 跑 z42c.driver --emit-zbc → .zbc → artifacts 镜像
       └ 全量时追加 z42.package/tests/fixtures/zbc-format：就地覆盖 (git diff = 格式漂移)
```

> `_regenCore`（rebuild stdlib+runtime+goldens）是 `test` gate 的 build-wave（`_regenForTest`）。
> 格式 bump 后重生 fixture：`build compiler && build stdlib && build test`。

### `package sdk` / `package runtime`（`src/package/dispatch.z42`）

```
package sdk [--profile P]            ──► host SDK 包（桌面 RID，含 host）
package runtime [--rid R] [--profile P] ──► 按 RID 分类 dispatch
  ├─ desktop  → src/package/desktop.z42
  │     sdk：z42c 种子 + z42vm + ext 动态库 + stdlib zpkg + manifest + 原生 apphost
  │     runtime：libz42（静态 + 动态）+ C-ABI headers + ext 动态库 + stdlib zpkg
  ├─ ios      → src/package/ios.z42      cargo rustc (staticlib) + SwiftPM facade
  ├─ android  → src/package/android.z42  cargo-ndk rustc (cdylib) + Gradle facade
  └─ wasm     → src/package/wasm.z42     cargo rustc (wasm) + npm facade
  ──► artifacts/packages/z42-<ver>-<rid>-<config>/ + manifest.toml + source-identity 门
```

### `setup`（`src/setup/`）

```
setup [--os P] [--force] ──► _setupInstall   纯安装（default action）
  ├─ 无 --os       当前 host 基础（跨平台 rust/node 检查，不装交叉栈）
  ├─ --os android  rust targets + cargo-ndk + JDK + 构建 SDK   src/setup/android.z42
  ├─ --os ios      rust targets + Xcode
  └─ --os wasm     rust targets + wasm-pack + hermetic node
setup vscode       ──► _setupVscode    编辑器资产 → <repo>/.vscode/extensions（工作区本地扩展）src/setup/vscode.z42
setup check [--os P] ──► _setupCheck   只读在场校验（presence 仅显式 --os 时计入退出码）src/setup/check.z42
```

版本漂移（versions.toml ↔ 投影文件）与机器无关，不归 setup：`check versions`（`src/test/check/versions.z42`）。
xtask 自己解析 NDK / SDK 位置，没有「导出环境变量」的命令。

用到才装（零命令面）：android emulator tier（~4GB）由 `test app android run`
自动装；node 兜底由 wasm 测试自动装。

## 典型流程

**首次 clone / 新 dev 环境**：
```bash
xtask setup check                   # 看本机缺什么
xtask setup --os android            # 需要打 android 时再装该平台必备
xtask setup check --os android      # 严格校验该平台依赖已就位（缺失即非 0）
```

**commit 前 / 合并前（必跑，workflow 阶段 5 全绿入口）**：
```bash
xtask test               # 串联全部验证 stage（组成见 internals/devinfra/test-gate.md；runtime 独立，见 test runtime）
```

> 不要单独只跑其中一个 stage 就当作通过 —— 例如 cross-zpkg subclass catch
> 这类 bug 只有 `test stdlib` 能发现。

**日常开发循环（最高频）**：
```bash
xtask build test              # 改了编译器后重生 .zbc 基线
xtask test e2e            # 跑 VM 端到端（interp + jit）
xtask test changed       # 或只测受改动影响的 stage
```

**改了 stdlib `.z42` 源**：
```bash
xtask build stdlib       # 重编 stdlib zpkg（扁平视图，无 namespace 索引）
xtask test e2e
```

**完整发行验证**：
```bash
xtask package sdk --verify    # 打 host-RID 发行包，再自检 + 端到端验证（发行包夹具 + packaged z42c/z42vm 跑 golden）
```

## 源码结构（按职责分目录）

> namespace 扁平（`Z42Xtask`），跨文件按裸名调用，目录纯为组织；`xtask.z42.toml` 的
> `[sources].include = ["src/**/*.z42"]` 递归收录 `src/` 下全部模块（`hooks.z42` 由 `[plugins]` 分走）。

```
scripts/
├── flows.toml             目录归属、影响图、执行前提与 CI 选择的单一配置
├── xtask.z42.toml          工程清单（源集 src/**/*.z42；output → artifacts/xtask/）
├── versions.toml           版本与平台承诺、workload / RID 登记（数据，见头注）
├── packages.toml           发行包内容清单（数据，见头注）
├── install-z42.{sh,ps1,bat,command}   SDK 安装器（见上文）
└── src/
    ├── main.z42            **根命令**：Main → XtaskCommand（全局选项 + 挂各命令族）+ `run` 直通；各族的 `Command` 类住在各自目录的 `command.z42`
    ├── hooks.z42           z42b build hook（`namespace Build`，非 Z42Xtask；`[plugins]` 的单文件条目，自动排除出 [sources]）：
    │                       publish 前现场 cargo 编 apphost stub，免装 desktop workload
    ├── clean.z42           `clean` 命令（production / tests / bench / intermediate / all，见 artifacts-layout.md §4）
    ├── common/             共享基建（非某个命令专属；不反调命令实现）
    │   ├── git.z42               flow / 本地选测共用的 Git 读取、改动文件与错误传播
    │   ├── repository.z42        仓库根（git）、源码里的常量、zpkg 文件头
    │   ├── toolchain.z42         用哪套工具链：运行中的 z42vm、`--toolchain` 选择器、Rust runtime 的 cargo 调用
    │   ├── logging.z42           verbosity 与流程的开始 / 结束 / 耗时标记
    │   ├── text.z42              通用的字符串 / 集合小工具
    │   ├── projects.z42          共享目录 / workspace 成员定位，复用项目模型并拒绝歧义 / 包名冲突
    │   ├── layout.z42            构建树布局路径（per-member 读 workspace.toml 单源 + flat/runtime/build-root 约定 + 已建产物路径）+ `layout` 命令
    │   ├── versions.z42          versions.toml 读取器 + RID / cargo target 映射 + 发布归档命名 + 版本号比较
    │   ├── packages_config.z42   packages.toml 读取器（package/component 注册表）
    │   ├── components.z42        workspace 成员、工具链组件与 TOML 产物/发布路径
    │   ├── process.z42           子进程：z42 进程工厂（stdlib、编译器 probing 与种子隔离）+ 通用执行 / 输出（_exec / _cmdOut / 时间戳 / sha256）
    │   ├── publish.z42           共用的 z42c build / z42b publish 调用（含编译模式与成员产物校验）
    │   ├── golden.z42            golden 枚举 / 测试单元识别 / 入口推导 / --dir、--file 选择（多 test stage + build test 复用）
    │   ├── toolset.z42           外部工具（cargo/gh/tar/... 与 Android NDK）的存在性与定位
    │   ├── fs.z42                文件系统原语（_resetDir/_copyAll/_linkAll/_copyFile/_copyIfExists/_cleanGlob/_makeExe；写入前先断链）
    │   └── harness/              test 与 bench 共用的执行机制（只被 test/、bench/ 调用，不进发布判定）
    │       ├── fixtures.z42          声明式夹具（`expect.toml`）的通用引擎：z42c / z42b 命令行夹具与发行包夹具共用
    │       ├── tooling.z42           共享工具路径与 z42b unit 进程工厂
    │       ├── fixture_tools.z42     开发树夹具共用的 z42c / z42b / VM 进程工厂
    │       ├── fixture_build.z42     多包 / 多入口夹具的构建、隔离 libs、产物定位与汇总
    │       ├── manifest_targets.z42  manifest 目标读取/校验、test/bench 编排与聚合（stdlib / targets / example 三个 flow 共用）
    │       │                         + micro bench 结果聚合 `MicroBenchAgg`（`bench stdlib --json`）
    │       ├── package_units.z42     unit 级机制（发现 / 批量编译运行 / 合成 manifest）
    │       ├── unit_projects.z42     workspace 真实工程集合、声明包名选择与按 kind 分片
    │       └── execution_profile.z42 执行剖面词汇（tier × aot_pkgs × VM caps）
    ├── build/              编译各组件、生成文档产物
    │   ├── command.z42           `build` 族的命令类
    │   ├── stdlib.z42            build stdlib（z42c build --workspace + 扁平视图）
    │   ├── compiler.z42          build compiler（workspace 自建与依赖准备）
    │   ├── seed.z42              自举种子：冷树从 SDK 供种（_ensureSeed）+ 种子 driver 的运行期 libs（代际锚、搬迁、补齐、回落）
    │   ├── compiler_prebuild.z42 自举输入的 path 依赖预建（不继承源 workspace 的隐式 SDK 成员可见性）
    │   ├── runtime.z42           build runtime（cargo z42vm）
    │   ├── toolchain.z42         build workload / build toolchain [z42b]（apphost publish，路径从各 toml 读）
    │   ├── golden_assets.z42     **`build test` 的实现**（golden .zbc 编译；_buildTest / _regenGolden / _regenCore）
    │   └── docs/
    │       ├── generate.z42      `build docs`：reference 的 stdlib 生成页 + SUMMARY 区段
    │       └── index.z42         `build docs index`：Rust 符号索引（`z42 doc` 查询用）
    ├── test/               `test` 命令族与各类验证，按被测对象分子目录（见 test-layout.md §1）
    │   ├── command.z42           `test` 族的命令类 + LibUnitsCommand（test stdlib / bench stdlib 共用）
    │   ├── tooling.z42           stdlib / 目标 / example 共用的工具链准备与显式工具链覆盖
    │   ├── gate.z42              GREEN / e2e 编排 + stage 单源登记 / 只读列表 / 计时
    │   ├── gate_selfcheck.z42    stage 编排回归（skip / 必跑 / 首败 / 退出码），随 builder 夹具跑
    │   ├── changed.z42           `test changed` / `test --changed`：直接消费 flow 计划的命令与 stage
    │   ├── changed_selfcheck.z42 真实 Git 的工作区归属、重命名、SDK 职责与读取失败回归，由必跑 flows stage 执行
    │   ├── catalog.z42           `test list`：只读语料目录（pretty / json）
    │   ├── runtime/
    │   │   └── runner.z42        Rust runtime 完整 / 窄版测试的共用执行器与 GC 模式回归
    │   ├── e2e/                  `test e2e`
    │   │   ├── goldens.z42             golden 跑分（+ --dir/--file 子选择）
    │   │   ├── cross_zpkg.z42          cross-zpkg 多包
    │   │   ├── multi_exe.z42           multi-exe（一工程 → N 个 exe zpkg）
    │   │   └── fixture_runs_selfcheck.z42  真实进程退出码与主产物选择回归，随 multi-exe 跑
    │   ├── stdlib/               `test stdlib` / `bench stdlib`
    │   │   ├── runner.z42              stdlib / 编译器域 [Test]/[Benchmark] 命令与编排；共用 runner `_testPkgsCore`
    │   │   ├── shards_selfcheck.z42    分片参数、冲突选择与空分片零构建 / 空 benchmark 报告回归，随 stage 自检跑
    │   │   └── lib_discovery_selfcheck.z42  workspace members/exclude、声明包名、非默认成员与 test / bench 分片回归
    │   ├── compiler/             `test compiler`（含 bootstrap / fingerprint / incremental 子命令）
    │   │   ├── runner.z42              编排与独立测试工程（统一清单定位、工程布局和主产物选择）
    │   │   ├── e2e.z42                 z42c 命令行与构建夹具套件（用例在 `src/compiler/z42c.driver/tests/fixtures/cli/`）
    │   │   ├── selfhost.z42            自举不动点重建与包比较（包头字段、段集合 / 数据、BLID 排除、目录范围校验）
    │   │   ├── selfhost_selfcheck.z42  不动点比较的 BLID 排除、包头 / 业务段差异与损坏目录回归，随 compiler 测试跑
    │   │   ├── projects_selfcheck.z42  真实编译器测试工程的标准清单、自定义 dist 与旧包干扰回归
    │   │   ├── bootstrap.z42           上一版 nightly z42c 能否编当前源（分阶段纪律边界检查）
    │   │   ├── fingerprint{,_entries,_selfcheck}.z42  编译器语义指纹检查（真实 Lexer 解析条目）及其回归
    │   │   └── incremental{,_workspace}.z42  增量编译暴力对账（逐文件 touch：增量产物 == 全量，逐字节；`_workspace` 为 stdlib 整体读回）
    │   ├── toolchain/            `test toolchain`
    │   │   ├── units.z42               src/toolchain/<comp>/ 下各工程的 [Test] unit
    │   │   ├── targets.z42             `test toolchain builder` / `bench targets` 入口（_testTargetsCore）+ fixture 工程与 tooling 准备
    │   │   ├── builder_fixtures.z42    builder 命令行与 manifest 目标声明式夹具套件；损坏状态与插件顺序检查留在 targets
    │   │   ├── targets_selfcheck.z42   夹具清单定位、真实 z42b 发现链路的候选名去重与缺失选择回归
    │   │   ├── projects_selfcheck.z42  工具链工程发现、声明包名与标准 / 命名清单的真实 runner 回归
    │   │   ├── example.z42             工程清单 `[[example]]` 发现与测试编排
    │   │   └── example_build.z42       example 依赖合并、合成清单与编译 / 执行
    │   ├── app/                  `test app`：设备 / 平台测试
    │   │   ├── app.z42                 一条命令跑完设备测试（stage / build / assets / bundle / run）
    │   │   ├── platform.z42            平台 3 段测试的 dispatcher + 共享 phase②
    │   │   ├── {desktop,wasm,ios,android}.z42  各平台 backend
    │   │   ├── device_host.z42         宿主工程暂存（package-wasm 也用）
    │   │   ├── embedded.z42            嵌入式 test-host 调度（desktop/wasm/ios/android）
    │   │   ├── embedded_corpus.z42     语料枚举 SoT（`test app desktop` 与 `test list` 共用）
    │   │   ├── embedded_golden.z42     golden / 单元语料的 zbc/zpkg 发射
    │   │   └── embedded_selfcheck.z42  bundle 回归（RID 类别采样 / 分片全覆盖 / 编译失败 / 旧 manifest），随 builder 夹具跑
    │   ├── docs/                 `test docs`
    │   │   ├── links.z42               相对链接死链 + 文档分层 / README 关联文档检查
    │   │   ├── examples.z42            `test docs examples`：学习手册配套示例门
    │   │   ├── examples_{book,run,transcript}.z42  书↔examples 引用校验、`*.console` 会话重放与改写
    │   │   ├── examples_selfcheck.z42  examples stage 跳过与显式 SDK 消费回归，随 stage 自检跑
    │   │   └── learn-prose-diag-allow.txt  学习手册正文里可不配演示的诊断码（examples_book 读）
    │   ├── check/                `check <name>`：静态检查与引用报告（也是 GREEN 的 stage）
    │   │   ├── command.z42             检查名称、说明与处理函数的统一登记和 CLI
    │   │   ├── {diagcodes,layout,ci_shell,proc_env,versions}.z42  诊断码 / 测试布局 / CI shell / 子进程环境 / 版本一致性
    │   │   ├── doc_lints.z42           stdlib 文档注释 W08xx 检查
    │   │   ├── doc_refs.z42            符号引用与旧行号的只读报告
    │   │   └── checks_selfcheck.z42    诊断码检查与 ci-shell 扫描器回归，随 builder 夹具跑
    │   └── sdk/                  桌面 SDK 验证：`package sdk --verify` 的执行器与自检（README 有功能索引）
    │       ├── verify.z42              入口 + 发行包夹具套件（用例在 `src/toolchain/launcher/tests/fixtures/package/`）+ golden 腿
    │       ├── goldens.z42             发行包 golden 编译 / 分模式运行，输出与退出码共同判定
    │       ├── goldens_selfcheck.z42   真实退出 0 / 7、输出不符和编译失败回归
    │       ├── package_selfcheck.z42   packages.toml 自检的聚合（`--verify` 的第一步）
    │       ├── {packages_config,install,release,payload}_selfcheck.z42  清单解析 / 组件安装 / 发布归档 / 发布闭包各自的自检层
    │       └── installer_selfcheck.z42 install-z42.{sh,ps1,bat,command} 的默认值 / 参数 / 转发 / 离线安装回归
    ├── package/            package 各 RID 类别 + 开发态组装 + 发行档组装
    │   ├── command.z42           `package` 族的命令类
    │   ├── dispatch.z42          sdk / runtime / workload 入口、cargo 助手、native/stdlib 拷贝与 source-identity 门
    │   ├── {desktop,ios,ios_dynamic,android,wasm}.z42  各 RID 类别的管道
    │   ├── dev.z42               `package dev-sdk` / `package ci`（只组装已编好的产物）
    │   ├── install.z42           组件安装：按 packages.toml include + kind 直接装进包目录（无暂存）
    │   ├── layout.z42            包结构门（`_pkgLayoutCheck`）
    │   ├── components.z42        桌面 runtime 组件的构建与安装（SDK 与 runtime 包共用）
    │   ├── component_{platform,static,link}.z42  按目标的组件图 / 静态归档组装 / 同构建共享库闭包的重定位与签名
    │   ├── component_verify{,_c,_ios,_wasm}.z42  组件校验：Rust 宿主 / 纯 C 宿主 / iOS 导出 / WASM 重链接（`--verify` 与 bundle 校验）
    │   ├── fixtures/             组件校验用的 WASM 宿主夹具
    │   ├── test_workload.z42     **`package workload test`**：打 payload-only 的 test workload（`test` 是 workload 名，不是「测试」）
    │   └── release.z42           `--archive`（包目录 → 归档）/ `package release`（desktop 合并 → SHA256SUMS → index → release-notes）
    ├── flow/               配置、workspace 成员归属（owners.z42）、所属包与分片交集（unit_projects.z42）、影响图与执行图、计划校验与导出
    ├── ci/                 GitHub 控制策略、main 累计规划（main_scope.z42）、宿主验证（host.z42）、离线回归与自举命令探针
    ├── setup/              `setup`：各平台依赖 / 编辑器资产
    │   ├── command.z42           `setup` 族的命令类（不带子命令 = 安装）
    │   ├── install.z42           `setup [--os]`：按 versions.toml 装开发依赖
    │   ├── android.z42           Android SDK·NDK·模拟器
    │   ├── check.z42             `setup check`：只读在场校验
    │   ├── ci_installers_selfcheck.z42 + testdata/  CI 工具安装器的离线回归与命令探针
    │   └── vscode.z42            生成 z42.tmLanguage.json + **`check vscode-syntax` 门**
    └── bench/              性能测量：bench / profile
        ├── command.z42           `bench` 族的命令类（不带子命令 = e2e；stdlib / vm / targets）
        ├── bench.z42             bench 基准 / --diff / --micro-diff / --ab 同 runner 回归门禁（判定数学 `_abVerdict` 可 `--ab-selftest`）
        ├── gc_pause.z42          GC 停顿指标解析（bench 场景头 `// gc-pause: report`）
        ├── micro.z42 / micro_selfcheck.z42  脚本 micro A/B 整轮捕获、可疑复测与失败传播（本地 / CI 共用）
        ├── vm.z42 / vm_selfcheck.z42  Rust VM Criterion runner 与真实 Cargo 夹具
        ├── criterion.z42         Criterion 置信区间报告
        ├── verdict_selfcheck.z42  `bench check`：入库判定夹具与退出码
        └── profile.z42           profile 单脚本性能（cpu/heap/threads/e2e；samply/dhat/hyperfine + counter JSON）
```

每个命令的详细 Usage 见 `xtask -h`（每层子命令 `-h` 自动生成）与各源文件顶部注释。

### 目录分层规则

`src/common/` 提供路径、进程、文件系统与共享索引操作，不反调命令实现；其中 `common/harness/` 是 test 与
bench 共用的执行机制，只被这两族调用。命令入口负责解析参数，具体处理函数按职责放在 `build/`、`test/`、
`package/`、`setup/`、`bench/`、`flow/`、`ci/`。`test/` 负责组合构建、执行与静态检查（静态检查在 `test/check/`，对外仍是
`xtask check <name>`；VSCode 检查复用 `setup/` 的生成逻辑）。`test/` 下按被测对象分子目录：产品用例放回所属组件，
xtask 的计划 / 调度回归作为 `*_selfcheck.z42` 与被测模块放在一起、接入对应自检。新增用例的位置、执行器和确认步骤见
[测试用例组织规范](../docs/internals/src/devinfra/test-layout.md#1-用例放哪)。

被两个以上族用到的东西，就该上移到 `src/common/`——最底层的文件系统原语在
`src/common/fs.z42`，路径布局在 `src/common/layout.z42`，外部工具探测在 `src/common/toolset.z42`。

> ⚠️ **编译器不会拦你**：namespace 扁平（`Z42Xtask`）+ `include = ["src/**/*.z42"]`，
> 任何文件都能裸名调任何文件的函数，反向依赖不会报错、只会让人读代码时在目录间打转。
> 这条规则只能靠 review 守。

### 文件命名规则

目录说明职责，文件名不再带 `xtask_` 与族前缀。新增文件时按下表取名：

| 文件承担的角色 | 命名 | 例 |
|---|---|---|
| 某命令族的 `Command` 类 | `<族目录>/command.z42` | `src/build/command.z42`、`src/test/check/command.z42` |
| 命令或其一段的实现 | 所在目录 + 描述职责的名字 | `src/build/stdlib.z42`、`src/test/e2e/goldens.z42`、`src/package/wasm.z42` |
| 被多个命令共用的执行机制（不对应任何命令） | `src/common/harness/<名>.z42` | `src/common/harness/manifest_targets.z42` |
| 某模块的 throw-on-mismatch **自检层** | 与被测模块同目录的 `<模块>_selfcheck.z42` | `src/test/sdk/packages_config_selfcheck.z42`、`src/test/gate_selfcheck.z42` |

> `src/package/test_workload.z42` 打的是名为 `test` 的 workload，不是测试代码。

## 迭代注意点（自举边界与验证）

> 按改动类型的完整验证速查（改了编译器/stdlib/VM/xtask 各跑什么）：
> [`docs/internals/src/devinfra/testing.md`](../docs/internals/src/devinfra/testing.md)。

改本目录（乃至全仓）代码时，最容易踩的是**自举边界**：

- **语法/格式越界**：当前源码**不得使用比上一版已发布 nightly z42c 更新的语法或 zbc·zpkg 格式**
  ——新语法必须"support 先行、晚一个 nightly 再 use"，否则跨版本自举断链。改动 z42c
  （lexer/parser/codegen/格式 writer）或引入新语法后**必跑**边界检查：

  ```bash
  xtask test compiler bootstrap      # 下载 nightly z42c 编当前源，验证无越界
  ```

- **删种子/兜底路径**：必须与"为所有 cold-start 入口供种"作为**同一原子变更**；本地只能验
  warm 路径，cold 路径的全绿以 CI 为准。完整纪律见 [编译器自举](../docs/internals/src/compiler/self-hosting.md)。

- **改 xtask 源码后**：先重建再验证——`z42c build scripts/xtask.z42.toml --release` 产新
  `xtask.zpkg`，再跑 GREEN gate。


**commit 前验证**（GREEN 标准）：

```bash
xtask test                # 完整 gate（stage 组成见 internals/devinfra/test-gate.md）
```

iteration 期可用 `test changed`（按改动挑 stage）或单跑某 stage（`test e2e --dir/--file` /
`test stdlib <lib>` / `--no-build`）缩窄加速，但 **commit 前必须完整 gate**。改打包系统时
另跑 `package sdk --verify`（packages.toml 自检 + 打包出的 SDK 端到端）；改增量编译
（IncrementalBuild / CacheStore / ZbcReader / IncrementalDriver）时另跑
`test compiler incremental`（暴力对账器：语料逐文件 touch，断言增量产物 == 全量产物逐字节 + 计时）。

## 关联文档

本 README 是**基础层**（干什么 / 怎么用 / 怎么开发）；设计思路与实现机制（深入层）见 book 工具链部分：

- [xtask：自举 dev CLI](../docs/internals/src/devinfra/xtask.md) —— 自举链路、CLI 分发架构、`--toolchain` 机制
- [按改动规划流程](../docs/internals/src/devinfra/flows.md) —— 本地与 CI 共用的配置、影响图、执行前提与计划校验
- [构建编排](../docs/internals/src/devinfra/build.md) —— 编译器 workspace 自建拓扑、stdlib 三阶段、不动点验证、golden 重生
- [开发基础设施概览](../docs/internals/src/devinfra/README.md) —— xtask 三条产线全景
- 操作手册（构建/测试的完整流程）：[`docs/internals/src/devinfra/`](../docs/internals/src/devinfra)

## 按改动选择 CI 资源

`flows.toml` 是目录归属、资源前提与正确性 / 性能消费者的配置入口。`check xtask` 用种子 SDK 验证规划、执行、清单和工作流契约；`check bench-vm` 用小型 Rust 夹具验证 VM benchmark runner。`check docs` 聚合链接、诊断码与布局；`check release-archives` 独立验证发布归档，不需要产品构建。控制层改动不因同属 xtask 而触发所有产品测试。
`test stdlib --plan` 与 `bench stdlib --plan` 保留计划所属包及分片；`bench vm --base <树>` 直接对两棵 Rust 树做 Criterion A/B。完整配置说明见[流程规划](../docs/internals/src/devinfra/flows.md)。
