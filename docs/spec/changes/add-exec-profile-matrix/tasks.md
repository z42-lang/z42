# Tasks: 统一 test / bench 的执行画像矩阵

> 状态：🟡 进行中（阶段 0-2 已实施+提交）| 创建：2026-07-23 | 实施起：2026-07-31
> 占用子系统：`stdlib`（`Std.Platform` 能力函数）+ `runtime`（`__platform_caps` builtin）+ `toolchain`（scripts/）。`bench/` schema、`docs/` 不上锁。**独立分支 `add-exec-profile-matrix`（off origin/main），GREEN 以 CI 为权威。**

## 进度概览
- [x] 阶段 0: 标准库运行时能力查询（caps 地面真值）—— commit cc567052，探针端到端验证
- [x] 阶段 1: 共享 SoT 模块 + schema v2 —— commit 64aeeffa
- [x] 阶段 2: bench e2e 模式扫描 + 打标 —— commit 1d30ce28 + micro v2 收尾 d6734c9a
- [x] 阶段 3.1: 多线程场景 06_thread_scaling —— commit 17b9b2fb（编译+运行验证：输出 23999980）
- [~] 阶段 3.2: 平台 bench 接入 —— **Deferred**（exec-profile-matrix-future-platform-bench）：
      冷环境不可验 + 大面 + informational 非门禁；profile 机制已平台就绪，缺平台 harness 编排
- [x] 阶段 4: CI 腿（bench-pr/update 跑 `--mode both`）+ 文档（README 去死引用 + 设计doc）—— commit 7337bbff+
- [ ] 阶段 5: 验证与归档（GREEN 以 CI 为权威）

> **本地验证限制（冷环境）**：本 worktree 无匹配 origin/main 期的 z42c 种子（sibling 种子过旧、
> 无法编 origin/main 的 z42.scripting）→ 完整 `xtask bench` 端到端跑不通；已验证：Rust 单测 8/8、
> 探针端到端（caps 正确）、xtask.zpkg 全量编译（阶段0-2 改动）通过、bench 入口可达。完整 GREEN 以 CI 为准。

## 阶段 0: 标准库运行时能力查询（caps 地面真值）
- [ ] 0.1 `src/runtime/src/corelib/platform.rs`：加 `__platform_caps`（返回 `string[]`）+
      `__platform_exec_modes` builtin，`cfg!(feature=…)` 收集 jit/native-interop/bundled-compression
      + `cfg!(not(target_arch="wasm32"))` 加 threads
- [ ] 0.2 `src/runtime/src/corelib/mod.rs`：注册新 builtin
- [ ] 0.3 `src/libraries/z42.core/src/Platform.z42`：加 `Capabilities()`/`ExecModes()` extern +
      `HasJit()`/`HasThreads()`/`HasNativeInterop()`/`HasAot()` 谓词（沿用 `IsLinux()` 风格）
- [ ] 0.4 `bench/probe/capabilities.z42`：调 stdlib 函数打印一行 JSON（caps+exec_modes+os+arch）
- [ ] 0.5 单测：Rust `platform_tests.rs`（desktop caps 含 jit+threads）+ stdlib `[Test]`
      （`HasThreads()` 与 `Capabilities()` 一致）

## 阶段 1: 共享 SoT 模块 + schema v2
- [ ] 1.1 新建 `scripts/common/xtask_exec_profile.z42`：`ExecProfile{mode:{tiers,aot_pkgs},
      platform,caps}` 类型 + `mode_label` 规范化 + `SUPPORT_MATRIX`（策略覆盖层）+ 跑
      `capabilities.z42` 探针取 caps + `cellStatus(profile, vmCaps)` + `enumerateCells()`
      （稳定排序，common-pitfalls §1）
- [ ] 1.2 `cellStatus` 单测：threads 由 vmCaps 决定（wasm=never/desktop=runnable）/
      `aot_pkgs≠[]`@任意=skipped（整列占位）/ `{[interp,jit],[]}`@desktop=runnable /
      `{[interp,jit],[]}`@wasm=never / `mode_label` 规范化（jit+aot[z42.core]）/ 枚举顺序稳定
- [ ] 1.3 `bench/baseline-schema.json` → v2：加 `profile{mode:{tiers,aot_pkgs},mode_label,
      platform{os,arch},caps}` + `z42vm_version`；删 `csharp-throughput` tier + `dotnet_version`；
      收紧 additionalProperties
- [ ] 1.4 schema 校验用例：v2 含 profile 通过；csharp tier 被拒；v1 无 profile 被拒

## 阶段 2: bench e2e 模式扫描 + 打标
- [ ] 2.1 `scripts/xtask_cli.z42`：`bench`(e2e) parser 加 `--mode interp|jit|both` / `--caps` /
      e2e `--json`；更新帮助文案
