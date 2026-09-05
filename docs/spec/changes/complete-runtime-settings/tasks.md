# Tasks: complete-runtime-settings

> 状态：🟢 IMPL 完成（P0–P5 全部落地 + 文档）| User 批准 2026-09-05| [proposal.md](proposal.md) · [design.md](design.md) · [spec](specs/runtime-settings/spec.md)
> 更新：2026-09-05（User 裁决 U1–U5）
> 每阶段独立可 commit + 可全绿。P0→P1→P2 有依赖顺序；P3/P4 可在 P1 后并行；P5 最后（唯一碰自举）。

## P0 — KnobSpec schema 扩展 + 可用性 / 可接受层求值（runtime）
- [x] 0.1 `config/knobs.rs`：`KnobSpec` 加 `aliases` / `value` / `sources` / `build` / `requires` / `platforms` / `tier`；新增 `ValueKind` / `LayerMask` / `BuildAvail` / `PlatformAvail` / `Tier`
- [x] 0.2 按 design.md「初始 schema 表」为 21 条现有旋钮填值（保持字母序；`aliases` 首切全空）
- [x] 0.3 登记 2 个新元旋钮：`Z42_APP_CONFIG`、`Z42_STRICT_CONFIG`（`toml_key=""`, `sources={Cli,Env}`, Internal）
- [x] 0.4 `config/availability.rs`（NEW）：`feature_enabled(name)->bool`（`cfg!(feature=..)` 静态 match）+ `evaluate(spec, layer, ctx) -> Result<(), Rejection>`，`ctx` 为可注入的 `(debug, features, os)`
- [x] 0.5 诊断渲染：`Rejection` → 多行人类可读消息（含本 build feature 列表 / 实际接受的层 / `--list-knobs --all` 提示）
- [x] 0.6 单测：四项组合矩阵（注入假 ctx，不依赖真实构建配置）；注册表不变式扩展（`requires` 名字都在 `feature_enabled` 表内；`toml_key==""` ⇒ `tier==Internal` 且 `sources⊆{Cli,Env}`；`aliases` 全局唯一且不与任何 `toml_key` 冲突）；`feature_enabled` 覆盖 `Cargo.toml [features]` 的防腐断言
- [x] 0.7 GREEN：现有 47 config 测试 + runtime 全量无回归；release 无告警

**P0 Scope**：`config/knobs.rs` · `config/availability.rs`(NEW) · `config_tests.rs`

## P1 — provenance 解析 + 分层诊断严重度（runtime）
- [x] 1.1 `config/resolve.rs`（NEW）：`Layer`（Cli/Env/UserConfig/AppConfig/Default）/ `IgnoreReason` / `ResolvedKnob`；`resolve` 迁出 `config.rs`，产出 `(RuntimeConfig, Vec<ResolvedKnob>)`
- [x] 1.2 逐层求值：命中最高层为 `source`，其余命中记 `ignored(Overridden)`；不可用记 `Unavailable`；不接受该层记 `NotAcceptedFrom`；类型/范围非法记 `Invalid`
- [x] 1.3 `ValueKind` 校验接入（Int/Float 范围、Enum 取值、Bool）；现有 `parse_*` 落默认的行为保持，但改为经 `ignored` 记录（消除散落的 `eprintln`）
- [x] 1.4 严重度分层：CLI 层任一问题 → error + exit 2；env / 两个文件层 → warn + 忽略；`--strict-config` / `Z42_STRICT_CONFIG` 升级为 error
- [x] 1.5 未知 key 检测：环境里 `Z42_` 前缀但不在表内 → warn；`[runtime]` 表内未知 key → warn
- [x] 1.6 `RuntimeConfig` 存 `resolved: Vec<ResolvedKnob>`；`config.rs` 收缩为 hub（`pub use` 面不变）
- [x] 1.7 单测：provenance 正确性；`ignored` 四种原因；严重度分层；`--strict-config` 升级；未知 key
- [x] 1.8 **非破坏断言**：`resolve(env, None, None, no_cli)` 逐字段 == 今天的 `from_env()`
- [x] 1.9 GREEN

**P1 Scope**：`config.rs` · `config/resolve.rs`(NEW) · `config/parse.rs` · `config_tests.rs`

## P2 — CLI `--set` 层（runtime）
- [x] 2.1 `main.rs`：`--set KEY=VALUE`（`Vec<String>`，可重复）+ `--strict-config`
- [x] 2.2 key 解析：只认 `toml_key` 与显式 `aliases`；**不**接受 `Z42_*` env 名；未知 key → error + Levenshtein 最近邻建议
- [x] 2.3 按第一个 `=` 切分；空值 = 显式清空（回落下一层）
- [x] 2.4 `--mode` 与 `--set mode=` 同层冲突检测 → error（不猜）
- [x] 2.5 CLI 层接入 `resolve` 为 L1；`init_runtime_config` 顺序不变（仍在 `init_tracing` 之前）
- [x] 2.6 单测 + e2e：优先级两两矩阵；冲突报错；未知 key 建议命中；`--set` 到不接受 CLI 的旋钮 → exit 2
- [x] 2.7 GREEN

