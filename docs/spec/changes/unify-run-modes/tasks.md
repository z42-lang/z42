# Tasks: unify-run-modes

> 状态：🔴 DRAFT（待 User 审批）| 见 [proposal.md](proposal.md)
> 每阶段独立可 commit + 可全绿；IMPL 起步前逐阶段查 ACTIVE.md 排锁。

## 锁现状（2026-07-26）
- `runtime` 空闲 → **P0 可立即起**
- `toolchain` 空闲 → P1/P3/P4 可起
- `compiler` 被 `nested-types-followup` 占 → P2 排队
- `stdlib` 被 `converge-z42c-onto-z42-project` 占 → P4 物理迁移排队（若做）

## P0 — 设置 SoT 收敛（runtime）
- [ ] 补 `Z42_JIT_PROFILE` / `Z42_TARGET` 进 `KNOWN_KNOBS`（config.rs）
- [ ] 修 GC minor threshold 描述（"64 KiB" → 比率 0.75）
- [ ] `RuntimeConfig` 加 `[runtime]` TOML 输入源
- [ ] 实现优先级合并：CLI > env > 文件 > profile > 默认
- [ ] `--info` 枚举补齐（含 TOML key + env 名映射）

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
