# scripts

## 职责

仓库的开发 CLI 与启动引导。绝大多数开发命令（build / test / package / deps /
bench）都已收敛到一个自举的 z42 程序 **xtask**：源码是本目录的
`xtask*.z42`（按命令分子目录组织，见末尾），构建为**原生 apphost 可执行**（仓库根 `./xtask`，
内嵌 launcher + `xtask.zpkg`）：

```
z42 publish scripts/xtask.z42.toml             # 构建+部署 → 仓库根 ./xtask
xtask <command> [args]                         # 直接运行
```

xtask 是独立的 z42 应用——它不是通用 `z42` launcher 的一部分（launcher 保持通用
运行时）。冷启动如何先产出 xtask 见下文「冷启动 bootstrap」。

本目录的 `.z42` 全部是 xtask 模块（含 stdlib 构建逻辑 `build/xtask_stdlib.z42`）。唯一
的非 xtask 文件是安装引导脚本：

- **`install-z42.{sh,bat,command}`** —— 下载预编译发行版并安装。运行在「还没有 z42 工具链」的最前端，故保持 shell。
  - 无参数：portable 安装到 `<repo>/.z42`（bootstrap，最常用）。
  - `--system`：managed 安装到 `$Z42_HOME`（默认 `~/.z42`），展开 `bin/launcher/runtimes` 布局，打印 PATH 接入提示。
  - `--dest <dir>`：安装到指定目录（与 `--system` 组合时用 managed 布局，否则用 portable）。

> **冷启动 bootstrap（鸡生蛋的真正破解点）**：`xtask.zpkg` 依赖 stdlib 才能编译，且
> 编译它的 z42c 自身也是 z42 写的。冷树上**下载上一版已发布 nightly 的 z42c 种子**
> （`z42c.driver.zpkg` + stdlib dist），用它 `z42c build scripts/xtask.z42.toml` 产出
> `xtask.zpkg` → 再 `xtask build stdlib`（即 `build/xtask_stdlib.z42`，z42c 从源码
> 重编 stdlib + 自建 z42c）。z42c 只读 `.zsym`、VM 扫目录（读各 zpkg 的 NSPC section 认领
> namespace），都不需要任何 namespace 索引即可编译/运行 xtask，所以这个次序无死锁。
> **工具链全程 z42 自举**。

> **种子从哪来（`build compiler` / `build stdlib` 共用一套解析，CI = 本地）**：冷树无
> in-tree 种子时，`_ensureSeed`（`common/xtask_common.z42`）按 **`Z42_HOME`
> （`--toolchain` 设它，或 launcher/install 设）→ 运行 xtask 的 apphost SDK
> （`Z42_PORTABLE_VM` 反推）→ `./.z42`** 顺序找 SDK-toolchain 布局的根
> （`programs/z42c` + `libs`），把 `programs/z42c` + `libs` 拷进 in-tree 再自建；
> warm 树（已有 in-tree 种子）直接复用——**gen2 字节不动点靠它**，故不覆盖。所以
> `install-z42.sh` 之后本地 `xtask build compiler` 开箱即用；CI 只需设
> `Z42_HOME=<下载的 SDK>`（`.github/actions/ci-bootstrap`），不再手动拷种子。
> managed 布局的 `Z42_HOME`（`runtimes/`，无 `programs/`）不符 SDK-toolchain 布局 →
> 跳过（不误当种子源）；`Z42_LIBS` 显式覆盖仅在其确实含 `z42.core.zpkg` 时生效。

> 所有版本号的唯一真相源是仓库根 `versions.toml`（xtask 经 `Std.Toml` 原生解析，
> 见共享模块 `common/xtask_versions.z42`）。

## 全局选项

在任意子命令前（pre-route，见 `xtask_cli.z42` 的 `_apply*Opt`）：

| 选项 | 作用 |
|------|------|
| `--toolchain <dir>` | 指定 `.z42` 布局的工具链（`programs/z42c` + `libs` + `bin`）；写入 `Z42_HOME`（单一「SDK 根」变量），被 seed / VM / libs 定位器读取 |
| `--verbosity` / `-v <level>` | 输出详略：`q[uiet]` \| `m[inimal]`（默认）\| `n[ormal]` \| `d[etailed]` \| `diag[nostic]`；写入 `Z42_VERBOSITY`，子进程继承 |

**verbosity 级别（累进，MSBuild 风格；实现见 `common/xtask_common.z42`）：**

