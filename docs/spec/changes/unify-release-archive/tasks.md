# Tasks: 发布归档收进 xtask —— release / nightly / 本地共用一条命令

**状态：🟡 进行中 | 开始：2026-10-01**

类型：`refactor`（CI + xtask；对外产物同名同格式，nightly 归档顺带修好可执行位）→ 最小化模式。
来自 CI 简化第二批的「release.yml / bench-pr.yml 迁 xtask 垫片」：用户要求「能简化就做，更统一，后面也能本地发布」。

## 问题

发布归档的命名与打包在两处各写一遍 bash，规则相同、实现不同：

- **release.yml** package job：按 RID 分支调 `package *`，再用 `_tar` / `_wl_of` / `_is_primary` 打归档；
  publish job 分三步（合并 desktop workload / `shasum` / `package index`）。全程手抄 z42vm / libs / xtask.zpkg 路径。
- **ci.yml** publish-nightly：下载各 job 上传的**原始包目录**，90 行 bash 从目录名反推 RID 再集中打包。
  - 缺陷：upload-artifact **不保留可执行位** ⇒ nightly tar 里的 `z42vm` / `z42` 等都不可执行。
- **bench-pr.yml** 7 处手抄 `"$vm" artifacts/xtask/xtask.zpkg -- …`。

本地没有对应命令：想本地出一份发布归档，只能照着 workflow 手敲。

## 方案

- `xtask package archive [--label L]`：`artifacts/packages/` 下的 release 包 → `artifacts/release/` 归档（命名规则唯一出处
  `xtask_release.z42`；windows-x64 出 zip，Windows 主机走 .NET ZipFile —— PS 5.1 的 Compress-Archive 写 `\` 分隔符）。
- `xtask package finalize <L>`：合并 4 份逐 RID desktop workload → SHA256SUMS → release-index.json。
- release.yml / ci.yml 各 package job 在**自己的 runner** 上 `archive`（可执行位保留），汇总 job 只 `finalize`。
  归档 PR 上也做（几秒），host-package 的 Installer smoke 改为安装这份**真归档** ⇒ 4 个桌面平台每个 PR 都端到端验它；
  上传只在非 PR（只有 publish-nightly 消费）。
- ci-bootstrap 的种子回退改读 `release-host-<rid>` 里的 `z42-sdk-nightly-<rid>` 归档（与首选路径同一个
  `unpack_and_check`）；旧形态 `z42-host-package-*` 目录保留为过渡期第二选择。
- bench-pr.yml 7 处改 `xtask` 垫片（垫片按自身位置定位 PR 树，`cd base-src` 不受影响）。
- 自检 `_testPackagesRelease`（`xtask test packages`）：9 个 RID 的假包目录 → archive → finalize，锁命名映射 /
  非主 RID 不出 workload / debug 包忽略 / 21→18 归档 / 4 个 apphost 合并 / SHA256SUMS / index。

## 进度概览

- [x] `scripts/package/xtask_release.z42`：`_releaseArchive` / `_releaseFinalize` / `_releaseWriteSums`
- [x] `scripts/cli/xtask_cli_package.z42`：`archive` / `finalize` 子命令
- [x] `scripts/package/xtask_selfcheck_release.z42` + 挂进 `xtask test packages`
      （首轮即抓到一个真 bug：`_exec` 强制 cwd = 仓库根 ⇒ 非 Windows 主机的 zip 把整个仓库打了进去 → 改 `_execIn`）
- [x] 本地真打：macOS host 的 sdk / runtime / desktop workload / test workload → `archive` → 4 个归档；解包后可执行位在、`z42 --version` 可跑
- [x] `.github/workflows/{release,ci,bench-pr}.yml`、`.github/actions/ci-bootstrap/action.yml`
- [x] 文档：`release.md`、`packaging.md`、`ci.md`（§3.1 回退链与人工恢复流程）、`scripts/README.md`
- [x] 本地 GREEN（基底 df7398709，10m36s；src 下的产物目录是种子太旧（9-30、早于 #977）在首次编 xtask / build all 时按旧布局写的，GREEN 本身零写入）
- [ ] PR CI（改了 ci.yml ⇒ 全跑；Installer smoke 在 4 个桌面平台装 `archive` 出的真归档）
- [ ] 合入后首个 main 运行：publish-nightly 用 `finalize` 发布成功、nightly 归档可执行位正确
