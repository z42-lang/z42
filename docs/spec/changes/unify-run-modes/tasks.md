# Tasks: unify-run-modes

> 状态：🔴 DRAFT（待 User 审批）| 见 [proposal.md](proposal.md)
> 更新：2026-07-28（取消单文件 + 合并多 exe 目标；六阶段，build 侧先于 run 侧）
> 每阶段独立可 commit + 可全绿；IMPL 起步前逐阶段查 ACTIVE.md 排锁。

## 锁现状（2026-07-28）
- `runtime` 空闲 → **P0 可立即起**
- `toolchain` 空闲 → P1 / P4 / P5 可起
- `compiler` 被 `nested-types-followup` 占 → P2 / P3 排队
- 依赖顺序：P3（多 exe 构建 build 侧）必须先于 P4（`--bin` run 侧）

## P0 — 设置 SoT 收敛 + VM 端 [runtime] 解析（runtime 单锁）✅ 已落地（#48，2026-07-28）
> design: [design.md](design.md) | spec: [specs/runtime-settings/spec.md](specs/runtime-settings/spec.md)
- [x] 0.1 `KnobSpec` 加 `toml_key` 字段（config.rs）
- [x] 0.2 KNOWN_KNOBS 补 `Z42_JIT_PROFILE` / `Z42_TARGET`(reserved) / `Z42_CONFIG`（字母序插入）+ 为每条填 `toml_key`
- [x] 0.3 修 `Z42_GC_MINOR_THRESHOLD` 描述失真（照 struct 字段文档 → 存活率 0.75）
- [x] 0.4 `RuntimeConfig::resolve(env, runtime_table)` 分层：env > 文件 > 默认；`from_getter` 退化为 `resolve(_, None)`
- [x] 0.5 `Z42_CONFIG` 加载：`load_runtime_toml` 读 `[runtime]` 段（缺失→None+warn，解析错→显式 error）
- [x] 0.6 `Z42_JIT_PROFILE` 去 straggler：`jit/lazy.rs` 改读 `runtime_config().jit_profile`
- [x] 0.7 `--info`（main.rs）枚举 `name/toml_key` + 生效 Z42_CONFIG 路径 + 逐旋钮 [env]/[config]/[default]
- [x] 0.8 单测 42 全绿（非破坏等价 / 优先级 / 文件缺失不 panic / 坏 TOML 显式报错 / 字母序不变式）
- [x] 0.9 GREEN：release 无告警 + runtime 917 测试 + config 42 + `--info` e2e；冷启动 CI 全 gate 绿（#48）

**P0 Scope（文件）**：`src/runtime/src/config.rs` · `src/runtime/src/main.rs` · `src/runtime/src/jit/lazy.rs` · config tests（同文件 mod tests）

## P1 — 侧车 JSON→TOML（toolchain）🟡 IMPL（2026-07-28）
- [x] launcher `_cmdRun` 读 `.runtimeconfig.toml`（Std.Toml），退役 JSON 读取（顶层 `version`）
- [x] **弃 configProperties 注入** → 改设 `Z42_CONFIG=sidecar`，由 z42vm 端 P0 分层解析读 `[runtime]`（尊重用户显式 Z42_CONFIG）
- [x] `~/.z42/config.toml` 的 `_defaultVersion` 换 Std.Toml（消手写单行扫描）
- [x] ~~`z42 publish` 侧车产出改 TOML~~ **空操作**：侧车是 .NET 风格手写文件，全仓无生成器/无实体样例（探查确认）
- [x] 文档：launcher.md runtimeconfig 段 JSON→TOML + Z42_CONFIG 机制；apphost 注释；依赖注释
- [ ] GREEN：冷启动 CI（compile-toolchain 编 launcher + test-host e2e）——本地 cold worktree 不可编 z42，以 CI 为权威

**P1 Scope（文件）**：`src/toolchain/launcher/core/launcher.z42` · `.../z42.launcher.z42.toml`(注释) · `docs/design/runtime/launcher.md`

