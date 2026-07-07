# Spec: deps 命令面收敛（deps-cli）

> 变更类型：toolchain（CLI 对外行为变更）。非 lang/ir/vm——无 IR Mapping / Pipeline
> Steps；编译器与 VM 零改动。

## MODIFIED Requirements

### Requirement: `deps check` = 唯一只读校验

**Before:** `deps check` 只查文件 drift；工具存在性要 `deps install --check`；drift
另有 `deps install --drift` 重复实现。
**After:** `deps check [--os <p>]` 一次跑完：跨平台工具告警（rust/node）→ 各平台
presence（`_setup*` check 模式）→ versions.toml↔投影 drift（单实现）。

#### Scenario: 全量检查（裸跑 = drift 门禁，presence 信息性）
- **WHEN** 运行 `deps check`（无 `--os`）
- **THEN** 依次输出跨平台 / android / ios / wasm 各段 presence + drift 结果；
  **drift 任一 ✗ → exit 1**；presence ✗ 仅信息性展示（附 note 提示），不影响退出码
  ——CI 在无平台 SDK 的 runner 上裸跑本命令当 drift 门禁，presence 缺失是预期

#### Scenario: 平台严格检查
- **WHEN** 运行 `deps check --os <p>` 且该平台 presence 任一 ✗
- **THEN** exit 1（显式 `--os` = 调用者声明开发平台 p，presence 计入退出码）

#### Scenario: 平台过滤
- **WHEN** 运行 `deps check --os wasm`
- **THEN** 只跑跨平台段 + wasm 段（wasm-pack、node、drift 无项提示）

#### Scenario: wasm 真校验（修复恒真假检查）
- **WHEN** wasm-pack 未安装，或版本 < versions.toml `build.wasm.wasm_pack_min`
- **THEN** `deps check` 该项 ✗ 且 exit 1（Before：`_check(wpm, wpm)` 恒 ✓）

### Requirement: `deps install` = 纯安装，平台必备直装

**Before:** install 身兼 install/--check/--drift/--print-env 四职 + step positional
（node / android-emulator）。
**After:** `deps install [--os <p>] [--force]` 只装平台必备；node 属 wasm 必备。

#### Scenario: wasm 必备含 node
- **WHEN** 运行 `deps install --os wasm`
- **THEN** rust targets + wasm-pack + hermetic node（`artifacts/tools/node`，幂等）全部就位

#### Scenario: android 必备不含 emulator
- **WHEN** 运行 `deps install --os android`
- **THEN** 装 build tier（rust targets、cargo-ndk、JDK 检查、SDK build 包）；**不**装
  emulator/AVD/Gradle（~4GB 留给用到时自动装）

#### Scenario: 旧命令面已删
- **WHEN** 运行 `deps install --drift` / `--check` / `--print-env` / `deps install node` /
  `deps install android-emulator`
- **THEN** Std.Cli 报 unknown option / unexpected positional，exit 非 0

## ADDED Requirements

### Requirement: 用到才装（自动，无命令面）

#### Scenario: android 测试自动装 emulator
- **WHEN** `test platform android run` 且 emulator/AVD 未安装
- **THEN** 打印「installing android emulator tier (~4GB, one-time)」→ 自动安装 →
  继续跑测试；安装失败则测试失败并透出错误（不吞不跳过）

#### Scenario: wasm 测试自动装 node
- **WHEN** wasm RunTests 且 hermetic node 与 PATH node 均缺
- **THEN** 自动 `_depsInstallNode` 后继续；已有任一 node 则不动

### Requirement: `deps env`

#### Scenario: NDK 环境导出
- **WHEN** 运行 `deps env`（或 `deps env --os android`）且 NDK 已装
- **THEN** stdout 仅含 `export ANDROID_NDK_HOME="…"`（可 `eval`）；未装 → stderr 提示 + exit 1

## REMOVED Requirements

- `deps install --drift`（与 `deps check` 重复的第二套 drift 实现，连同
  `_checkAndroidDrift`/`_checkIosDrift`/`_firstIntAfter` 删除）
- `deps install --check` / `--print-env` mode flag、`node`/`android-emulator` step
  positional（语义分别并入 `deps check` / `deps env` / 自动安装）

### Requirement: CI 兼容

#### Scenario: CI 不依赖被删命令
- **WHEN** 全 CI workflows 跑（android 用 setup-ndk + android-emulator-runner，node 用
  actions/setup-node）
- **THEN** 无任何 job 调用被删的命令面，全部照常

#### Scenario: CI 裸跑 `deps check` 语义保持（实施期修正 2026-07-07）
- **WHEN** build-and-test 在无 android/ios SDK 的 runner 上跑 `deps check`（ci.yml:131-138）
- **THEN** presence 缺失仅信息性展示，drift 全 ✓ 时 exit 0——CI 零 workflow 改动
