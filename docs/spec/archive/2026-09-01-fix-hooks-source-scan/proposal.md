# Proposal: fix-hooks-source-scan（hooks 目录默认排除 + 修好被忽略的 `[sources] exclude`）

> 状态：🟡 DRAFT（待 User 确认后进入 IMPL） | 类型：feat（stdlib z42.project + compiler z42c）
> 关联：Layer 1 = `fix-projecthooks-vtable-fixup`（PR #360，runtime 侧已消除 WARN 刷屏）。
> 本 change = Layer 2（构建侧根因）。

## 背景 / 问题

运行任意 xtask 命令时曾刷屏 25 次 `Build.ProjectHooks ... fields may be silently wrong`
（Layer 1 已在 runtime 侧消除警报）。**根因在构建侧**：

1. xtask 项目在 `scripts/`，`[sources] include = ["**/*.z42"]` 递归把 `scripts/hooks/hooks.z42`
   扫进 xtask.zpkg → xtask.zpkg 含一个 `Build.ProjectHooks : Z42.Build.BuildHooks` **死类**
   （从不实例化/派发；真实 hook 经 `[build] hooks` 由 z42b 单独编 `hooks/` + ModuleLoader.Load 加载）。
2. xtask **无 `[dependencies]`** → `z42.build`（BuildHooks 所在包）非构建期依赖、未 merge →
   该死类 own-only + 跨包 base → 运行期 fixup 麻烦（Layer 1 已兜）。

**两个可确认的事实（本 change 修）：**
- **A. `[sources] exclude` 目前被完全忽略**：`ManifestLoader._parseSources` 把 `exclude` 解析进
  `Sources.Exclude`（[src/libraries/z42.project/src/ManifestLoader.z42:191-193](../../../../src/libraries/z42.project/src/ManifestLoader.z42)），
  但 `SourceDiscovery.Discover` 只接收 `Include`（[SourceDiscovery.z42](../../../../src/libraries/z42.project/src/SourceDiscovery.z42)
  的 `_excluded` 只硬编码排除 `dist/`/`.cache/`），`Exclude` 从不 apply。→ 手动 `exclude` 今天无效。
- **B. z42c 编译时不知道 hooks 目录特殊**：`[build] hooks` 由 z42b builder 解析
  （[builder.z42](../../../../src/toolchain/builder/core/builder.z42)），`z42.project` 的 `BuildConfig`
  没有 hooks 字段 → z42c 照 `**/*.z42` 把 hooks 目录扫进 app 源。

## 目标

1. **`[sources] exclude` 生效**（修 A）：`SourceDiscovery` 应用 `Sources.Exclude` glob。
2. **声明 `[build] hooks` 即自动排除该目录出 app 源**（修 B，User 定的「默认排除」）：z42c 构建时把
   hooks 目录并入有效 exclude，hooks 源不再进 app zpkg（真实 hook 仍经 `[build] hooks` 单独编，不受影响）。

## 设计

### `SourceDiscovery` = 纯 glob 原语（include − exclude）
新增排除参数，`_excluded` 除硬编码 `dist/`/`.cache/` 外，再按调用方传入的 exclude glob 逐条匹配相对路径。
「hooks 目录自动排除」的**策略**放在调用方（z42c）——把 `build.HooksDir` 并进传给 Discover 的 exclude 列表——
保持 Discover 是无策略的纯原语。

### ⚠️ 多-nightly 硬约束（bootstrap-seed 轴② **+ 轴④**——校正）

> **校正（2026-09-01，阶段2 实施时实测）**：原设计假设「阶段2 同 PR 删旧 `Discover`」（轴②「切调用点+
> 删旧 API 同提交」）。**证伪**：`SourceDiscovery.Discover` 不止被 z42c 源*引用*，它是 z42c 的**运行期
> 自依赖**——编译器自建时 `Main.z42` 在**运行期**调它发现源，而自建流程 `_ensureBootstrapSelfDepLibs`
> 先把当前源 z42.project 预建进 flat-libs、种子 z42c（阶段1 nightly，其二进制 `Main.z42` 仍调 3 参
> `Discover`）随后对着它跑。阶段2 就删 → 种子 z42c 立即 `undefined function Discover$3` 断自举（实测
> 复现）。故**删除是阶段3**（轴④：z42c 运行期自依赖的 stdlib API，比轴②再晚一 nightly）。详见
> [tasks.md「阶段划分校正」](tasks.md)。