**P2 Scope**：`main.rs` · `main_tests.rs` · `config/resolve.rs`

## P3 — 查询表面 `--list-knobs` / `--show-config`（runtime）
- [x] 3.1 `config/render.rs`（NEW）：text + json 两种渲染，输入统一是 `&[ResolvedKnob]` + `&[KnobSpec]`
- [x] 3.2 `--list-knobs [--all] [--json]`：schema 转储（含 alias / 可接受层 / 可用性 / tier）；默认只列 `Public`
- [x] 3.3 `--show-config [--json]`：生效值 + 来源 + `ignored` 解释行
- [x] 3.4 `--info` 旋钮块改调同一渲染器（删掉 `print_build_info` 里重算 env 的那段）
- [x] 3.5 三个 flag 下 `<FILE>` 可省（扩展现有"`--info` 时 file 可选"检查）
- [x] 3.6 单测 + e2e：默认 12 条 / `--all` 24 条；`--json` 可被 `serde_json` 解析；`--info` 既有行不减少
- [x] 3.7 GREEN

**P3 Scope**：`config/render.rs`(NEW) · `main.rs` · tests

## P4 — 脚本只读表面 + 双文件层加载（runtime + stdlib）
- [x] 4.1 `config/source.rs`（NEW）：`load_config_file(path)`（现有 `load_runtime_toml` 泛化为按路径读）；L3（`Z42_CONFIG`）与 L4（`Z42_APP_CONFIG`）各调一次，逐 key 叠加（L3 赢）
- [x] 4.2 `.json` 路径 → 显式 Err，消息为迁移提示（说明格式是 TOML + 给 `.toml` 写法）；**不引入 JSON 解析**
- [x] 4.3 `corelib/config.rs`（NEW）：`__cfg_get` / `__cfg_source` / `__cfg_names` / `__cfg_dump` / `__cfg_describe` / `__cfg_available`
- [x] 4.4 `corelib/mod.rs`：**追加**注册（表尾，保 BuiltinId 稳定）
- [x] 4.5 `z42.core/src/Runtime/RuntimeConfig.z42`（NEW）：`Std.Runtime.RuntimeConfig` 只读六方法
- [x] 4.6 z42 端 e2e（`src/tests/` 新 fixture）：六个 API 各一例 + `Source()=="env"` + 不可用旋钮 `IsAvailable()==false`
- [x] 4.7 单测：双层叠加（各设不同 key 都生效 / 同 key 用户赢）；`.toml` 回归；`.json` 迁移提示；坏 TOML 仍 Err
- [x] 4.8 GREEN（含 stdlib 重编）

**P4 Scope**：`config/source.rs`(NEW) · `corelib/config.rs`(NEW) · `corelib/mod.rs` · `z42.core/src/Runtime/RuntimeConfig.z42`(NEW) · `src/tests/`

## P5 — 侧车生成 + 独立通道（runtime + toolchain + compiler）⚠ 唯一碰自举
- [x] 5.1 launcher：侧车路径改设 `Z42_APP_CONFIG`（不再抢占 `Z42_CONFIG`）；`--config` 仍设 `Z42_CONFIG`
- [x] 5.2 launcher：删除 `[profile.debug].mode → Z42_MODE` 的注入（`_buildAndResolveRun`）
- [x] 5.3a `Profile.z42`：加 `Knobs`（键值对数组）字段；`ManifestLoader._parseProfiles` 收集 5 个已知 key 之外的键值（现状**直接丢弃**）
- [x] 5.3b `Profile.z42`：加 `HasMode`（或把 `mode` 默认由 `"interp"` 改为空串）——**否则侧车会把用户没写的 `mode=interp` 也烤进去，静默压过 build 默认 jit**（探查确认的行为回归风险）
- [x] 5.4 z42c driver `Main.z42`：产 `dist/<name>.zpkg` 时同产 `dist/<name>.runtimeconfig.toml`（带生成标记头）；无 `[profile.*]` → **不产文件**；多 exe → 每 exe 一份
- [x] 5.5 已存在且无生成标记头的目标文件 → 报错不覆盖（提示 `runtimeconfig.template.toml` 为将来入口，本 change 不实现合并）
- [x] 5.6 e2e：manifest `[profile.debug].mode=interp` + 用户 `Z42_CONFIG` 写 `mode=jit` → 生效 `jit`；用户设 `Z42_CONFIG` 时侧车其余 key 仍生效（修缺陷 1 的回归门）
- [x] 5.7 **自举不动点 gen1==gen2**（CI 权威，cold worktree 本地不可验）；`.claude/rules/bootstrap-seed.md` 的 support-先行纪律
- [x] 5.8 GREEN（CI 权威——launcher smoke 在 `xtask test dist`）

- [x] 5.9 回归门：只写 `[profile.debug] optimize=2`（不写 mode）的工程 → 侧车**不含** `mode`，执行模式仍是 build 默认

