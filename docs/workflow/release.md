# Release 工作流

> **状态**：✅ 自动化已落地（[archive/2026-05-14-add-release-automation](../spec/archive/2026-05-14-add-release-automation/)）。本地 per-arch SDK package 同步可用（[packaging.md](packaging.md)）。

## 本地打 SDK package

```bash
./xtask package release                       # host RID
./xtask package release --rid ios-arm64       # 任一 9 RID 之一
./xtask package --help                        # RID 矩阵 + 选项
```

9 RID 矩阵 + 平台前置 + 验证 + 失败排查见 [`packaging.md`](packaging.md)。

## 发 release（tag-triggered 自动化）

### Step-by-step

```bash
# 1. 改 versions.toml [project].version（单一 SoT）
$EDITOR versions.toml                             # 例：0.1.0 → 0.2.0

# 2. drift-check 提醒同步 Cargo.toml
./xtask deps check                 # 应 fail（versions.toml 已改但 Cargo.toml 未改）

# 3. 同步 src/runtime/Cargo.toml [workspace.package].version
$EDITOR src/runtime/Cargo.toml                    # 同样改成 0.2.0

# 4. 验证（drift-check 通过 + workspace 解析正确）
./xtask deps check                 # 应通过
cargo metadata --manifest-path src/runtime/Cargo.toml --format-version 1 --no-deps \
    | jq -r '.packages[].version' | sort -u       # 应全部为 0.2.0

# 5. commit + push
git add versions.toml src/runtime/Cargo.toml
git commit -m "chore(release): bump version 0.1.0 → 0.2.0"
git push origin main

# 6. tag + push tag → 触发 .github/workflows/release.yml
git tag v0.2.0
git push origin v0.2.0
```

### CI 流程

`.github/workflows/release.yml` 在 tag push 后跑 3 阶段：

1. **verify** — 校验 `tag.strip_prefix('v') == versions.toml [project].version`；drift fail-fast
2. **package** (matrix × 9 RID) — 每个 RID 一台 runner，跑 `xtask build stdlib` + `xtask package release --rid <rid>` + release.yml 的内联 tar/shasum 归档步骤
3. **publish** — 汇总 9 个 archive，生成 `SHA256SUMS`，调 `gh release create v<version>` 上传

### Artifact 命名

| RID | 文件名 |
|-----|--------|
| linux-x64 / linux-arm64 / macos-arm64 | `z42-<v>-<rid>.tar.gz` |
| windows-x64 | `z42-<v>-windows-x64.zip` |
| ios-arm64 / iossim-arm64 | `z42-<v>-<rid>.tar.gz` |
| android-arm64 / android-x64 | `z42-<v>-<rid>.tar.gz` |
| browser-wasm | `z42-<v>-browser-wasm.tar.gz` |
| 校验和 | `SHA256SUMS`（coreutils 格式）|

### Pre-release 自动标记

`--prerelease` 自动设置的条件：
- 版本号 < `1.0.0`（pre-1.0 阶段全部）
- Tag 含 `-` 后缀（`v0.2.5-rc1` / `v1.0.0-rc.1` 等）

GitHub UI 不把 prerelease 显示在 "Latest release" 高亮位。

### 手动 dry-run（不实际打 tag）

GitHub Actions UI → "Release" workflow → "Run workflow" → 输入 version（必须与 versions.toml 一致）。同样跑完整 3 阶段，最终 publish 也会真正创建 release —— 因此 dry-run 实际上等于一次正式 release（pre-1.0 节奏下不必清理）。

### 失败排查

| 症状 | 原因 |
|------|------|
| `verify` job：`drift: tag=X versions.toml=Y` | tag 与 versions.toml 不一致；改正后重新打 tag |
| 某个 `package-<rid>` job fail | 看 ci.yml 对应 `package-<rid>` job 是否也 fail；通常是 toolchain 环境 / 网络 / cache |
| `publish` job：`gh release create` 报 already exists | tag 重复；删 release + 重 push tag（注意：pre-1.0 一般不删，直接 bump 到下一版本）|

## Nightly rolling pre-release

每次 push 到 `main`，`.github/workflows/ci.yml` 的 `publish-nightly` job 会汇总所有 9 个 RID 的 package artifact，强制覆盖一个名为 `nightly` 的 GitHub Release。

| 属性 | 值 |
|------|-----|
| 触发 | `push` 到 `main` 且全部 build / test / package job 通过 |
| Tag | `nightly`（每次 delete + recreate，URL 永远稳定）|
| 标记 | `--prerelease`（不进 "Latest release"）|
| 签名 | **不签名**（与 tag-triggered release 区分）|
| 内容 | 9 个 archive（tar.gz / zip）+ `SHA256SUMS` |
| 文件名 | `z42-nightly-<rid>.{tar.gz|zip}`（无版本号；URL 稳定）|

下载示例：

```bash
# 一次性拿最新 Linux x64
curl -LO https://github.com/<owner>/z42/releases/download/nightly/z42-nightly-linux-x64.tar.gz
curl -LO https://github.com/<owner>/z42/releases/download/nightly/SHA256SUMS
shasum -a 256 -c SHA256SUMS --ignore-missing
```

> **使用边界**：nightly 是 main 的最新 snapshot，不保证稳定。生产 / 集成场景请使用 tag-triggered release（`v<version>`）。

## 校验下载的 release

```bash
# 在解压前
curl -LO https://github.com/<owner>/z42/releases/download/v0.2.0/SHA256SUMS
curl -LO https://github.com/<owner>/z42/releases/download/v0.2.0/z42-0.2.0-macos-arm64.tar.gz
sha256sum -c SHA256SUMS --ignore-missing      # 或 shasum -a 256 -c (BSD/macOS)

# 解压
tar -xzf z42-0.2.0-macos-arm64.tar.gz
cd z42-0.2.0-macos-arm64-release/
./bin/z42c --version
```

## 1.0 之后

`z42up` 跨平台安装器（rustup 等价物）启用，用户走 `z42up install stable` 而非手工下载 tarball。详见 [`docs/roadmap.md`](../roadmap.md) §1.0.x charter。

## 发布打包：release 子命令（脚本归零，2026-06-28）

release-time 打包胶水已搬进 `xtask release`（源 `scripts/xtask_release.z42`），原 `scripts/release/*.sh` 已删：

| 命令 | 功能 |
|------|------|
| `xtask release assemble-desktop-workload <LABEL> [dist]` | 合并 4 个 per-RID desktop workload 产物为单一 RID-agnostic archive + manifest |
| `xtask release gen-release-index <LABEL> [dist] [channel] [tag] [version]` | 从 `SHA256SUMS` 生成 `release-index.json`（launcher 供给契约；JSON 经 z42.json 构建）|

tar/unzip/date 作外部子进程；逻辑在 z42。两条命令在 `release.yml`（tagged）+ `ci.yml` publish-nightly（rolling）调用——这两个 job 现各自 provision z42vm + xtask.zpkg（publish-nightly 经 `xtask-bootstrap-artifact` action 消费 toolchain artifact；release.yml publish 经 `ci-bootstrap` action 自举），再 `xtask release …`。