`SourceDiscovery.Discover` 是 **z42c 消费的 stdlib API**。z42c 源新用一个 stdlib API，该 API 必须已随
上一个 nightly 发布。故分阶段跨多 nightly：

**阶段 1（本 change 首 PR，additive，z42c 不用）：**
- `z42.project`：
  - `SourceDiscovery` 新增 overload `Discover(projectDir, includes, incCount, excludes, excCount)`，
    `_excluded` 应用 excludes（相对路径 glob 匹配，复用 `dist`/`.cache` 逻辑同款）。**保留**旧
    `Discover(projectDir, includes, incCount)` 三参 overload（委托新版、excludes 空）。
  - `BuildConfig` 加 `bool HasHooks` + `string HooksDir`；`ManifestLoader._parseBuild` 解析 `[build] hooks`。
- z42c / z42b **调用点不动**（仍用旧三参 Discover）。→ 上一 nightly z42c 能编本阶段源。发新 nightly。

**阶段 2（下一 nightly 发布后，第二 PR = 本 PR）：切调用点，**保留** shim**
- z42c `Main.z42:_build`：有效 exclude = `pm.Sources.Exclude ++ (build.HooksDir + "/**" if HasHooks)`，调
  `DiscoverWithExclude`。**已确认编 xtask.zpkg 的路径 = Main 直编**（`z42c build scripts/xtask.z42.toml`；
  workspace 两函数均委托 `_build`，故此一处覆盖）。
- z42c `Z42cCompiler.Compile`（z42b in-process 路径）：切 `DiscoverWithExclude`（空 exclude，行为不变），
  仅为让阶段3 能删 shim。该路径的 hooks 排除留待**阶段4**（需 `CompileRequest` 增 `Excludes` 字段）。
- **不删** shim（校正：见上，删除是阶段3）。

**阶段 3（阶段2 nightly 发布后，第三 PR）：删 shim**
- 删 `SourceDiscovery.Discover` 三参 shim（此时种子 `Main.z42` 已调 `DiscoverWithExclude`、不再引用它）。

**阶段 4（可选）：z42b 发布路径也排除 hooks** —— `CompileRequest.Excludes` additive → nightly →
`Pipeline.Compile` 填 + `Z42cCompiler` 读。让 `z42 publish` 的 repl/builder 也不含死类。

## 验证
- 阶段 1：`xtask test bootstrap`（上一 nightly z42c 编当前 z42c 源无越界）+ 全量 GREEN；新 overload 单测
  （exclude glob 命中/不命中）+ `_parseBuild` hooks 解析单测。行为无变化（z42c 未用）。
- 阶段 2：构建 xtask.zpkg → `strings xtask.zpkg | grep ProjectHooks` = 空；用**旧 runtime**（Layer 1 前的
  get_mut 版，或临时还原）跑 xtask → 0 WARN（证明死类真没进去，而非靠 Layer 1 兜）；全量 GREEN。

## 权衡 / 备注
- **与 Layer 1 的关系**：Layer 1（runtime CoW）已让 spam 消失且是通用防御（任何 seeded-own-only 类型）。
  Layer 2 是根因（不产出死代码），二者互补：即便未来又有别的死类被误扫，Layer 1 兜住不刷屏；Layer 2 让
  xtask 这个具体源头不再产出死类。**故 Layer 2 非紧急**（spam 已由 Layer 1 修），可从容走两 nightly。
- **不做兼容**：**阶段 3** 删旧 Discover shim（非兼容层，是多-nightly 纪律，见 bootstrap-seed 轴④）。