## P2 — config 驱动执行模式（runtime；方向 B）🟡 IMPL（2026-07-28）
> 方向 (B)：mode 走 `[runtime]` 配置驱动，不烤 zpkg 格式（User 定 2026-07-28）。
> **P2a（runtime，本 PR）**——mode 成为分层链的一等旋钮：
- [x] `Z42_MODE` 旋钮进 KNOWN_KNOBS（toml_key `mode`，字母序 LOG<MODE<NATIVE）；`RuntimeConfig.mode: Option<String>`（raw，main.rs 校验+feature-gate）
- [x] main.rs `effective_mode` 的 `None` 臂（无 `--mode` CLI）→ `resolve_config_mode(runtime_config().mode)` → 落 build 默认；优先级 `--mode CLI > Z42_MODE/[runtime].mode > build 默认`
- [x] `resolve_config_mode`：interp/jit/aot 校验 + jit/aot feature-gate（未编入则 warn + 落默认）；未知值 warn + 落默认
- [x] 单测 5（注册/unset/env raw/env>table/table>默认）+ 全 config 47 全绿（字母序不变式保持）+ `--info` e2e（`[runtime].mode=interp` → `[config]` 生效）+ runtime 全量无回归
- **P2b（launcher）🟡 IMPL（2026-07-28）**：`z42 run <dir>` 读 manifest `[profile.debug].mode` → 设 `Z42_MODE`（源码工程运行时 profile 驱动 mode）
  - [x] `_buildAndResolveRun` 读 `_profileMode(pm, "debug")`，`Z42_MODE` 未设时注入（尊重用户显式 Z42_MODE；`--mode` CLI 仍最高）；z42vm 子进程经 P2a 层消费
  - [x] fixture two_mains 加 `[profile.debug].mode=interp`（z42c 忽略 profile，launcher smoke 的 `z42 run --bin` 路径 exercise 之）
  - **观测局限**：mode 切换对 stdout 不可见（interp/jit 都出 "greet-main"）→ smoke 验「profile-read 路径不崩 + run 仍对」；mode 解析本身由 P2a 单测覆盖

## P3 — 多 exe 构建 build 侧（compiler）🟡 IMPL（2026-07-28）
> spec: [specs/multi-exe-targets/spec.md](specs/multi-exe-targets/spec.md) | design: design.md「多 exe 目标」节
- [x] `Main.z42`：`ExeCount>0` 遍历 `pm.Exes` 重烤 name/entry 各产 `dist/<name>.zpkg`（compile-once-restamp，packed+indexed）；`ExeCount==0` 走现有单产物路径（自包含 early-return，字节不变）
- [x] `isExe` 纳入 `ExeCount>0`；preserved 快路径 + entry 自动探测校验按 `ExeCount==0` 门控（多 exe 全量 + 用显式 entry）
- [x] ~~`PackageCompile` 按 exe 各编一次~~ 首切用 compile-once-restamp（共享 [sources]），无需改 PackageCompile
- [x] ~~z42b _orchestrate~~ 不需要：`z42 build`→`_forwardZ42c` 直跑 z42c，多 exe 全在 driver
- **首切边界（deferred）**：
  - [ ] `ProjectInfo`/`ManifestLoader` `default-run` 字段（占 stdlib 锁，现被 converge-z42c 占 → 后置；当前多 exe 无 --bin 由 P4 报错列名）
  - [ ] exe 专属 `src` 子集（当前声明 `src` → 显式报错「尚未支持」）
  - [ ] 多 exe 增量 preserved（当前一律全量）
- [x] e2e：新增 multi-exe runner（`scripts/test/xtask_test_multiexe.z42`，仿 cross-zpkg）+ fixture `src/tests/multi-exe/two_mains/`（一工程双 [[exe]]→build 产两 zpkg→各跑→比对）；wire 进 `xtask test`（主 gate + `--dir multi-exe`）；两处非 golden 排除（`_isNonRegenCat`/`_isNonRunnableCat`）。**CI 权威**（cold worktree 不可本地跑）
- [ ] **自举不动点 gen1==gen2**（非破坏关键证据）—— CI 权威（cold worktree 不可本地验）

