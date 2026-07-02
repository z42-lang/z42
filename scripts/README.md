# scripts

## 职责

仓库的开发 CLI 与启动引导。绝大多数开发命令（build / test / package / deps /
regen / audit / bench / release）都已收敛到一个自举的 z42 程序 **xtask**：源码是本目录的
`xtask*.z42`（按命令分子目录组织，见末尾），编译成单个 `artifacts/xtask/xtask.zpkg`，
经 launcher 调用：

```
z42c build scripts/xtask.z42.toml --release    # 构建 → artifacts/xtask/xtask.zpkg
z42 xtask.zpkg <command> [args]                # 经 launcher 运行
./xtask <command> [args]                       # 或经原生 apphost（z42 apphost build 产仓库根 ./xtask）
```

xtask 本身是一个 z42 程序，由 launcher **运行**——它不是通用 `z42` launcher 的一
部分（launcher 保持通用运行时）。冷启动如何先产出 `xtask.zpkg` 见下文「冷启动
bootstrap」。

本目录的 `.z42` 全部是 xtask 模块（含 stdlib 构建逻辑 `build/xtask_stdlib.z42`）。唯一
的非 xtask 文件是安装引导脚本：

- **`install-z42.{sh,bat,command}`** —— 下载预编译发行版并安装。运行在「还没有 z42 工具链」的最前端，故保持 shell。
  - 无参数：portable 安装到 `<repo>/.z42`（bootstrap，最常用）。
  - `--system`：managed 安装到 `$Z42_HOME`（默认 `~/.z42`），展开 `bin/launcher/runtimes` 布局，打印 PATH 接入提示。
  - `--dest <dir>`：安装到指定目录（与 `--system` 组合时用 managed 布局，否则用 portable）。

> **冷启动 bootstrap（鸡生蛋的真正破解点）**：`xtask.zpkg` 依赖 stdlib 才能编译，且
> 编译它的 z42c 自身也是 z42 写的。冷树上**下载上一版已发布 nightly 的 z42c 种子**
> （`z42c.driver.zpkg` + stdlib dist），用它 `z42c build scripts/xtask.z42.toml` 产出
> `xtask.zpkg` → 再 `z42 xtask.zpkg build stdlib`（即 `build/xtask_stdlib.z42`，z42c 从源码
> 重编 stdlib + 自建 z42c）。z42c 只读 `.zsym`、VM 扫目录（读各 zpkg 的 NSPC section 认领
> namespace），都不需要任何 namespace 索引即可编译/运行 xtask，所以这个次序无死锁。
> **工具链全程 z42 自举**。

> 所有版本号的唯一真相源是仓库根 `versions.toml`（xtask 经 `Std.Toml` 原生解析，
> 见共享模块 `common/xtask_versions.z42`）。

## 命令一览

