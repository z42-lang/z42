# Proposal: 收敛 xtask deps 命令面（依赖分层：平台必备直装 + 用到自动装）

## Why

`xtask deps` 命令面冗余且心智负担重（2026-07-07 分析）：

1. **同一套 drift 检查两套实现**：`deps check`（`xtask_deps.z42:15` regex 解析）与
   `deps install --drift`（`xtask_install.z42:318-378` 手写扫描）检查完全相同的东西
   （gradle minSdk/compileSdk、Package.swift .iOS/.macOS ↔ versions.toml），连辅助
   函数都各一份（`_firstMatchGroup` vs `_firstIntAfter`）——防漂移的检查自己在漂移。
2. **假检查**：`deps check` 的 wasm 段 `_check("wasm_pack_min present", wpm, wpm)`
   拿同一值和自己比，恒真，什么都没校验（`xtask_deps.z42:68-71`）。
3. **命名撞车**：`deps check`（查文件漂移）vs `deps install --check`（查工具存在性）
   名字近义、语义不同；`--drift` 又与 `check` 同义。
4. **install 一身四职**：默认安装 / `--check` / `--drift` / `--print-env` 四个 mode
   flag 复用一个动词；`--print-env` 是查询不是安装。
5. **依赖分层错位**：node 是 wasm 平台必备（wasm js/Playwright 测试跑不了），却被归
   为用户手动触发的 TIER 2 step（`deps install node`）；android emulator（~4GB）是
   "跑 instrumentation 测试才需要"的重型依赖，也要用户手动 `deps install android-emulator`。
   `--os android` 与 `android-emulator` step 实现层已共用 `_depsInstallAndroidSdk(force, tier)`，
   重复的只是命令面。

## What Changes

**依赖模型（User 裁决 2026-07-07）：平台必备依赖随 `deps install --os <p>` 直接安装；
重型/可选依赖在用到的步骤自动安装，不需要用户手动触发。**

新命令面（3 个正交子命令，删全部 mode flag 与 step positional）：

| 命令 | 语义 |
|------|------|
| `deps check [--os <p>]` | **唯一只读校验**：工具存在性（原 `install --check`）+ versions.toml↔投影 drift（原 `check` + `--drift` 合一，单实现）；修复 wasm 假检查 |
| `deps install [--os <p>] [--force]` | **纯安装**平台必备依赖；**node 划入 wasm 必备**（`--os wasm` 直接装 hermetic node） |
| `deps env [--os android]` | 环境导出（原 `install --print-env`） |

用到才装（自动，无命令面）：
- **android emulator**：`test platform android run` 检测 emulator/AVD 缺失 → 自动调
  `_depsInstallAndroidEmulator`（打印清晰日志，~4GB 一次性）
- **node 兜底**：wasm 测试若 hermetic node 与 PATH node 均缺 → 自动 `_depsInstallNode`

删除：`install` 的 `--check` / `--drift` / `--print-env` flag、`node` / `android-emulator`
step positional；`xtask_install.z42` 的 `_checkAndroidDrift` / `_checkIosDrift` /
`_firstIntAfter`（重复实现，~60 行）。

CI 兼容（实施期事实修正 2026-07-07）：CI 不使用**被删的**命令面（安装类走
setup-ndk / android-emulator-runner / actions/setup-node action），但 build-and-test
会在无平台 SDK 的 runner 上**裸跑 `deps check`** 当 drift 门禁（ci.yml:131-138）。
因此 check 的退出码策略细化为：**drift 失败恒致败（机器无关）；presence 失败仅显式
`--os <p>` 时致败**，裸跑时 presence 仅信息性展示——CI 语义不变、零 workflow 改动。

> 后续衔接：`add-vscode-syntax-ext`（排队中）将在收敛后的 `deps install` 上新增
> optional positional component `vscode`（`deps install vscode` = 编辑器资产安装，
> 用户显式触发——编辑器集成无法"用到自动装"）。本变更不预留死代码，槽位由该变更引入。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `scripts/xtask_cli.z42` | MODIFY | deps router：`check` 加 `--os`；`install` 删 3 个 mode flag + step positional；新增 `env` leaf；`_dispatchDeps` 同步 |
| `scripts/xtask_deps.z42` | MODIFY | check 收敛为单实现：presence（复用 `_setup*` 的 check 模式）+ drift（保留 regex 版）+ `--os` 过滤；wasm 假检查改真校验（wasm-pack/wasm-tools 存在 + ≥ min 版本） |
| `scripts/install/xtask_install.z42` | MODIFY | 删 `_checkAndroidDrift`/`_checkIosDrift`/`_firstIntAfter` 与 mode 多路复用；`_setupWasm` 增 node 必备安装；`_depsEnv`（原 print-env 逻辑迁入）；`_depsInstallNode` 转内部函数 |
| `scripts/install/xtask_install_android.z42` | MODIFY | 两层 tier 注释同步（emulator tier = 内部 lazy 入口，不再是用户命令） |
| `scripts/test/xtask_test_android.z42` | MODIFY | RunTests 前置探测：emulator/AVD 缺失 → 自动 `_depsInstallAndroidEmulator` |
| `scripts/test/xtask_test_wasm.z42` | MODIFY | RunTests node 探测：hermetic + PATH 均缺 → 自动 `_depsInstallNode` |
| `docs/workflow/building/android.md` | MODIFY | 命令面同步（删 android-emulator step 说法，改自动安装说明） |
| `docs/workflow/building/wasm.md` | MODIFY | 命令面同步（node = wasm 必备） |
| `docs/workflow/building/windows.md` | MODIFY | 命令面同步（node MSI 指引处的命令引用） |
| `docs/workflow/testing/platform-tests.md` | MODIFY | 命令面同步 |
| `docs/workflow/packaging.md` | MODIFY | 命令面同步（如引用 deps install） |
| `docs/book/src/dev/xtask.md` | MODIFY | deps 命令面章节刷新 |
| `scripts/xtask.z42` | MODIFY | `_depsCheck` 包装签名改 ParseResult（实施期 Scope 增补） |
| `scripts/README.md` | MODIFY | deps 命令图 + 典型流程刷新（实施期 Scope 增补：触发矩阵 README 同步） |
| `src/toolchain/workload/wasm/README.md` | MODIFY | 删 `deps install node` 旧用法（实施期 Scope 增补：同上） |

**只读引用**（不修改）：

- `versions.toml` — SoT 字段面
- `.github/workflows/ci.yml` / `.github/actions/ci-bootstrap` — 确认 CI 不依赖被删命令
- `scripts/common/xtask_versions.z42` — `_vget`/`_scalarStr` 共享设施

## Out of Scope

- `deps install vscode` component 槽（归 `add-vscode-syntax-ext`）
- AndroidBackend.RunTests 的 test.sh 桥接 z42 化（roadmap `port-android-emulator-run-to-z42`）
- versions.toml 字段结构调整
- Windows 平台的 node/SDK 自动安装（现状 POSIX-only，维持）

## Open Questions

- [x] `deps env` 独立子命令 —— **已裁决**（User 2026-07-07 确认整体方案）：独立保留
  （`eval "$(...)"` 用法需要纯净 stdout）
- [x] emulator 命令面 —— **已裁决**（User 2026-07-07）：零命令面、彻底隐藏；
  `_depsInstallAndroidEmulator` 纯内部函数，仅 `deps check` 状态行与自动安装日志可见
