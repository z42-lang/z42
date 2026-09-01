# Tasks: fix-hooks-source-scan

> 状态：🟢 阶段4.2 完成（阶段1 #362、阶段2 #367、阶段3 #369、阶段4.1 #371 均已合并；阶段4.1 nightly @ 8538fe3 已发布）→ 本 change 全部完成，随本 PR 归档 | 类型：feat（stdlib z42.project/z42.build；阶段2 含 compiler z42c）
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

## 阶段 4（独立后续）= z42b 发布路径也排除 hooks（User 2026-09-01 定「现在就推进」）

- [x] 4.1 `CompileRequest`（`z42.build/ICompiler.z42`）加 `string[] Excludes` + `int ExcludesCount`
      （additive support；**z42c 尚不读** → 一 nightly）。6 个构造点全传新参：`Pipeline.z42`（空占位）、
      `builder_hooks.z42`（空）、z42ccompiler_tests 4 处（空）。`Z42cCompiler` **不动**（不引用新字段），
      故 `verify-selfhost` 边界不越界（阶段2 nightly 种子 z42.build 无此字段，但 z42c 源不碰它）。
      验证：`test bootstrap`（nightly z42c 编当前源通过）+ cold-seed build compiler/stdlib + 全量 GREEN。
- [x] 4.2 阶段4.1 nightly（@ 8538fe3，其种子 z42.build 已含 `Excludes` 字段）发布后：
      `Pipeline.Compile`（`z42.build`）从 `ctx.Manifest`（`Sources.Exclude` + `HasHooks?HooksDir/**`）
      组装 exclude 填入 req（与 z42c.driver `Main._build` 同款）；`Z42cCompiler`（z42c）读 `req.Excludes`
      传 `DiscoverWithExclude`。→ `z42b build`/`run`/`export` 的 in-process Pipeline 路径产物不含死类。
      **验证（A/B，同一 hooks 工程 + output_dir 在源树外）**：pre-4.2 nightly z42b build → app.zpkg
      `strings|grep ProjectHooks`=**1**；post-4.2 z42b build → **0**（DIAG 证 `HasHooks=true effExcN=1`、
      app 源发现 `srcs=1` 仅 Main.z42、hooks/ 已排除）。cold-seed `build compiler`（种子 z42c 编读
      `req.Excludes` 的当前源，轴②满足）+ `build stdlib` 25/25 EXIT=0。
      - **⚠️ 已知边界（非 4.2 引入、不影响真实消费者）**：`Z42cCompiler` 对 app 源恒用 `**/*.z42` 从
        `req.SourceDir`（工程根）递归发现，`_excluded` 只跳 `/dist/`·`/.cache/` **不跳 `/build/`**。故当
        `[build] output_dir` **落在源树内**（默认 `<src>/artifacts`）时，递归 glob 会捞到 z42b 在 app
        编译前 stage 到 `artifacts/.../build/hooks/` 的 hooks **副本**（rel 以 `artifacts/` 开头，`hooks/**`
        不匹配）→ 死类经副本重新混入。真实消费者（z42.repl/z42.builder/xtask）`output_dir` 均为源树外的
        共享 artifacts 树，不触发；此 gap 属 Z42cCompiler「递归 glob 捞构建产物」的既有问题，与 hooks 排除
        正交，留待独立评估（如 `_excluded` 增排除 `/build/`，或 Z42cCompiler 用 manifest sources 而非硬编码 glob）。

## 备注

- Layer 1（PR #360）已消除 WARN spam；本 change 是构建侧根因（不产死代码），非紧急，从容走多 nightly。
- `[sources] exclude` 从「解析了但忽略」变为「生效」——阶段2 顺带修好的既有 gap（任意工程可用）。
- 目前声明 `[build] hooks` 的工程：xtask（Main 直编 → 阶段2 已修）、z42.repl / z42.builder（`build toolchain`
  经 publish/z42b → 待阶段4）。
