# Tasks: unify-run-modes

> 状态：🔴 DRAFT（待 User 审批）| 见 [proposal.md](proposal.md)
> 更新：2026-07-28（取消单文件 + 合并多 exe 目标；六阶段，build 侧先于 run 侧）
> 每阶段独立可 commit + 可全绿；IMPL 起步前逐阶段查 ACTIVE.md 排锁。

## 锁现状（2026-07-28）
- `runtime` 空闲 → **P0 可立即起**
- `toolchain` 空闲 → P1 / P4 / P5 可起
- `compiler` 被 `nested-types-followup` 占 → P2 / P3 排队
- 依赖顺序：P3（多 exe 构建 build 侧）必须先于 P4（`--bin` run 侧）

## P0 — 设置 SoT 收敛 + VM 端 [runtime] 解析（runtime 单锁）
> design: [design.md](design.md) | spec: [specs/runtime-settings/spec.md](specs/runtime-settings/spec.md)
- [ ] 0.1 `KnobSpec` 加 `toml_key` 字段（config.rs）
- [ ] 0.2 KNOWN_KNOBS 补 `Z42_JIT_PROFILE` / `Z42_TARGET`(reserved) / `Z42_CONFIG`（字母序插入）+ 为每条填 `toml_key`
- [ ] 0.3 修 `Z42_GC_MINOR_THRESHOLD` 描述失真（读 parse+arc_heap 确认措辞 → 存活率 0.75）
- [ ] 0.4 `RuntimeConfig::resolve(env, runtime_table)` 分层：env > 文件 > 默认；`from_env()` 退化为 `resolve(env, None)`
- [ ] 0.5 `Z42_CONFIG` 加载：读 TOML `[runtime]` 段（缺失→None+warn，解析错→显式 error）
- [ ] 0.6 `Z42_JIT_PROFILE` 去 straggler：`jit/lazy.rs` 改读 `runtime_config().jit_profile`
- [ ] 0.7 `--info`（main.rs）枚举 `name|toml_key|default|consumed_by` + 生效 Z42_CONFIG 路径
- [ ] 0.8 单测（config 测试现有位置）：非破坏等价 / 优先级 / 文件缺失不 panic / 不变式保持
- [ ] 0.9 GREEN：`cargo build --release` 无告警 + `xtask test`（e2e/golden 逐字节不变作非破坏证据）

**P0 Scope（文件）**：`src/runtime/src/config.rs`(MODIFY) · `src/runtime/src/main.rs`(MODIFY，--info) · `src/runtime/src/jit/lazy.rs`(MODIFY) · config 测试文件(MODIFY/NEW，随现有位置)

## P1 — 侧车 JSON→TOML（toolchain）
- [ ] launcher 读 `.runtimeconfig.toml`（Std.Toml），退役 JSON 读取
- [ ] `~/.z42/config.toml` 换 Std.Toml（消手写单行解析）
- [ ] `z42 publish` 侧车产出改 TOML

## P2 — profile.mode 打通（compiler，排队）
- [ ] z42c 解析 `[profile.*]` 段（Main.z42 现延后项）
- [ ] 运行路径消费 `mode`
- [ ] 自举不动点验证（gen1==gen2）

## P3 — 多 exe 构建 build 侧（compiler + z42b，排队）
> spec: [specs/multi-exe-targets/spec.md](specs/multi-exe-targets/spec.md) | design: design.md「多 exe 目标」节
- [ ] `ManifestLoader` + `ProjectInfo` 解析 `default-run` 字段
- [ ] `Main.z42`：`ExeCount>0` 遍历 `pm.Exes` 各产 `dist/<name>.zpkg`（entry=exe.Entry, 源集=exe.Src‖[sources]）；`ExeCount==0` 走现有单入口
- [ ] `PackageCompile` 按 exe 目标 entry/源集各编一次（若需）
- [ ] z42b `_orchestrate` 多目标（或下沉 driver、z42b 透传）
- [ ] 单测/e2e：双 exe→两 zpkg entry 正确、专属 src 只编子集、ExeCount==0 产物不变
- [ ] **自举不动点 gen1==gen2**（非破坏关键证据）

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
