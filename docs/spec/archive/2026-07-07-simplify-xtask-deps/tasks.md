# Tasks: 收敛 xtask deps 命令面

> 状态：🟢 已完成 | 创建：2026-07-07 | 完成：2026-07-07
> （spec User 确认 + 实施 + GREEN 全 stage 同日；presence 退出码策略实施期修正见备注）
> 占用子系统：`toolchain`（开工时登记 ACTIVE.md）

## 进度概览
- [x] 阶段 1: check 收敛（单实现 + 真校验）
- [x] 阶段 2: install 纯化 + 依赖两层落位
- [x] 阶段 3: env 子命令 + router 收口
- [x] 阶段 4: 验证（smoke 全过 + 裸 `xtask test` 全绿）
- [x] 阶段 5: 文档同步

## 阶段 1: check 收敛
- [x] 1.1 `xtask_deps.z42`：`_checkVersionsDrift` 重组为 `deps check` 主流程（`_depsCheckRun`）——跨平台告警 + 按 `--os` 分段（presence: `_setup*(mode="check")` + drift: regex 版）
- [x] 1.2 wasm 假检查改真校验：`_checkWasmToolMins`（wasm-pack/wasm-tools 在场时版本 ≥ min）
- [x] 1.3 删 `xtask_install.z42` 的 `_checkAndroidDrift` / `_checkIosDrift` / `_firstIntAfter`

## 阶段 2: install 纯化 + 两层落位
- [x] 2.1 `_depsInstall` 删 check/drift/print-env mode 与 step positional 分发；`_setup*` mode 收敛为 install|check 两值
- [x] 2.2 `_setupWasm` 追加 node 必备（`_depsInstallNode`，幂等）+ `_checkWasmNode`（check 模式）
- [x] 2.3 `xtask_test_android.z42` RunTests 前置：emulator 缺 → 日志 + 自动 `_depsInstallAndroidEmulator`；外部 ANDROID_HOME 缺 emulator → 明确报错；失败即测试失败
- [x] 2.4 `xtask_test_wasm.z42` RunTests：hermetic + PATH node 均缺 → 自动 `_depsInstallNode`
- [x] 2.5 `xtask_install_android.z42` tier 注释同步（emulator = 内部 lazy 入口）

## 阶段 3: env 子命令 + router 收口
- [x] 3.1 `_depsEnv`（print-env 逻辑迁入；stdout 纯净可 eval）
- [x] 3.2 `xtask_cli.z42`：deps router 改三 leaf（check --os / install --os --force / env --os）+ `_dispatchDeps` 同步 + `xtask.z42` `_depsCheck(ParseResult)` 包装

## 阶段 4: 验证
- [x] 4.1 smoke：裸 `deps check` exit 0（drift 全 ✓、presence 信息性）；`deps check --os wasm` 缺 node 时 exit 1、装后 exit 0；`deps install --os wasm` node 落 hermetic（v22.11.0）；`deps env` stdout 纯净（NDK 未装 → stderr 提示 + exit 1，`--os ios` → exit 0）
- [x] 4.2 负路径：`--drift`/`--print-env` → unknown option exit 2；`node` step → unexpected positional exit 2；`--os foo` → exit 2（wasm min 临时改动的 drift 验证由真校验逻辑覆盖，✓ 行已带版本比对证据）
- [x] 4.3 lazy 钩子：wasm node 钩子与安装器同路径已由 4.1 实证（安装前 strict ✗ → 安装 → ✓）；emulator 钩子代码审查 + 检测逻辑与 `deps check` 状态行同源（~4GB 真实安装不在本机执行，留待首次 android 测试）
- [x] 4.4 裸 `xtask test` 全绿（e2e goldens + stdlib [Test] + z42c 自举不动点 7/7 byte-identical，exit 0）
- [x] 4.5 spec scenarios 逐条覆盖确认（含实施期修正后的退出码策略场景）

## 阶段 5: 文档同步（阶段 9 触发矩阵：对外 CLI 行为变更）
- [x] 5.1 `docs/workflow/building/{android,wasm,windows}.md` + `docs/workflow/testing/platform-tests.md` 命令面刷新（`packaging.md` 现有引用均为存活形式，无需改）
- [x] 5.2 `docs/book/src/dev/xtask.md` deps 章节（两层依赖模型 + 三子命令 + 退出码策略；对齐日期刷新）
- [x] 5.3 `scripts/README.md` deps 命令图 + 典型流程、`src/toolchain/workload/wasm/README.md` 旧用法清理（Scope 增补）
- [x] 5.4 归档前 doc-check 清单核对（触发矩阵命中行全落实；book 页头对齐日期已刷；改动文档相对链接可解析）

## 备注
- **实施期事实修正（2026-07-07）**：起草时称"CI 不用这些命令"，实施时发现 build-and-test
  裸跑 `deps check` 当 drift 门禁（ci.yml:131-138）→ 细化退出码策略（drift 恒致败 /
  presence 仅 `--os` 时致败），CI 零 workflow 改动。已同步 proposal/design（Decision 2a）/spec。
- **Scope 增补（2026-07-07）**：`scripts/xtask.z42`（check 包装签名）、`scripts/README.md`、
  `src/toolchain/workload/wasm/README.md`（触发矩阵强制的 README 同步，起草时遗漏）。
- 旧 flag 拒绝的退出码是 2（Std.Cli parse error 约定），非 1——spec 写"非 0"，满足。
