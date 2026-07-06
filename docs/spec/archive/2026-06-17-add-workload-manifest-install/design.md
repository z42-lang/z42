# Design: add-workload-manifest-install（B2-4）

## Architecture

```
CI (release.yml, per tag)                       launcher (z42 workload install <wl>)
─────────────────────────                       ────────────────────────────────────
package matrix (9 RID)                           resolve baseUrl
  xtask package --rid <rid>                         <tag> = nightly | v<ver>   (or --base-url)
  → z42-<v>-<rid>-release/   (tooling)           fetch <base>/release-index.json
  → z42-runtime-<v>-<rid>/   (runtime pack)      workloads.<wl> = {archive, sha256, runtimes:[rid…]}
Archive (per rid):                                 ↓ download+sha+extract tooling
  platform primary rid → z42-<v>-<wl>.tar.gz       → runtimes/<ver>/workloads/<wl>/
  every platform rid  → z42-runtime-<v>-<rid>      ↓ for rid in runtimes:
publish:                                             runtimes.<rid>.runtime = {archive, sha256}
  SHA256SUMS + release-index.json                    download+sha+extract → runtimes/<rid>/<ver>/
   runtimes.<rid>.{sdk,runtime} (fixed)              bed (ios rewrite / wasm symlink / android jniLibs+assets)
   workloads.<wl>.{archive,sha256,runtimes}
```

`--from <dir>`（B2 已有）与无 `--from`（本次联网）共用同一 `_cmdWorkloadInstall` + 同一平台铺设分支；
唯一差别是 tooling/runtime 的字节来自本地目录 vs 下载解压。

## Decisions

### D1：manifest `workloads` 段 schema 定稿
**问题：** release-index.json 怎样表达「一个 workload tooling + 它依赖的 runtime RID 集」。
**决定：** 落地 runtime-workload-distribution.md 既有草案——顶层 `workloads` 与 `runtimes` 并列：
```json
"workloads": {
  "ios":     { "archive": "z42-<v>-ios.tar.gz",     "sha256": "…", "runtimes": ["ios-arm64","iossim-arm64"] },
  "android": { "archive": "z42-<v>-android.tar.gz", "sha256": "…", "runtimes": ["android-arm64","android-x64"] },
  "wasm":    { "archive": "z42-<v>-wasm.tar.gz",    "sha256": "…", "runtimes": ["browser-wasm"] }
}
```
`runtimes` 列表的每个 RID 在顶层 `runtimes.<rid>.runtime` 有对应 pack 条目（launcher 据此拉）。
**理由：** 与既有 `runtimes` 段同构、launcher `_fetchManifest` 解析风格可直接复用；`runtimes` 列表把
「一 workload 多 RID」（android 双 ABI、ios 真机+模拟器）显式编码，launcher 无需硬编码平台→RID 映射。
desktop 不在 `workloads`（它复用 host runtime，无 target runtime pack；Decision 9）。

### D2：CI tooling 归档去重到 platform primary RID
**问题：** android-arm64 与 android-x64 两个 matrix job 产**相同** tooling，`z42-<v>-android.tar.gz` 同名 →
`download-artifact merge-multiple` 冲突。
**决定：** workload tooling 归档只在 platform 的 **primary RID** 产：ios→`ios-arm64`、android→`android-arm64`、
wasm→`browser-wasm`。非 primary RID（iossim-arm64、android-x64）只产自己的 runtime pack 归档。
**理由：** tooling 跨同平台 RID 字节相同，产一次足够；primary 选 device/默认 ABI。`_wl_of(rid)` +
`_is_primary(rid)` 两个 bash helper 判定。

### D3：`--base-url` 覆盖 + URL 布局
**问题：** 缺省 GitHub release，但本地验证 / 自托管镜像需指向别处；且联网装需可本地端到端验证。
**决定：** `workload install <wl> [--base-url <url>]`。缺省
`https://github.com/z42-lang/z42/releases/download/<tag>`（对齐 `_cmdInstall`）。给 `--base-url` 则原样用作根，
launcher 取 `<base>/release-index.json` 与 `<base>/<archive>`（与 GitHub 资产同布局，故本地
`python3 -m http.server` 起 `<root>/<tag>/` 即可：`--base-url http://localhost:PORT/<tag>`）。
**理由：** 一个选项同时解锁镜像/离线/CI-less 验证；零额外协议。