## P4 — 统一前门 + run 选择 run 侧（toolchain）🟡 IMPL（2026-07-28）
- [x] `_cmdRun` 源码工程分类：`_isSourceProject`（目录 / `*.z42.toml` → build+run；`.zpkg`/`.zbc` 走现有产物路径）
- [x] `z42 run <dir>`：`_buildAndResolveRun` 调 `_forwardZ42c build <manifest>` → 用 `ManifestLoader.Load` 定位产物 zpkg → 落到现有 run 逻辑
- [x] `--bin X` → 跑 `dist/<X>.zpkg`（校验是已声明 [[exe]]）；无 --bin 多 exe → 报错列名；--bin 不存在 → 报错列名；单 exe → `dist/<name>.zpkg`
- [x] 无 manifest → 明确报错；launcher 加 `z42.project` 依赖 + `using Z42.Build.Project`
- **首切边界（deferred）**：default-run（占 stdlib 锁）；`[build].dist_dir` 覆盖（默认 `<dir>/dist`）；workspace/本地依赖拓扑构建；`-p` workspace 成员选择
- [x] e2e（选 a）：`_launcherSmoke` 加 `z42 run <fixture> --bin greet → greet-main`（打包 launcher → z42c build → 跑选中 exe，真端到端；复用 P3 的 two_mains fixture，`Directory.Copy` 进 temp 避免污染源树）
- [x] **CI 覆盖**（选 i）：`_testDistRun` 加 `DIST_SMOKE_ONLY=launcher` env 门（只跑 launcher smoke）；ci.yml package-host 加一步 `test dist` 跑该 smoke（**linux+macos**，Windows 因 dist-test 预存 `.exe` 后缀缺口 gate 掉，另修）→ P4 `z42 run --bin greet → greet-main` 真端到端 CI 绿（linux-arm64/x64 + macos）
- **CI 抓到并修的 2 个真 P4 bug**（compile 阶段抓不到）：① `_findProjectToml` 漏裸 `z42.toml` ② build 进度 `wrote ->` 污染程序 stdout（改 Stdout(Null)）
- **遗留**（另开）：`_testDistRun` 预检 + `_launcherSmoke` 的 Windows `.exe` 后缀处理（预存 debt，与 P4 无关）

## P5 — publish 每 main 一 app（toolchain / z42b）🟡 IMPL（2026-07-28）
- [x] `builder_publish.z42` `_pubDesktop`：`ExeCount>0` 遍历 `[[exe]]` → 每 exe `_pubEnsureBuilt(ex.name)`（首次 build 全部、余者 resolve）→ `_pubProduceApphost(dist/<ex>.zpkg, root/<ex>, stub)`；`ExeCount==0` 走现有单 app 路径（非破坏）
- [x] `_pubExes(toml)` helper（TomlValue 直读 `[[exe]]` 名，仿 `_parseExes`，零新依赖、保 stdlib-only）；三个 `_pub*` helper 已按 name 参数化无需改
- [x] 多 exe 忽略 `[platform.desktop]` bin/payload 单值布局 → `root/<ex.name>`；deps bundle 一次（共享 dist）
- [x] e2e：`_apphostSmoke` 加多 exe publish smoke（2 `[[exe]]`→断言 aexe+bexe 两 apphost 产出）
- **CI 覆盖**：per-PR CI = compile-toolchain 验 z42b 编译；**功能 smoke 在 `xtask test dist`（local/release）跑**（_apphostSmoke 被 DIST_SMOKE_ONLY=launcher gate 掉）——如需进 per-PR CI 另议（会首次在 CI 跑 publish 路径）
- [ ] ~~examples/hello.z42.toml 装饰性 [[exe]]~~ 延后（examples 不进 CI，纯装饰）

## 文档（归档前必须落地）
- [ ] `docs/design/runtime/runtime-settings.md`（NEW）
- [ ] launcher.md / project.md（+`[[exe]]`/default-run）/ features.md / roadmap.md 更新

## 未决
无。设计定稿（2026-07-28）：取消单文件（Option 3）+ 合并多 exe 目标（接回归档特性）。