| 级别 | 打印 |
|------|------|
| `quiet` | 仅错误 |
| `minimal`（默认） | + `▶`/`✔` 每个流程的开始/结束标记（跟踪进度） |
| `normal` | + 每步最终选定的结果（如 `seed: z42c ← <path>`） |
| `detailed` | + 逐候选路径的选择过程（如 SDK 依次试 `Z42_HOME`→`Z42_PORTABLE_VM`→`./.z42`）+ 子工具逐文件输出 |
| `diagnostic` | + 每个流程的耗时（`⏱ … — N ms`） |

示例：`xtask -v detailed build compiler` 会打出冷启动供种的完整候选路径选择过程。

## 命令一览

| 命令 | 触发时机 | 关键依赖 | 主要产物 |
|------|---------|---------|---------|
| `deps install` | **首次 clone / 平台版本变动** | `versions.toml` | rust targets + cargo-ndk + wasm-pack；按平台装 NDK / 构建 SDK |
| `deps check` | 改 `versions.toml` 后对账 | `versions.toml` + 投影文件 | versions.toml ↔ Cargo.toml / build.gradle.kts / Package.swift 一致性 |
| `build stdlib` | 改了 stdlib `.z42` 源 | warm z42c 种子 | `artifacts/build/libraries/dist/release/<lib>.zpkg`（扁平视图，无 namespace 索引） |
| `build compiler` | 改了 z42c 编译器源 | warm z42c 种子 | `artifacts/build/compiler/<member>/release/dist/*.zpkg`（7 个自建成员） |
| `build workload` | 改了 `src/toolchain/workload` 源 | z42c/stdlib（缺则自建） | 4 个 workload lib → `artifacts/build/libraries/dist/release/z42.workload.*.zpkg`（launcher 依赖） |
| `build toolchain` | 改了 launcher/z42b/z42d/z42i 源 | 同上 + 自动 `build workload` | 4 个 apphost `publish <toml>` → **各 toml 的 `[platform.desktop].publish_dir`**（路径从 toml 读，不硬编码） |
| `build test` | 改了 golden 测试源 | z42c/stdlib（缺则自建） | `src/tests/**` → `.zbc` 镜像到 `artifacts/build/tests/`（golden 编译，不重建工具链；含 `regen` 命令旧职能） |
| `build sdk [--out D] [--no-build]` | 组装完整可运行 SDK | z42c/stdlib/z42vm + `build toolchain` | `.z42` 布局：`bin/{z42vm,z42c,z42b,z42d,z42i}` + `z42`(launcher 根) + `programs/*` + `libs/*`；apphost 从各 publish_dir 合并 |
| `package sdk [--profile] [--no-build]` | 打 host SDK 发行包 | `cargo` + z42c | `artifacts/packages/z42-<ver>-<host>-release/{bin,libs,native}`（末尾 SHA-256 invariant） |
| `package runtime [--rid R]` | runtime 包（native+stdlib，平台随 rid） | `cargo` + z42c | host: `z42-runtime-<ver>-<rid>`；平台: `z42-<ver>-<rid>-release` |
| `package workload [--rid R] \| <label> [dist]` | `--rid`/无参：建 per-RID desktop workload；`<label>`：合并 4 个 per-RID → 单 archive | `cargo` | workload 包 / 合并 archive |
| `package index <label> [dist] …` | 生成 release-index.json（launcher 供给契约） | SHA256SUMS | `release-index.json` |
| `bench [--diff]` | 性能基准 / 回归对比 | z42c + hyperfine | 各场景编译/执行耗时；`--diff` 比对两组结果 |
| `test` | **每次 commit / 归档前必跑** | 下面各 stage | 串联 GREEN 验证（e2e + stdlib + compiler；不含 runtime——见下） |
| `test runtime` | 改了 Rust VM (`src/runtime/`) | `cargo` | Rust VM 单测/集成（`cargo test --test-threads=1`；含 zbc/zpkg format 基线）。**不在 `test` gate 内**（signal 测试在受限沙箱会挂）；CI 每腿单独一步 + 按需本地跑 |
| `test e2e [--dir <cat>] [--file <p>] [--mode interp\|jit]` | 跑 `src/tests/` 端到端（golden + cross-zpkg；最常用） | `cargo build` + golden 产物 | 默认全跑；`--dir`/`--file` narrow |
| `test stdlib [lib]` | stdlib 源 / 编译器变动 | `build stdlib` + z42b（z42.builder.zpkg） | 各 stdlib lib 的 `[Test]` 通过率 |
| `test compiler` | z42c 编译器变动 | z42c 自建 | 7/7 自举不动点（gen1==gen2）+ [Test] units + e2e |
| `test dist` | 验证打包后发行版能独立工作 | `package sdk` 产物 | packaged z42c+z42vm 跑 golden 通过率 |
| `test changed [base]` | 增量自测（按改动文件挑 stage） | 上述各命令（in-process 调度） | 仅跑受影响的 stage |