| 命令 | 触发时机 | 关键依赖 | 主要产物 |
|------|---------|---------|---------|
| `deps install` | **首次 clone / 平台版本变动** | `versions.toml` | rust targets + cargo-ndk + wasm-pack；按平台装 NDK / 构建 SDK |
| `deps check` | 改 `versions.toml` 后对账 | `versions.toml` + 投影文件 | versions.toml ↔ Cargo.toml / build.gradle.kts / Package.swift 一致性 |
| `build stdlib` | 改了 stdlib `.z42` 源 | warm z42c 种子 | `artifacts/build/libraries/dist/release/<lib>.zpkg`（扁平视图，无 namespace 索引） |
| `build compiler` | 改了 z42c 编译器源 | warm z42c 种子 | `artifacts/build/z42c/<member>/release/dist/*.zpkg`（7 个自建成员） |
| `build package [release\|debug] [--rid R]` | 准备发行 / 测发行包 | `cargo` + z42c | `artifacts/packages/z42-<ver>-<rid>-<config>/{bin,libs,native}`（末尾自动跑 SHA-256 invariant） |
| `regen` | 编译器变更使 `.zbc` 基线漂移 | z42c + z42vm | run-golden 按组件镜像到 `artifacts/build/`（gitignored，不污染 src）；committed 字节基线 `src/tests/zbc-format/*/source.zbc` 就地重写 |
| `audit` | 新增 test `source.z42` | `z42.regex` | 自动补缺失的 `using` 声明 |
| `bench [--diff]` | 性能基准 / 回归对比 | z42c + hyperfine | 各场景编译/执行耗时；`--diff` 比对两组结果 |
| `test` | **每次 commit / 归档前必跑** | 下面各 stage | 串联全部 GREEN 验证 |
| `test vm [interp\|jit]` | 跑 VM 端到端 golden（最常用） | `cargo build` + regen 产物 | interp / jit 通过率 |
| `test cross-zpkg` | 跨包路径 / 元数据相关变更 | `cargo build` + z42c | target/ext/main 三方协作通过率 |
| `test stdlib [lib]` | stdlib 源 / 编译器变动 | `build stdlib` + z42b（z42.builder.zpkg） | 各 stdlib lib 的 `[Test]` 通过率 |
| `test compiler` | z42c 编译器变动 | z42c 自建 | 7/7 自举不动点（gen1==gen2）+ [Test] units + e2e |
| `test dist` | 验证打包后发行版能独立工作 | `build package` 产物 | packaged z42c+z42vm 跑 golden 通过率 |
| `test changed [base]` | 增量自测（按改动文件挑 stage） | 上述各命令（in-process 调度） | 仅跑受影响的 stage |
| `release …` | nightly / 发行打包编排 | 各 RID workload | 合并 desktop workload / 生成 release-index.json |

## 各命令处理流程

> 据源码逻辑绘制；函数名标在节点上，可对照源文件（z42c + z42vm 全程自举）。

### `test`（完整 GREEN gate，`test/xtask_test.z42 :: _testAll`）

```
test ──► _testAll
  │  ① regen 构建波 (一次)                _regenForTest → _regenCore
  │       └ build stdlib + z42c + cargo release z42vm + golden .zbc
  │  ② 额外工具链                         _buildDebugVmAndCompression
  │       └ cargo debug z42vm + z42-compression cdylib（runner = z42b，由各 stage 自建）
  ├─► stage VM goldens (interp)          _testVmCore   → test/xtask_test_vm.z42
  ├─► stage cross-zpkg                   _testCrossZpkgCore → test/xtask_test_cross.z42
  ├─► stage stdlib [Test]                _testLibCore  → test/xtask_test_lib.z42
  └─► stage compiler                     _testCompiler → build/xtask_compiler.z42
          └ 自举不动点 7/7 + [Test] units + e2e (build/xtask_compiler_e2e.z42)
  ──► ✅ GREEN（任一 stage 失败立即停）
```

### `build stdlib`（`build/xtask_stdlib.z42 :: _buildStdlibCore`）

```
build stdlib ──► _buildStdlibCore
  ① 校验 warm 种子 (z42c.driver.zpkg + stdlib dist 存在)   缺 → 报错引导下载 nightly
  ② z42c 自建 7 个 z42c 成员    _buildCompilerViaZ42c (build/xtask_compiler.z42)
       └ 种子 z42c 按 topo 序逐个编译 z42c.*，新 sibling 累积进 runlibs
  ③ run-libs 组装 (stdlib + z42c 7 包, copy)               _copyAll → dogfood/run-release
  ④ z42vm 跑 z42c.driver build --workspace --release       CWD=src/libraries, interp
       └ per-member dist 覆盖 canonical 布局
  ⑤ verify 产物 + flat view (hard-link)    _assembleStdlibFlatView → libraries/dist/release
```

### `regen`（`xtask_regen.z42 :: _regenCore → _regenGolden`）