- [ ] 2.2 `scripts/xtask_bench.z42`：被测 hyperfine 命令加 `--mode <m>`（改 line 61 无 mode 现状）；
      `both` → 两组结果
- [ ] 2.3 `xtask_bench.z42`：结果生成器写 `profile`（mode=`{tiers,aot_pkgs:[]}` + mode_label +
      platform{os,arch} + caps，后二者取自阶段 0 的探针查询）；`_osTag` 升级为结构化 platform
- [ ] 2.4 `xtask_bench.z42` `--diff`：匹配键升级为 `(name, metric, mode_label, platform)`；跨 profile 不比
- [ ] 2.5 派生「jit/interp 加速比」在 diff 报告展示
- [ ] 2.6 请求 never/skipped 格子 → 显式跳过 + log（不静默）
- [ ] 2.7 `scripts/test/xtask_test_lib.z42` / `MicroBenchAgg`：micro 结果项补 `profile`（已用 --mode）

## 阶段 3: 线程场景 + 平台 bench 接入
- [ ] 3.1 新建 `bench/scenarios/06_thread_scaling.z42`：`Std.Threading` spawn/join 可扩展性，
      确定聚合输出（Assert 自验证）
- [ ] 3.2 `scripts/test/xtask_test_platform.z42`：run 段增 bench 采集分支（或 `bench` 动作），
      wasm/ios/android 产 informational 基准，profile.platform 打标
- [ ] 3.3 平台 caps 取自各平台 VM 下探针的 `Std.Platform.Capabilities()`（wasm 报无 jit/threads；
      mobile 报无 jit、有 threads/native）——即验证阶段 0 的能力查询在 wasm/mobile 构建下也正确

## 阶段 4: CI 腿 + 文档 + 死引用清理
- [ ] 4.1 `.github/workflows/bench-pr.yml`：e2e PR 腿跑 `--mode both`（informational，不门禁）
- [ ] 4.2 `.github/workflows/bench-update.yml`：main baseline 按 profile 键持久化
- [ ] 4.3 新建 `docs/design/testing/exec-profile-matrix.md`：三元组 / 支持矩阵 / skip 语义 /
      schema v2 / 加速比（知识上浮）；挂入 SUMMARY（若 book）
- [ ] 4.4 `bench/README.md`：删 justfile / C# / 失效命令；写模式×平台×能力矩阵 + 运行方式
- [ ] 4.5 死引用 grep 清零：`grep -rn "csharp-throughput\|dotnet_version\|just bench" docs/ bench/ scripts/ .claude/`
- [ ] 4.6 Rust criterion tier（`src/runtime/benches/`）现状如实标注（未接入 xtask）或接入决策落文档

## 阶段 5: 验证与归档
- [ ] 5.1 `cargo build --release`（z42vm）无错
- [ ] 5.2 `xtask test`（完整 GREEN gate，含 `test compiler` 自举不动点——确认 bench 脚本改动不破坏 xtask.zpkg 自建）
- [ ] 5.3 `xtask bench --quick --mode both` 冒烟：产 interp+jit 双 profile 项、diff 同 profile 对比、加速比
- [ ] 5.4 spec scenarios 逐条覆盖确认
- [ ] 5.5 文档同步（阶段 9 触发矩阵）：目录 README / book / workflow testing 页
- [ ] 5.6 归档 + 释放 toolchain 锁（ACTIVE.md）

## 调度约束（跨子系统锁）
- 占 `stdlib` + `runtime` + `toolchain` 三锁。**独立分支推进**（User 授权，off origin/main，
  worktree `z42-benchmatrix`）：现状（2026-07-30 main）`runtime`/`toolchain` 空闲、`stdlib` 被
  `converge-z42c-onto-z42-project` 占 → 走**物理隔离分支**（stabilize-dispatch-keys 先例），
  与主线并行，合并解冲突（stdlib 侧仅动 `Platform.z42`，不与 z42c 收敛重叠）。ACTIVE.md 已登记。

## 备注
- caps 是**标准库运行时函数**返回的属性（User 定调）：`Std.Platform.Capabilities()`，探针在被测
  VM 下调用取真实 caps，不静态推断；不走 CLI。
- **mode = 执行组合 `{tiers, aot_pkgs}`**（User 定调）：统一 partial-AOT（aot.md D2），interp/jit/
  部分AOT/全AOT/hybrid 同一形状。本 change 只建模，`aot_pkgs` 恒 `[]`；AOT 执行 + 配置面归 M9。
- runtime 侧仅加只读能力 builtin（`__platform_caps`，沿用 `__platform_os` 模式），不碰执行语义。
- AOT 组合仅矩阵占位 skipped（Deferred：exec-profile-matrix-future-aot-composition-cells）。
- 平台 bench informational 不门禁（Deferred：exec-profile-matrix-future-platform-bench-gating）。
- schema v2 是否弃读 v1、platform 结构化、caps 全量 tag —— 见 proposal Open Questions，实施前需 User 裁决。
