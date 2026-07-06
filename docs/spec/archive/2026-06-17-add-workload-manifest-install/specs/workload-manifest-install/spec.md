# Spec: workload-manifest-install

## ADDED Requirements

### Requirement: `z42 workload install <wl>` 无 `--from` 时走 manifest 联网装

#### Scenario: 联网装一个单-RID workload（wasm）
- **WHEN** 用户运行 `z42 workload install wasm --base-url <root>/<tag> --version <v>`，`<root>` 下有
  `release-index.json`（含 `workloads.wasm.{archive, sha256, runtimes:["browser-wasm"]}` +
  `runtimes.browser-wasm.runtime.{archive, sha256}`）及对应 tar.gz
- **THEN** launcher 下载 tooling 归档、校验 sha256、解压到 `runtimes/<v>/workloads/wasm/`；下载
  `browser-wasm` runtime pack、校验、解压到 `runtimes/browser-wasm/<v>/`；执行 wasm 铺设（symlink
  pkg-web/pkg-nodejs 进 workload 根）；打印 `✅ workload install wasm done`

#### Scenario: 联网装一个多-RID workload（android 双 ABI）
- **WHEN** manifest `workloads.android.runtimes = ["android-arm64","android-x64"]`，用户
  `z42 workload install android --base-url … --version <v>`
- **THEN** tooling 解压一次到 `runtimes/<v>/workloads/android/`；**两个** runtime pack 各下载+校验+解压到
  `runtimes/android-arm64/<v>/`、`runtimes/android-x64/<v>/`；每个 pack 的 `.so` 铺进
  `z42vm/src/main/jniLibs/{arm64-v8a,x86_64}/`、stdlib zpkgs 铺进 `assets/stdlib/`

#### Scenario: 缺省 base-url 用 GitHub release
- **WHEN** 用户 `z42 workload install ios --version <v>`（无 `--base-url`）
- **THEN** baseUrl = `https://github.com/z42-lang/z42/releases/download/v<v>`，其余流程同上

#### Scenario: checksum 不匹配
- **WHEN** 下载的归档 sha256 与 manifest 不符
- **THEN** 报 `checksum mismatch` 并以非零码退出，不解压、不留半装状态

#### Scenario: manifest 无该 workload
- **WHEN** `release-index.json` 无 `workloads.<wl>` 段
- **THEN** 报错（`workload '<wl>' not found in manifest`）并非零退出

#### Scenario: `--from` 本地装路径不受影响（回归）
- **WHEN** 用户 `z42 workload install ios --from <dir> --runtime <pack> --rid ios-arm64 --version <v>`
- **THEN** 行为与 B2 完全一致（本地拷贝 + 铺设），不触发任何网络请求

## MODIFIED Requirements

### Requirement: CI release 为平台 RID 上传 workload tooling + runtime pack 两个归档

**Before:** 平台 RID 只产一个归档 `z42-runtime-<v>-<rid>.tar.gz`，字节实为 workload tooling（B2-3 分包后错配）；
`runtimes.<rid>.runtime` 指向 tooling；无 `workloads` 段。

**After:**
- 平台 primary RID（ios-arm64/android-arm64/browser-wasm）额外产 `z42-<v>-<wl>.tar.gz`（来自 tooling 目录
  `z42-<v>-<rid>-release/`）。
- 每个平台 RID 产 `z42-runtime-<v>-<rid>.tar.gz`（来自真 runtime pack 目录 `z42-runtime-<v>-<rid>/`）。
- `release-index.json` 加顶层 `workloads.<wl>.{archive, sha256, runtimes:[…]}`；`runtimes.<rid>.runtime`
  现指向真 runtime pack 归档。

## Out of Scope（本 spec 不涉及）

- workload 命令发现（B1）、签名、真机多-slice xcframework 合并、host 平台 gate、workload update。

## Pipeline Steps（toolchain only）

- [ ] CI: Archive 双归档 + dedup primary RID
- [ ] CI: release-index.json 加 workloads 段 + 修平台 runtime 指向
- [ ] launcher: `_fetchWorkloadEntry` + 联网装分支 + 铺设复用
- [ ] launcher: `--base-url` 选项
- [ ] 本地 mock e2e（wasm）
