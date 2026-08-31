# Tasks: fix-hooks-source-scan

> 状态：🟡 阶段1 IMPL 中 | 类型：feat（stdlib z42.project；阶段2 含 compiler z42c）
> 设计见 [proposal.md](proposal.md)。⚠️ 两-nightly：阶段1 additive → nightly → 阶段2 z42c 切用+删旧。

## 阶段 1（additive，z42c 不用；本 nightly 可落）

- [ ] 1.1 `SourceDiscovery.z42`：新增 overload `Discover(projectDir, includes, incCount, excludes, excCount)`，
      `_excluded` 增加「相对路径匹配任一 exclude glob」分支；旧三参 `Discover` 保留，委托新版（excludes 空）。
- [ ] 1.2 `BuildConfig.z42`：加 `bool HasHooks` + `string HooksDir` 字段 + 构造参数。
- [ ] 1.3 `ManifestLoader._parseBuild`：解析 `[build] hooks`（缺省 HasHooks=false）。
- [ ] 1.4 单测：`SourceDiscovery` exclude glob 命中/不命中（含 `hooks/**`）；`_parseBuild` hooks 解析。
- [ ] 1.5 z42c / z42b 调用点**不动**（仍旧三参）；确认 `xtask test bootstrap`（上一 nightly 编当前源）无越界。
- [ ] 1.6 GREEN（全量）+ 自举字节不动点。

## 阶段 2（下一 nightly 发布后，第二 PR）

- [ ] 2.1 z42c `Main.z42:_build`：有效 exclude = `Sources.Exclude ++ (HooksDir if HasHooks)` → 调新 overload。
- [ ] 2.2 确认编 xtask.zpkg 的实际路径（Main 直编 vs z42b in-process `Z42cCompiler`）；若经 z42b，`CompileRequest`
      加 exclude、z42b 传 hooks 目录。
- [ ] 2.3 删旧三参 `Discover` overload（同 PR 切调用点 + 删旧 API）。
- [ ] 2.4 验证：`strings xtask.zpkg | grep ProjectHooks` 空；旧 get_mut runtime 跑 xtask 0 WARN；全量 GREEN。

## 备注

- Layer 1（PR #360）已消除 spam；本 change 是根因（不产死代码），非紧急，从容走两 nightly。
- `[sources] exclude` 从「解析了但忽略」变为「生效」——顺带修好的既有 gap。