> **构建输出约定（add-build-toolchain, 2026-07-05）**：
> - `artifacts/build/` **只放编译/publish 产物**；构建/测试的中间态（`stdlib-run` 快照、`alllibs`
>   flat 视图、`e2e`/`selfhost-gen1`/`dogfood` 工作区）落 `artifacts/.scratch/`（gitignored、可重生）。
> - **toolchain 组件的输出/publish 路径一律从各 `z42.toml` 读**（`[build].dist_dir`/`output_dir`、
>   `[platform.desktop].publish_dir`，级联默认见 `docs/design/compiler/project.md`）——xtask 不硬编码，
>   改路径只动 toml。定位 helper：`build/xtask_toolchain.z42` 的 `_desktopPublishDir` / `_toolchainZpkg`。

## 各命令处理流程

> 据源码逻辑绘制；函数名标在节点上，可对照源文件（z42c + z42vm 全程自举）。

### `test`（完整 GREEN gate，`test/xtask_test.z42 :: _testAll`）

```
test ──► _testAll
  │  ① regen 构建波 (一次)                _regenForTest → _regenCore
  │       └ build stdlib + z42c + cargo release z42vm + golden .zbc
  │  ② 额外工具链                         _buildDebugVmAndCompression
  │       └ cargo debug z42vm + z42-compression cdylib（runner = z42b，由各 stage 自建）
  ├─► stage e2e goldens (interp)         _testE2eCore  → test/xtask_test_vm.z42
  ├─► stage e2e cross-zpkg               _testCrossZpkgCore → test/xtask_test_cross.z42
  ├─► stage stdlib [Test]                _testLibCore  → test/xtask_test_lib.z42
  ├─► stage compiler                     _testCompiler → build/xtask_compiler.z42
  │       └ 自举不动点 7/7 + [Test] units + e2e (build/xtask_compiler_e2e.z42)
  └─► stage vscode-syntax                _testVscodeSyntax → grammar ↔ Lexer 关键字防漂移
  ──► ✅ GREEN（任一 stage 失败立即停）
  # CI 只为并行把 stdlib/cross-zpkg 用 `--skip` 下放到独立 shard job（见 workflow/ci.md）；
  # stage 组成的唯一权威清单见 book/dev/test-gate.md。
```

### `build stdlib`（`build/xtask_stdlib.z42 :: _buildStdlibCore`）

```
build stdlib ──► _buildStdlibCore
  ① 校验 warm 种子 (z42c.driver.zpkg + stdlib dist 存在)   缺 → _ensureSeed 冷启动供种 (SDK)
  ② z42c 自建 7 个 z42c 成员    _buildCompilerViaZ42c (build/xtask_compiler.z42)
       └ z42c build --workspace（driver dist 自包含 6 个 z42c.* 兄弟包）
  ③ 快照 stdlib → .stdlib-run (只 stdlib；driver 运行期 Std.* 需稳定副本)   _copyAll(flatDir, .stdlib-run)
  ④ 直跑自包含 z42c.driver build --workspace --release    CWD=src/libraries, interp, Z42_LIBS=.stdlib-run
       └ per-member dist 覆盖 canonical 布局
  ⑤ verify 产物 + flat view (hard-link)    _assembleStdlibFlatView → libraries/dist/release
```

### `build test`（`build/xtask_test_assets.z42 :: _buildTest → _regenGolden`）

```
build test ──► _buildTest
  ① _ensureToolchainDeps: 缺则自建 z42c/stdlib/z42vm（build-if-missing）
  ② _regenGolden:
       枚举 golden 三种布局 (src/tests/<cat>/<name>/source.z42 · stdlib tests · flat *.z42)
       └ 排除 errors/parse/cross-zpkg (预期失败) + [Test]/[Benchmark] 目录
       并行批量 (8/批) z42vm 跑 z42c.driver --emit-zbc → .zbc
       └ zbc-format 类就地覆盖 (git diff = 格式漂移)；其余 → artifacts 镜像
```

