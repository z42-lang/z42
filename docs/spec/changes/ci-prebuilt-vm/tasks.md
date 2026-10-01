# Tasks: 不调 cargo 的 linux-x64 job 直接用 compile-toolchain 编好的 z42vm

**状态：🟡 进行中 | 开始：2026-10-01**

类型：`ci`（编排提速）→ 最小化模式。CI 简化第二批的「toolchain artifact 带 z42vm」。

## 问题

消费 `toolchain-ubuntu-latest` 的 job 每个都 `cargo build --release` 一遍 z42vm。rust-cache 只缓存依赖，
key 精确命中时 z42 crate 本身仍要 fat-LTO 重链：main run 36779111095 的 compiler-checks 里这一步 97 s
（`Restored from cache key … full match: true` → 1 分 37 秒后才进下一步），各消费 job 的 bootstrap 步骤 101–141 s。

## 方案

- `compile-toolchain` 另传 `z42vm-linux-x64`（`artifacts/build/runtime/release/{z42vm,*.so}`）——与消费方
  自己 `cargo build --release` 是同一条命令的产物。
- `xtask-bootstrap-artifact` 加 `prebuilt-vm` 输入：为 true 且宿主 Linux X64 时跳过 Setup Rust / rust-cache /
  cargo build，改为下载该 artifact 并恢复可执行位（`z42vm --version` 自检）；其它宿主不受影响。
- 只对**之后不再调 cargo** 的 job 开启（逐条核对过命令链）：
  - `test-stdlib-jit` ×2、`test-stdlib-interp`（linux-x64 腿）：`test stdlib --no-build`，`_buildRuntime` 只在
    `!noBuild` 分支；z42b 由 z42c 编。
  - `publish-nightly`：只跑 xtask。
  - 不开：`compiler-checks`（`test compiler` 无 `--no-build`，内部 `_buildRuntime`）、`test-vm-jit`（`test e2e`
    先 cargo 编 debug VM）、`compile-test-assets`、各打包 / 平台测试 job（都要 cargo）。

## 进度概览

- [x] `.github/actions/xtask-bootstrap-artifact/action.yml`：`prebuilt-vm`
- [x] `.github/workflows/ci.yml`：上传 `z42vm-linux-x64`；三个 job 开启
- [x] 文档：`ci.md`
- [x] 本地 GREEN（基底 18a2ccee5，10m09s）
- [ ] PR CI：开启的 job 日志里没有 `Compiling`、bootstrap 步骤明显变短
