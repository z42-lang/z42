# Tasks: fix-hooks-source-scan

> 状态：🟡 阶段3 IMPL 中（阶段1 = PR #362、阶段2 = PR #367 均已合并；阶段2 nightly @ a680cbd 已发布） | 类型：feat（stdlib z42.project；阶段2 含 compiler z42c）
> 设计见 [proposal.md](proposal.md)。⚠️ **三**阶段跨三 nightly（原以为两阶段——见下「阶段划分校正」）。

## ⚠️ 阶段划分校正（2026-09-01，阶段2 实施时发现）

原 proposal / 本 tasks 假设「阶段2 同 PR 删旧三参 `Discover`」。**实测证伪**：`SourceDiscovery.Discover`
不只是「z42c 源引用的 stdlib API」（axis②），它是 **z42c 的运行期自依赖**——编译器自建时其
`Main.z42` **在运行期调用**本方法发现源。自建流程 `_ensureBootstrapSelfDepLibs`
（[scripts/build/xtask_compiler.z42](../../../../scripts/build/xtask_compiler.z42)）在 workspace
self-build **前**先把**当前源** z42.project 预建进 flat-libs，种子 z42c（阶段1 nightly，其编译进二进制的
`Main.z42` 仍调 3 参 `Discover`）随后对着这份 flat 跑。**若阶段2 就删 `Discover`**，种子 z42c 一加载
新 z42.project 立即 `undefined function Discover$3` → 断自举（实测复现：`build z42.build/z42.ir/z42c.core`
全 `undefined function Discover$3`）。

→ **删除是阶段3**：等阶段2 nightly 发布（其种子 `Main.z42` 已改调 `DiscoverWithExclude`、不再引用
`Discover`）后，阶段3 才能安全删 shim。这是 bootstrap-seed **轴④**（z42c 运行期自依赖一个 stdlib
库/API）的体现，比 proposal 里假设的轴②晚一个 nightly。

## 阶段 1（additive，z42c 不用）= ✅ PR #362 已合并（nightly @ 039822d 已发布）

- [x] 1.1 `SourceDiscovery.z42`：新增 `DiscoverWithExclude(pd, inc, incCount, exc, excCount)`，`_excluded`
      应用 exclude glob；旧三参 `Discover` 保留、委托新方法（**刻意独立方法名**，不加同名 overload——
      否则 `Discover` 符号 mangle 成 `Discover$3` 断自举）。
- [x] 1.2 `BuildConfig.z42`：`bool HasHooks` + `string HooksDir`。
- [x] 1.3 `ManifestLoader._parseBuild`：解析 `[build] hooks`。
- [x] 1.4 单测：manifest_roundtrip 加断言。
- [x] 1.5 z42c/z42b 调用点不动；bootstrap 无越界。
- [x] 1.6 GREEN + 自举字节不动点。

## 阶段 2（本 PR，下一 nightly 发布后可落）= z42c 切用 exclude，**保留** shim

- [x] 2.1 z42c `Main.z42:_build`：有效 exclude = `pm.Sources.Exclude ++ (HooksDir + "/**" if HasHooks)` →
      调 `DiscoverWithExclude`。（workspace 两路径均委托 `_build`，故此一处覆盖单包 + workspace 成员 + path 依赖。）
- [x] 2.2 确认编 xtask.zpkg 的实际路径 = **Main 直编**（`z42c build scripts/xtask.z42.toml`，经
      `_z42cBuildPackage` 驱动路径；workspace 两函数均委托 `_build`）。z42b in-process `Z42cCompiler`
      是 `z42 publish` / compile-then-test 路径（编 repl/builder/publish-xtask），其 hooks 排除**留待阶段4**
      （需 `CompileRequest` 增 `Excludes` 字段 + z42c 读取 → 又一 nightly）。本阶段 `Z42cCompiler` 切
      `DiscoverWithExclude`（空 exclude，行为不变），仅为让阶段3 能删 shim。
- [x] 2.3 **不删** shim（校正：删除是阶段3，见上）；两调用点切 `DiscoverWithExclude`、`Discover` shim 保留。
- [x] 2.4 验证：`strings xtask.zpkg | grep ProjectHooks` = **空**（新 z42c 建：52 文件 vs 基线 53，hooks/ 已排除）；
      新 xtask `--help` 0 WARN；self-host gen 不动点通过。全量 GREEN（进行中）。

## 阶段 3（阶段2 nightly 发布后，第三 PR）= 删 shim ✅ 本 PR

- [x] 3.1 删 `SourceDiscovery.Discover` 三参 shim（种子 `Main.z42` 已调 `DiscoverWithExclude`、不再引用它）。
- [x] 3.2 验证：**cold-seed 自建**——用阶段2 nightly SDK（`main @ a680cbd`，其 driver 只含
      `DiscoverWithExclude$5`、无 `Discover$3` 调用）作 in-tree 种子，`build compiler`（触发
      `_ensureBootstrapSelfDepLibs` 预建当前源 z42.project、种子 z42c 对其自建）+ `build stdlib`
      （25/25）均 EXIT=0、无 `undefined function Discover$3`；新建 z42.project.zpkg 只含
      `DiscoverWithExclude$5`。全量 GREEN（含 stage 5 self-host 不动点 gen1==gen2）+ `test bootstrap`
      边界通过。

## 阶段 4（可选，独立后续）= z42b 发布路径也排除 hooks

- [ ] 4.1 `CompileRequest` 加 `string[] Excludes` + count（additive support；z42c 不读 → 一 nightly）。
- [ ] 4.2 nightly 后：`Pipeline.Compile` 从 manifest 组装 exclude 填入 req；`Z42cCompiler` 读 `req.Excludes`
      传 `DiscoverWithExclude`。→ `z42 publish` 的 repl/builder/xtask 也不含死类。

## 备注

- Layer 1（PR #360）已消除 WARN spam；本 change 是构建侧根因（不产死代码），非紧急，从容走多 nightly。
- `[sources] exclude` 从「解析了但忽略」变为「生效」——阶段2 顺带修好的既有 gap（任意工程可用）。
- 目前声明 `[build] hooks` 的工程：xtask（Main 直编 → 阶段2 已修）、z42.repl / z42.builder（`build toolchain`
  经 publish/z42b → 待阶段4）。