> `regen` 命令已并入 `build test`（redesign-xtask-test）。`_regenCore`（rebuild
> stdlib+runtime+goldens）仍作为 `test` gate 的 build-wave（`_regenForTest`）保留。
> 格式 bump 后重生 fixture：`build compiler && build stdlib && build test`。

### `package sdk` / `package runtime`（`package/xtask_package.z42`）

```
package sdk [--profile P]            ──► host SDK 包（桌面 RID，含 host）
package runtime [--rid R] [--profile P] ──► 按 RID 分类 dispatch
  ├─ desktop  → package/xtask_package_desktop.z42
  │     z42c 种子 + z42vm + libz42 + C-ABI headers + stdlib zpkg + manifest + 原生 apphost
  ├─ ios      → package/xtask_package_ios.z42      cargo rustc (staticlib) + SwiftPM facade
  ├─ android  → package/xtask_package_android.z42  cargo-ndk rustc (cdylib) + Gradle facade
  └─ wasm     → package/xtask_package_wasm.z42     cargo rustc (wasm) + npm facade
  ──► artifacts/packages/z42-<ver>-<rid>-<config>/ + manifest.toml + SHA-256 invariant
```

### `deps`（三正交子命令；`install/xtask_install.z42` + `xtask_deps.z42`）

```
deps check [--os P]     ──► _depsCheckRun    唯一只读校验：presence + versions.toml drift
                            （presence 仅显式 --os 时计入退出码；drift 恒致败 —— CI 裸跑兼容）
deps install [--os P] [--force] [vscode] ──► _depsInstall   纯安装
  ├─ 无 --os       当前 host 基础（跨平台 rust/node 检查，不装交叉栈）
  ├─ --os android  rust targets + cargo-ndk + JDK + 构建 SDK   install/xtask_install_android.z42
  ├─ --os ios      rust targets + Xcode
  ├─ --os wasm     rust targets + wasm-pack + hermetic node
  └─ vscode        编辑器资产 → <repo>/.vscode/extensions（工作区本地扩展）install/xtask_install_vscode.z42
deps env [--os android] ──► _depsEnv         可 eval 的 export（ANDROID_NDK_HOME）
```

用到才装（零命令面）：android emulator tier（~4GB）由 `test platform android run`
自动装；node 兜底由 wasm 测试自动装。

## 典型流程

**首次 clone / 新 dev 环境**：
```bash
xtask deps check                    # 看缺什么 + 校验投影文件跟 versions.toml 一致
xtask deps install --os android     # 需要打 android 时再装该平台必备
xtask deps check --os android       # 严格校验该平台依赖已就位（缺失即非 0）
```

**commit 前 / 归档前（必跑，workflow 阶段 8 全绿入口）**：
```bash
xtask test               # 串联 e2e + cross-zpkg + stdlib + compiler 全 stage（runtime 独立，见 test runtime）
```

