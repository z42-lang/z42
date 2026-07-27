# Tasks: unify-run-modes

> 状态：🔴 DRAFT（待 User 审批）| 见 [proposal.md](proposal.md)
> 每阶段独立可 commit + 可全绿；IMPL 起步前逐阶段查 ACTIVE.md 排锁。

## 锁现状（2026-07-26）
- `runtime` 空闲 → **P0 可立即起**
- `toolchain` 空闲 → P1/P3/P4 可起
- `compiler` 被 `nested-types-followup` 占 → P2 排队
- `stdlib` 被 `converge-z42c-onto-z42-project` 占 → P4 物理迁移排队（若做）

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

## P3 — 统一分发器（toolchain）
- [ ] `RunEngine`：设置解析 + vm/libs 定位 + 派发
- [ ] `_cmdRun` 前门分类器（.zpkg/.zbc / .z42 / 目录 / 省略）
- [ ] ①③ 收敛到单一前门

## P4 — 源码运行（toolchain + stdlib）
- [ ] `SourceRunEngine`：合成 manifest（单文件）/ 加载 manifest（工程）
- [ ] `z42.scripting` 加 `CompileFile` / `CompileProject`
- [ ] 依赖 provider 注入接口（D6）：host 扫 `Z42_LIBS` / embed 显式注入
- [ ] 进程内 load/invoke（复用 `__load_bytecode_in_memory`）+ hash 增量缓存，缓存**随文件 `.cache/`**（D7）
- [ ] ~~z42.scripting 物理迁 stdlib~~ 不做（D8：依赖 z42c.* 仍在 compiler 树，随 (B) 一起动）

## 文档（归档前必须落地）
- [ ] `docs/design/runtime/runtime-settings.md`（NEW）
- [ ] launcher.md / project.md / repl.md / features.md / roadmap.md 更新

## 未决
全部已敲定（2026-07-27）：D6 依赖注入接口 / D7 随文件缓存 / D8 暂不迁移。