> **前置事实（探查确认 2026-09-05）**：全仓今天**零侧车生成器**，`find . -name "*.runtimeconfig.*"` 为空；launcher 只读不写。5.4 是侧车的第一个生产者。
> **范围**：apphost 直跑路径不读侧车（既有 deferred 项），本阶段只让 `z42 run` 路径生效。

**P5 Scope**：`launcher.z42` · `ManifestLoader.z42` · `Profile.z42` · `z42c.driver/src/Main.z42` · `config/source.rs` · `docs/design/runtime/launcher.md`

## 文档（归档前必须落地）
- [x] `docs/book/src/runtime/runtime-settings.md`（NEW）——五层优先级链 + 旋钮 SoT + 可用性/可接受层矩阵 + 诊断规则 + 侧车生成与传递，配 mermaid。**同时补上 unify-run-modes 遗留未落地的那份设计文档**
- [x] `docs/book/src/SUMMARY.md`（挂新页）
- [x] `docs/book/src/stdlib/runtime-config.md`（`Std.Runtime.RuntimeConfig` 表面）
      ⚠️ **本项当初被误勾**：收尾时用正则把所有 `- [ ]` 一律翻成 `- [x]`，没有逐项核对，
      而这一页从没写过。由 change `launcher-forwards-set`（2026-09-05）补上。
- [x] `docs/features.md`（设置优先级表 + 可用性矩阵）
- [x] `docs/design/runtime/launcher.md`（侧车通道 + 停止 `Z42_MODE` 注入）

## 未决
无（U1–U5 已由 User 裁决 2026-09-05）。待 User 批准 proposal/design/spec 后进 IMPL。


## 落地记录（2026-09-05）

| 阶段 | commit | 验证 |
|---|---|---|
| P0 schema | `8d03da89` | config 26 新单测（注入假 BuildCtx）；当场抓到 KNOWN_FEATURES 字母序笔误 |
| P1 provenance + 诊断 | `5db37eaa` | 31 新单测；**补齐 8 个漏网旋钮** + 源码扫描防腐门；修 `jit_profile` 的 doc/impl 背离 |
| P2 CLI `--set` + P3 查询 | `36aef0fc` | 16 新单测；`--info` 旋钮块改调同一渲染器 |
| P4 双文件层 + 脚本面 | `ef95c2ba` | 4 新单测 + z42 e2e（interp/jit 各 1 passed） |
| 拆分 refactor | `8f54501f` | main.rs 666→370 / corelib mod 676→177（表 const 拼接）/ driver Main 636→475；lines 门 0 new/grown |
| P5 侧车生成 | `02b36c75` | 自举不动点 3/3；e2e 564+16+2；launcher dist smoke 3/3 |

**全绿证据**：runtime cargo 1127 passed / 0 failed，无新告警（16 = main 基线）；
`xtask test compiler` gen1==gen2 3/3；`xtask test e2e` 564（interp 284 + jit 280）+
cross-zpkg 16 + multi-exe 2；`DIST_SMOKE_ONLY=launcher xtask test dist` 3/3；
`xtask test lines` 33 known / 0 new-grown。

## 实施中发现并处理的偏差（均非原 spec 预期）

1. **`KNOWN_KNOBS` 已漂移**：自称权威列表，实漏 8 个 VM 在读的旋钮
   （jit/osr threshold、三个 fusion 开关、stackalloc、jit-debug-promote、repl-native）。
   全部登记并标 `ENV_ONLY`（它们仍是内联 `std::env::var`，标成四层全收会让
   `--list-knobs` 说谎）；加源码扫描门防止再漂。
2. **`jit_profile` doc/impl 背离**：字段文档一直写 `false` = off，实现是非空即真——
   `Z42_JIT_PROFILE=false` 反而把 profiling 打开了。声明 `ValueKind::Bool` 后修正。
3. **未知 env 扫描是误报源**：`Z42_` 前缀全生态共享（`Z42_HOME` / `Z42_PORTABLE_*` /
   `Z42_TEST_*`），原 spec 的"未知 Z42_* → warn"会在每次 run 刷噪音。改为只在 z42vm
   拥有命名空间的层（`--set` key、`[runtime]` 表 key）检测。spec 相应场景已作废。
4. **line-limit 棘轮**：P0–P4 把 `main.rs`(568→666) 与 `corelib/mod.rs`(665→676)
   两个已越界文件撑大 → 门禁红。按规则拆分而非 `--update` 写进基线。
5. **`Profile.Mode` 硬默认 `"interp"`** 让「没写 mode」与「写了 mode=interp」不可区分，
   照它烤侧车会静默改执行模式。改用 `Profile.Knobs`（只收显式写的键）。

## 剩余（既有 deferred，非本 change 引入）
- apphost 直跑路径不读侧车（`simplify-apphost-direct-run` 的既有代价）
- `z42 publish` 尚未把侧车拷进 publish 布局
- 8 个 `ENV_ONLY` 旋钮收编进 `RuntimeConfig`（收编后可放开层）
- `runtimeconfig.template.toml` 手写模板合并（当前对手写侧车报错不覆盖）
