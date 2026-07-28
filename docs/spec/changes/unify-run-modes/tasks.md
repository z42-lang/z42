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

## P2 — profile.mode 打通（compiler，排队）
- [ ] z42c 解析 `[profile.*]` 段（Main.z42 现延后项）
- [ ] 运行路径消费 `mode`
- [ ] 自举不动点验证（gen1==gen2）

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
- [ ] 单测/e2e：双 exe→两 zpkg entry 正确、ExeCount==0 产物不变 —— **无现成 harness 跑「一工程→N zpkg→各跑」；待定：新 e2e runner vs 随 P4 run --bin 落地**
- [ ] **自举不动点 gen1==gen2**（非破坏关键证据）—— CI 权威（cold worktree 不可本地验）

## P4 — 统一前门 + run 选择 run 侧（toolchain）
- [ ] `_cmdRun` 前门分类器（`.zpkg`/`.zbc` → 跑产物；`<目录>`/省略 → 找 manifest）
- [ ] `z42 run <dir>`：调现有 `z42 build`（增量，已新鲜则跳过）→ 跑产出 `.zpkg`
- [ ] `--bin X` → 跑 `dist/X.zpkg`；无 --bin → default-run 否则报错列名；--bin 名不存在报错
- [ ] 无 manifest → 明确报错；workspace 目录支持 `-p`
- [ ] 单元/e2e：源码工程运行、增量跳过、--bin 选择、各报错路径

## P5 — publish 每 main 一 app（toolchain）
- [ ] `z42 publish` 遍历 `[[exe]]` 各配 apphost（复用 per-zpkg，不改 payload）
- [ ] 修 `examples/hello.z42.toml` 等装饰性 `[[exe]]` 为真可跑（补 kind/entry）
- [ ] e2e：双 exe→两 apphost 各可独立跑

## 文档（归档前必须落地）
- [ ] `docs/design/runtime/runtime-settings.md`（NEW）
- [ ] launcher.md / project.md（+`[[exe]]`/default-run）/ features.md / roadmap.md 更新

## 未决
无。设计定稿（2026-07-28）：取消单文件（Option 3）+ 合并多 exe 目标（接回归档特性）。
