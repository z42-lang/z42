# 产物目录布局（`artifacts/`）

> 对齐：2026-09-17（change `restructure-docs-three-books`）｜ 代码：`scripts/common/xtask_layout.z42`（路径 SoT）、`src/libraries/z42.workspace.toml` 与 `src/compiler/z42.workspace.toml` 的 `[workspace.build]`、`.cargo/config.toml`
>
> 构建步骤本身见[构建编排](build.md)；打包见[打包引擎](packaging.md)。

`artifacts/` 是仓库里**唯一**的构建输出根，整棵目录被 `.gitignore` 忽略，删了都能重生。
**要加一个新产物、或者在找「某个东西被写到哪去了」，读这页。**

## 1. 顶层桶

| 桶 | 装什么 | 谁写 |
|---|---|---|
| `build/` | 编译产物与 per-component 输出，子目录**镜像 `src/`** | `xtask build *`、cargo |
| `packages/` | 组装好的发行包（`z42-<...>-<rid>-<profile>/` 及归档）| `xtask package *` |
| `publish/<comp>/` | 打包的**暂存根**：每个组件按自己的 `dest` 形状产出到这里，包再从这里拷 | staging handler / `z42 publish` |
| `xtask/` | xtask 自己的 zpkg / zsym / cache —— **不在 `build/` 里面** | `z42 publish scripts/xtask.z42.toml` |
| `.scratch/` | 构建与测试的**中间态**，可重生、不进任何包 | 各 stage |
| `bench/` | `e2e.json` / `ab.json` 等测量结果 | `xtask bench` |
| `profile/<name>/` | 火焰图、dhat 报告、counter 摘要、`report.md` | `xtask profile` |
| `tools/` | 构建**下载**的第三方工具（`node`、`android-sdk`、`playwright-browsers`）| `xtask deps install` 与按需自动安装 |
| `test-reports/<platform>/` | `junit.xml`（平台三段测试）| `xtask test platform *` |
| `tmp/` | 自检 harness 的一次性目录 | `xtask test packages` |
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
| `src/toolchain/<comp>/` | `build/toolchain/<comp>/` + `…/publish/` | launcher / builder 等的 dist 与 publish 落点 |
| `src/tests/<rel>` | `build/tests/<rel>` | golden 编译出的 `.zbc` 镜像 |
| （wasm 测试）| `build/wasm-test/` | wasm deployable（agent + bundle + libs）|

**per-member 的产物路径不是硬编码的**：`scripts/common/xtask_layout.z42` 读各 workspace toml 的
`[workspace.build].output_dir` / `cache_dir` 模板（正是 z42c 的 `WorkspaceBuild.PlanLayout` 消费的
同一份）再展开。改 toml 模板，xtask 自动跟上。cargo 侧同理由 `.cargo/config.toml` 的
`target-dir = "artifacts/build/runtime"` 决定——**注意它不带 `<cargo-target>` 这一层**，profile
直接挂在 `runtime/` 下。

xtask 里只有两类路径是「自己发明的约定、没有 toml 归属」，它们**在 `xtask_layout.z42` 里各有一个
单一定义**：扁平 stdlib dist（`_libsFlatDist`）与 cargo 输出目录（`_runtimeOut`）。
一次性的临时目录（`.scratch/*`、`tools/*`、每个测试自己的 staging）**不集中**——它们没有 toml 归属、
没有重复、也没有布局意义，各自留在唯一的使用点。

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

## 3. `.scratch/`：中间态的统一去处

**`artifacts/build/` 只放编译 / publish 产物。** 构建与测试过程中的中间态一律落 `.scratch/`：

| 目录 | 谁用 | 是什么 |
|---|---|---|
| `.scratch/stdlib-run/<profile>` | `build stdlib` 阶段二 | stdlib 的稳定快照，供正在重编 stdlib 的 driver 当 `Z42_LIBS` |
| `.scratch/alllibs/<profile>` | `test stdlib` / `test compiler` units / bench | stdlib + 编译器成员**拷**到一起的单一查找点（driver 的兄弟包与被测 stdlib 必须同处一目录）。曾是 hard-link，relocate-compiler-domain-libs 改成拷贝——它上了关键路径后，别名撕开 zpkg/zsym 配对让 1128 个 golden 假红。**不变式：其中的 stdlib 只来自 flat dist** —— 见下 |
| `.scratch/selfhost-gen1` | `test compiler` | 不动点验证的 gen1 快照 |
| `.scratch/e2e` / `.scratch/xpkg-driver` | e2e / cross-zpkg | 用例工作区 |
| `.scratch/targets/<proj>` | `test targets` | manifest target fixture 输出 |
| `.scratch/incr-reconcile` | `test incremental` | 增量 vs 全量对账的两份产物 |
| `.scratch/fingerprint` | `test fingerprint` | base 与本树编译器各编一份 stdlib 的对比场地 |
| `.scratch/exec-profile` | bench / profile | 执行画像探测 |

判据很简单：**它会不会被别的步骤当作「产物」消费？** 会 → `build/`；只是这一步自己用完就扔 → `.scratch/`。

### `alllibs` 的不变式：stdlib 只来自 flat dist

`relocate-compiler-domain-libs` 之后，**每个编译器成员的 `release/dist/` 里都有一份 stdlib 副本**
（用来让该包自己的依赖闭包自洽）。`alllibs` 要把「stdlib + 编译器成员」拼成一个目录，于是这些
同名副本就成了陷阱：**谁后拷谁赢**。

⇒ 不变式：`alllibs` 里的 stdlib **一律来自 flat dist**（`artifacts/build/libraries/dist/release`），
编译器成员 dist 的同名 zpkg 一个都不得覆盖它。`_assembleAllLibs` 两条手段一起用：成员那一轮走
「不覆盖已存在目标」的拷贝，assemble 完再逐字节对账一次（不匹配就抛，因为 `_copyAll` 是吞异常的
best-effort，拷贝失败也会让旧副本顶上）。

为什么必须是不变式而不是「注意拷贝顺序」：`ci-bootstrap` 的 **`[3/5] build compiler` 在
`[4/5] build stdlib` 之前**，所以编译器域里留下的**必然是上一代** stdlib（冷启动时就是种子那代）。
一旦被它顶掉，`test stdlib` / `test compiler` units / bench 全部在测上一代的 stdlib，而症状离现场
极远 —— PR #955 实测：命令明明被显式喂了 `Z42_LIBS=artifacts/build/libraries/dist/release`，
跑的却是旧 stdlib，只有「断言新改的 stdlib 消息文本」的那几条用例判红，本地同一条命令全绿。

## 4. 清理

`xtask clean [tests|bench|all]`：无参删生产 cache/dist，`all` 全删。
`.scratch/`、`tools/`、`tmp/` 任何时候 `rm -rf` 都安全（前者可重生，`tools/` 会被下次
`deps install` / 按需安装补回）。
`artifacts/` 整棵删掉之后是**冷启动**路径：需要网络下载 nightly 种子，见[xtask](xtask.md) §5。