```
regen [release] [--no-stdlib] ──► _regenCore
  ① (除非 --no-stdlib) build stdlib          _buildStdlib
  ② cargo build release z42vm                 _buildRuntime
  ③ _regenGolden:
       枚举 golden 三种布局 (src/tests/<cat>/<name>/source.z42 · stdlib tests · flat *.z42)
       └ 排除 errors/parse/cross-zpkg (预期失败) + [Test]/[Benchmark] 目录
       并行批量 (8/批) z42vm 跑 z42c.driver --emit-zbc → .zbc
       └ zbc-format 类就地覆盖 (git diff = 格式漂移)；其余 → artifacts 镜像
```

### `build package`（`package/xtask_package.z42 :: _buildPackageCore`）

```
build package [--rid R] [release|debug] ──► 按 RID 分类 dispatch
  ├─ desktop  → package/xtask_package_desktop.z42
  │     z42c 种子 + z42vm + libz42 + C-ABI headers + stdlib zpkg + manifest + 原生 apphost
  ├─ ios      → package/xtask_package_ios.z42      cargo rustc (staticlib) + SwiftPM facade
  ├─ android  → package/xtask_package_android.z42  cargo-ndk rustc (cdylib) + Gradle facade
  └─ wasm     → package/xtask_package_wasm.z42     cargo rustc (wasm) + npm facade
  ──► artifacts/packages/z42-<ver>-<rid>-<config>/ + manifest.toml + SHA-256 invariant
```

### `deps install`（`install/xtask_install.z42 :: _depsInstall`）

```
deps install [--os P] [--check] [--print-env] ──► _depsInstall
  ├─ 无 --os    跨平台 toolchain 存在性检查 (rust / node)
  ├─ --os android  TIER1: rust targets + cargo-ndk + JDK + 构建 SDK   install/xtask_install_android.z42
  ├─ --os ios      TIER1: rust targets + Xcode
  ├─ --os wasm     TIER1: rust targets + wasm-pack
  ├─ node          TIER2 (惰性): Node LTS (wasm js / playground)
  └─ android-emulator  TIER2: emulator + system-image + AVD + Gradle
  --check 只 verify 不装；--print-env 输出可 eval 的 export
```

## 典型流程

**首次 clone / 新 dev 环境**：
```bash
z42 xtask.zpkg deps install --check          # 看缺什么
z42 xtask.zpkg deps install                  # 装跨平台 toolchain
z42 xtask.zpkg deps install --os android     # 需要打 android 时再装该平台必备
z42 xtask.zpkg deps install --drift          # 校验投影文件跟 versions.toml 一致
```

**commit 前 / 归档前（必跑，workflow 阶段 8 全绿入口）**：
```bash
z42 xtask.zpkg test               # 串联 vm + cross-zpkg + stdlib + compiler 全 stage
```

> 不要单独只跑其中一个 stage 就当作通过 —— 历史上 cross-zpkg subclass catch
> bug 就是因为 `test stdlib` 不在默认 GREEN 路径里，每次 spec 验证都被漏掉。

**日常开发循环（最高频）**：
```bash
z42 xtask.zpkg regen              # 改了编译器后重生 .zbc 基线
z42 xtask.zpkg test vm            # 跑 VM 端到端（interp + jit）
z42 xtask.zpkg test changed       # 或只测受改动影响的 stage
```

**改了 stdlib `.z42` 源**：
```bash
z42 xtask.zpkg build stdlib       # 重编 stdlib zpkg（扁平视图，无 namespace 索引）
z42 xtask.zpkg test vm
```