### D4：联网装流程（tooling 先、runtimes 后、逐个铺设）
**问题：** 拉取与铺设顺序。
**决定：**
1. 下载+校验+解压 **tooling** → `runtimes/<ver>/workloads/<wl>/`（staging `.stage` + 原子 `File.Move`，镜像 `_cmdInstall`）。
2. **遍历 `workloads.<wl>.runtimes`**：每个 rid 读 `runtimes.<rid>.runtime` → 下载+校验+解压 → `runtimes/<rid>/<ver>/`。
3. 每个 rid 解压后**立即跑既有平台铺设**（D 复用 B2 的 ios/wasm/android 分支：改写 Package.swift / symlink / jniLibs+assets）。
**理由：** tooling 是铺设目标，必须先在；逐 rid 拉+铺与 B2 的「多 RID 增量」语义一致（android 双 ABI 都铺进同一
workload 的 jniLibs/<abi>）。任一 rid 缺失 manifest 条目 → 报错退出（不静默跳过）。

### D5：验证分半（launcher 本地 mock / CI correct-by-construction）
**问题：** CI 真 release 无法本地跑。
**决定：** launcher 联网装用**本地 mock manifest 服务器**端到端验证（wasm，最轻、包在手）；CI release.yml 改动
**correct-by-construction**，逐行对照既有 desktop 双归档 + index 生成核对，真验证落下次打 tag，unvalidatable
性质记 tasks.md。**理由：** 见 memory feedback_fix_validation_gap——不可本地验的部分追踪而非假装验过；launcher 半
完全可验，覆盖真正的新逻辑（下载-校验-解压-铺设）。

### D6：`_fetchWorkloadEntry` helper
**问题：** `_fetchManifest` 现读 `runtimes.<rid>.<pkgType>`，不读 `workloads`。
**决定：** 加 `string[] _fetchWorkloadEntry(string baseUrl, string wl)` → 返回 `[archive, sha256, rid0, rid1, …]`
（首二元素归档+sha，其后为 runtimes 列表；空数组=失败）。复用同一 `JsonValue.Parse` 风格。runtime pack 仍走
现有 `_fetchManifest(baseUrl, rid, "runtime")`，零改动。
**理由：** 最小新增面；workloads 解析与 runtimes 解析并存不互扰。

## Implementation Notes

- **launcher_network.z42**：新 `_fetchWorkloadEntry`；抽出一个 `byte[] _downloadVerified(string baseUrl, string asset, string sha)`
  小工具（下载+HTTP 检查+sha 校验，返回 body 或退出），供 `_cmdInstall` 与 workload 联网装共用（顺带去重）。
- **launcher_workload.z42**：`_cmdWorkloadInstall` 无 `--from` 且给定（或默认）联网参数时：调 `_fetchWorkloadEntry` →
  `_downloadVerified` tooling → 解压到 `wlDest`（staging+move）→ 循环 runtimes：`_fetchManifest(.., rid, "runtime")` →
  `_downloadVerified` → 解压到 `rtDest` → 调既有平台铺设（把 B2 里 inline 的铺设逻辑提取成 `_bedRuntimeIntoWorkload(wlDest, rtDest, rid, ver)` 复用）。
- **版本解析**：`--version` 给定用之；否则缺省策略同 `_cmdInstall`（`nightly`/`v<ver>`），但 workload 联网装通常显式 `--version`。
- **isWindows / 解压类型**：workload tooling 与平台 runtime 均 `.tar.gz`（平台无 windows）；desktop workload 不在本流程。
- **`--base-url` 透传**：`_cmdWorkloadInstall` 读 `--base-url`，为空则用默认 GitHub baseUrl。

## Testing Strategy

- **launcher 联网装 e2e（本地 mock，权威验证）**：
  1. `xtask package release --rid browser-wasm` → 产 tooling + runtime pack 目录。
  2. tar 成 `z42-<v>-wasm.tar.gz`（tooling）+ `z42-runtime-<v>-browser-wasm.tar.gz`（pack）。
  3. 手写 `release-index.json`（workloads.wasm + runtimes.browser-wasm.runtime）+ 算 sha256 填入。
  4. `python3 -m http.server` 起 `<root>/<tag>/`；`z42 workload install wasm --base-url http://localhost:PORT/<tag> --version <v>`。
  5. 断言：tooling 解压到 `runtimes/<ver>/workloads/wasm/`、runtime pack 到 `runtimes/browser-wasm/<ver>/`、
     symlink pkg-web/pkg-nodejs 建立、`node -e "import('<wlDest>/pkg-nodejs/z42_wasm.js')"` 加载成功。
  6. 反例：错 sha → checksum mismatch 退出；缺 workloads 段 → 报错。
- **CI correct-by-construction**：release.yml 改动逐行对照 desktop 双归档；jq workloads 段语法 `jq -n` 本地干跑校验
  JSON 合法。真 release 验证 = 下次 tag（tasks.md 记 unvalidatable）。

## Pipeline Steps（非 lang/ir/vm —— 仅 toolchain）

- [ ] CI YAML（release.yml Archive + index）
- [ ] launcher z42（network fetch + workload install 联网分支 + 铺设提取复用）
- [ ] 本地 mock e2e 验证
- [ ] docs 同步（manifest schema 定稿移出 Deferred）