> 不要单独只跑其中一个 stage 就当作通过 —— 历史上 cross-zpkg subclass catch
> bug 就是因为 `test stdlib` 不在默认 GREEN 路径里，每次 spec 验证都被漏掉。

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
xtask package sdk             # 打 host-RID 发行包
xtask test dist               # 端到端验证发行包（packaged z42c/z42vm 跑 golden + launcher smoke）
```

## 源码结构（按命令分子目录）

> namespace 扁平（`Z42Xtask`），跨文件按裸名调用，子目录纯为组织；`xtask.z42.toml` 的
> `[sources].include = ["**/*.z42"]` 递归收录全部模块。

```
scripts/
├── xtask.z42            入口 Main + 顶层 handler（run / clean / feature-matrix）
├── xtask.z42.toml       工程清单（glob include；output → artifacts/xtask/）
├── xtask_cli.z42        Std.Cli 命令树构建 + dispatch（每层 -h 自动生成）
├── xtask_deps.z42       deps check 版本漂移检查
├── xtask_bench.z42      bench 基准 / --diff 回归对比
├── common/             共享基建（非某个命令专属）
│   ├── xtask_common.z42     _root/_exec/path/cargo/toolchain 选择器
│   ├── xtask_versions.z42   versions.toml 读取器（_vget/_vRead/...）
│   └── xtask_golden.z42     golden 枚举 / 入口推导（多 test stage + build test 复用）
├── build/              build stdlib / compiler / test-assets + 自举边界检查
│   ├── xtask_stdlib.z42         build stdlib（z42c build --workspace + 扁平视图）
│   ├── xtask_compiler.z42       build/test compiler（自建 + 不动点 + units）
│   ├── xtask_compiler_e2e.z42   z42c 自举 e2e oracle 套件（div-by-zero 验证）
│   ├── xtask_test_assets.z42    build test（golden .zbc 编译；_buildTest / _regenGolden；供 test gate）
│   └── xtask_bootstrap_check.z42 上一版 nightly z42c 能否编当前源（分阶段纪律边界检查）
├── test/               test 命令族
│   ├── xtask_test.z42           runtime/e2e/dist/all 编排 + shard 解析
│   ├── xtask_test_lib.z42       stdlib [Test]/[Benchmark] harness（发现/依赖/批量编译运行）
│   ├── xtask_test_vm.z42        e2e golden 跑分（+ --dir/--file 子选择）
│   ├── xtask_test_cross.z42     e2e cross-zpkg 多包
│   ├── xtask_test_dist.z42      发行包 e2e
│   ├── xtask_test_changed.z42   按改动文件挑 stage
│   └── xtask_test_{platform,wasm,ios,android,desktop}.z42  平台 3 段测试（build/assets/run）
├── package/            package 各 RID 类别 + 发行档组装
│   ├── xtask_package{,_desktop,_ios,_android,_wasm}.z42
│   └── xtask_release.z42        package workload(merge)/index 发行档组装
└── install/            deps install 各平台 / SDK 安装
    └── xtask_install{,_android}.z42
```

每个命令的详细 Usage 见 `xtask -h`（每层子命令 `-h` 自动生成）与各源文件顶部注释。

## 迭代注意点（自举边界与验证）

> 按改动类型的完整验证速查（改了编译器/stdlib/VM/xtask 各跑什么）：
> [`docs/workflow/testing/verify-by-change.md`](../docs/workflow/testing/verify-by-change.md)。

改本目录（乃至全仓）代码时，最容易踩的是**自举边界**：

- **语法/格式越界**：当前源码**不得使用比上一版已发布 nightly z42c 更新的语法或 zbc·zpkg 格式**
  ——新语法必须"support 先行、晚一个 nightly 再 use"，否则跨版本自举断链。改动 z42c
  （lexer/parser/codegen/格式 writer）或引入新语法后**必跑**边界检查：

  ```bash
  xtask test bootstrap      # 下载 nightly z42c 编当前源，验证无越界
  ```

- **删种子/兜底路径**：必须与"为所有 cold-start 入口供种"作为**同一原子变更**；本地只能验
  warm 路径，cold 路径的全绿以 CI 为准。完整纪律见 [`.claude/rules/bootstrap-seed.md`](../.claude/rules/bootstrap-seed.md)。

- **改 xtask 源码后**：先重建再验证——`z42c build scripts/xtask.z42.toml --release` 产新
  `xtask.zpkg`，再跑 GREEN gate。

**commit 前验证**（GREEN 标准）：

```bash
xtask test                # 完整 gate（e2e / cross-zpkg / stdlib / compiler 全 stage）
```

iteration 期可用 `test changed`（按改动挑 stage）或单跑某 stage（`test e2e --dir/--file` /
`test stdlib <lib>` / `--no-build`）缩窄加速，但 **commit 前必须完整 gate**。改打包系统时
另跑 `test packages`（parse + staging + assembly 三层自检合一）；改增量编译
（IncrementalBuild / CacheStore / ZbcReader / IncrementalDriver）时另跑
`test incremental`（暴力对账器：语料逐文件 touch，断言增量产物 == 全量产物逐字节 + 计时）。

## 关联文档

本 README 是**基础层**（干什么 / 怎么用 / 怎么开发）；设计思路与实现机制（深入层）见 book 工具链部分：

- [xtask：自举 dev CLI](../docs/book/src/dev/xtask.md) —— 自举链路、CLI 分发架构、`--toolchain` 机制
- [构建编排（build / regen）](../docs/book/src/dev/build.md) —— z42c 七包自建拓扑、stdlib 三阶段、不动点验证、golden 重生
- [开发基础设施概览](../docs/book/src/dev/README.md) —— xtask 三条产线全景（在线版：<https://z42-lang.github.io/z42/dev/>）
- 操作手册（构建/测试的完整流程）：[`docs/workflow/`](../docs/workflow/)