**完整发行验证**：
```bash
z42 xtask.zpkg build package release   # 打 host-RID 发行包
z42 xtask.zpkg test dist               # 端到端验证发行包（packaged z42c/z42vm 跑 golden + launcher smoke）
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
├── xtask_regen.z42      regen 重生 .zbc 基线
├── xtask_audit.z42      audit 补缺失 using
├── xtask_bench.z42      bench 基准 / --diff 回归对比
├── xtask_release.z42    release 打包编排（assemble-desktop-workload / gen-release-index）
├── common/             共享基建（非某个命令专属）
│   ├── xtask_common.z42     _root/_exec/path/cargo/toolchain 选择器
│   ├── xtask_versions.z42   versions.toml 读取器（_vget/_vRead/...）
│   └── xtask_golden.z42     golden 枚举 / 入口推导（多 test stage + regen 复用）
├── build/              build stdlib / compiler + 自举边界检查
│   ├── xtask_stdlib.z42         build stdlib（z42c build --workspace + 扁平视图）
│   ├── xtask_compiler.z42       build/test compiler（自建 + 不动点 + units）
│   ├── xtask_compiler_e2e.z42   z42c 自举 e2e oracle 套件（div-by-zero 验证）
│   └── xtask_bootstrap_check.z42 上一版 nightly z42c 能否编当前源（分阶段纪律边界检查）
├── test/               test 命令族
│   ├── xtask_test.z42           vm/cross/dist/all 编排 + shard 解析
│   ├── xtask_test_lib.z42       stdlib [Test]/[Benchmark] harness（发现/依赖/批量编译运行）
│   ├── xtask_test_vm.z42        VM golden 跑分
│   ├── xtask_test_cross.z42     cross-zpkg e2e
│   ├── xtask_test_dist.z42      发行包 e2e
│   ├── xtask_test_changed.z42   按改动文件挑 stage
│   └── xtask_test_{platform,wasm,ios,android,desktop}.z42  平台 3 段测试（build/assets/run）
├── package/            build package 各 RID 类别
│   └── xtask_package{,_desktop,_ios,_android,_wasm}.z42
└── install/            deps install 各平台 / SDK 安装
    └── xtask_install{,_android}.z42
```

每个命令的详细 Usage 见 `z42 xtask.zpkg --help`（每层子命令 `-h` 自动生成）与各源文件顶部注释。

## 迭代注意点（自举边界与验证）

改本目录（乃至全仓）代码时，最容易踩的是**自举边界**：

- **语法/格式越界**：当前源码**不得使用比上一版已发布 nightly z42c 更新的语法或 zbc·zpkg 格式**
  ——新语法必须"support 先行、晚一个 nightly 再 use"，否则跨版本自举断链。改动 z42c
  （lexer/parser/codegen/格式 writer）或引入新语法后**必跑**边界检查：

  ```bash
  z42 xtask.zpkg bootstrap-check     # 下载 nightly z42c 编当前源，验证无越界
  ```

- **删种子/兜底路径**：必须与"为所有 cold-start 入口供种"作为**同一原子变更**；本地只能验
  warm 路径，cold 路径的全绿以 CI 为准。完整纪律见 [`.claude/rules/bootstrap-seed.md`](../.claude/rules/bootstrap-seed.md)。

- **改 xtask 源码后**：先重建再验证——`z42c build scripts/xtask.z42.toml --release` 产新
  `xtask.zpkg`，再跑 GREEN gate。

**commit 前验证**（GREEN 标准）：

```bash
z42 xtask.zpkg test                # 完整 gate（vm / cross-zpkg / lib / compiler 全 stage）
```

iteration 期可用 `--scope=runtime|compiler|stdlib|auto` 缩窄加速，但 **commit 前必须完整
gate**。改打包系统时另跑 `test packages-config / packages-staging / packages-assemble` 自检。

## 关联文档

本 README 是**基础层**（干什么 / 怎么用 / 怎么开发）；设计思路与实现机制（深入层）见 book 工具链部分：

- [xtask：自举 dev CLI](../docs/book/src/dev/xtask.md) —— 自举链路、CLI 分发架构、`--toolchain` 机制
- [构建编排（build / regen）](../docs/book/src/dev/build.md) —— z42c 七包自建拓扑、stdlib 三阶段、不动点验证、golden 重生
- [开发基础设施概览](../docs/book/src/dev/README.md) —— xtask 三条产线全景（在线版：<https://codesigner-ui.github.io/z42/dev/>）
- 操作手册（构建/测试的完整流程）：[`docs/workflow/`](../docs/workflow/)
