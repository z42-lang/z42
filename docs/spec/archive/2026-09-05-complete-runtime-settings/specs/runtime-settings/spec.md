# Spec: 运行时设置补完（CLI 层 / 可用性 / 侧车 / 脚本读取 / 查询）

> Capability: `runtime-settings`（延续 [archive/2026-07-29-unify-run-modes](../../../../archive/2026-07-29-unify-run-modes/specs/runtime-settings/spec.md) 的同名 capability）。
> 父提案 [proposal.md](../../proposal.md)，方案 [design.md](../../design.md)。
> 本 spec 只定义本 change 的 **delta**；P0 已有的 `env > 文件 > 默认` 行为不重复陈述，仅在被修改处标 MODIFIED。
> 层名：L1 `cli` / L2 `env` / L3 `user-config`（`Z42_CONFIG`）/ L4 `app-config`（`Z42_APP_CONFIG` 侧车）/ L5 `default`。

## ADDED Requirements

### Requirement: CLI `--set` 是优先级链最高层

#### Scenario: `--set` 压过环境变量
- **WHEN** 环境有 `Z42_GC_MODE=stw`，命令行为 `z42vm --set gc-mode=concurrent app.zpkg`
- **THEN** 生效 gc_mode = concurrent
- **AND** `--show-config` 该行标注来源 `[cli]`，并列出被忽略的 `[env] "stw"`（原因：被更高层覆盖）

#### Scenario: `--set` 只认完整 key
- **WHEN** 命令行为 `--set Z42_GC_MODE=concurrent`（env 名形式）
- **THEN** 报错并退出码 2（未知 key），消息含最近邻建议 `gc-mode`
- **AND** **NOT** 被当作 `gc-mode` 的等价写法接受

#### Scenario: 显式声明的 alias 可用
- **WHEN** 某旋钮的 `KnobSpec.aliases` 含 `"x"`，命令行为 `--set x=v`
- **THEN** 等价于用该旋钮的 `toml_key` 设置
- **AND** `--list-knobs` 为该旋钮打印其 alias 列表

#### Scenario: `--set` 可重复
- **WHEN** 命令行为 `--set gc-mode=concurrent --set safepoint-throttle=1`
- **THEN** 两个旋钮都生效于 `[cli]` 层

#### Scenario: 值中含 `=` 按第一个等号切分
- **WHEN** 命令行为 `--set path=/a=b:/c`
- **THEN** key = `path`，value = `/a=b:/c`

#### Scenario: 空值显式清空、回落下一层
- **WHEN** 环境有 `Z42_GC_MODE=concurrent`，命令行为 `--set gc-mode=`
- **THEN** CLI 层视为未设，生效值来自 `[env]` = concurrent

#### Scenario: 同一旋钮的专用 flag 与 `--set` 冲突 → 报错
- **WHEN** 命令行同时含 `--mode interp` 与 `--set mode=jit`
- **THEN** 明确报错（指出两者同层、要求只给一个），退出码 2
- **AND** **NOT** 静默选择其一

#### Scenario: `--set` 未知 key → 致命 + 建议
- **WHEN** 命令行为 `--set gc-mod=stw`
- **THEN** 报错并退出码 2，消息含最近邻建议 `gc-mode`

---

### Requirement: 每个旋钮声明可接受的输入层

#### Scenario: KnobSpec 携带 `sources` 掩码
- **WHEN** 读取 `KNOWN_KNOBS` 任一条目
- **THEN** 提供 `sources`，指明该旋钮接受 `cli` / `env` / `user-config` / `app-config` 中的哪些层

#### Scenario: 元旋钮不接受配置文件层
- **WHEN** 枚举 `Z42_CONFIG` / `Z42_APP_CONFIG` / `Z42_STRICT_CONFIG`
- **THEN** 其 `sources` 只含 `cli` 与 `env`，`toml_key` 为空串，`tier` 为 `Internal`
- **AND** 在 `[runtime]` 表中写这些 key 不产生任何效果（不自指）

#### Scenario: 从不被接受的层设置 → 诊断
- **WHEN** 在命令行给出 `--set stress-iters=5`（该旋钮 `sources` 仅含 `env`）
- **THEN** 报错并退出码 2，消息指出该旋钮不能从 `[cli]` 设置、并列出它实际接受的层

#### Scenario: `--list-knobs` 展示可接受层
- **WHEN** 运行 `z42vm --list-knobs`
- **THEN** 每条旋钮输出其可接受层

---

### Requirement: 每个旋钮声明可用性（build / feature / platform）与支持级别

#### Scenario: KnobSpec 携带完整 schema
- **WHEN** 读取 `KNOWN_KNOBS` 任一条目
- **THEN** 除已有五字段外，还提供 `aliases`、`value`（类型）、`sources`、`build`、`requires`、`platforms`、`tier`

#### Scenario: 可用性 = 四项全通过
- **WHEN** 对给定的 (build profile, 已启用 feature 集合, OS, 来源层) 求值某旋钮
- **THEN** 仅当 `sources` 允许该层、`build` 满足、`requires` 中每个 feature 都启用、且 `platforms` 允许该 OS 时，该值才生效

#### Scenario: `Z42_MODE` 的逐值门控不上升为旋钮级 requires
- **WHEN** 读取 `Z42_MODE` 的 `requires`
- **THEN** 为空列表（interp 在任何 build 都可用）
- **AND** `Z42_MODE=jit` 在无 `jit` feature 的 build 上仍由 `resolve_config_mode` 处理（warn + 落 build 默认），行为不变

---

### Requirement: 设置了不可用 / 不被接受的旋钮时明确告知，严重度按来源分层

#### Scenario: CLI 设置不可用旋钮 → 致命
- **WHEN** 在无 `jit` feature 的 build 上运行 `z42vm --set jit-profile=1 app.zpkg`
- **THEN** 报错并以退出码 2 终止
- **AND** 消息含：旋钮 key、来源 `[cli]`、不可用原因（需要 feature `jit`）、**本 build 实际启用的 feature 列表**、以及查看全部可用性的命令提示

#### Scenario: 环境变量设置不可用旋钮 → warn 但继续
- **WHEN** 在无 `jit` feature 的 build 上，环境有 `Z42_JIT_PROFILE=1`
- **THEN** stderr 输出一条同等信息量的 warn（含"已忽略该值，使用默认"）
- **AND** 进程正常继续运行，该旋钮取内置默认
- **AND** **NOT** 因此终止（装了该 env 的 CI 环境必须能跑 interp-only 二进制）

#### Scenario: 两个配置文件层同样是 warn
- **WHEN** 用户配置或应用侧车的 `[runtime]` 表含不可用旋钮
- **THEN** 同上：warn + 忽略 + 用默认

#### Scenario: `--strict-config` 把 warn 升级为 error
- **WHEN** 上述任一 warn 场景加上 `--strict-config`（或 `Z42_STRICT_CONFIG=1`）
- **THEN** 变为报错并退出码 2

#### Scenario: 未知的 `Z42_*` 环境变量 → warn，不致命
- **WHEN** 环境有 `Z42_NOT_A_KNOB=1`
- **THEN** 一条 warn 指出该名字不在 `KNOWN_KNOBS` 内
- **AND** 进程正常运行

#### Scenario: DebugOnly 旋钮在 release build 上不可用
- **WHEN** release build（`debug_assertions` 关）上设置 `Z42_STRESS_ITERS=5`
- **THEN** warn 指出该旋钮仅在 debug build 可用，并忽略

#### Scenario: 平台受限旋钮在该平台上不可用
- **WHEN** 在 wasm 目标上设置 `Z42_SAMPLE_HZ=100`
- **THEN** warn 指出该旋钮在本平台不可用（采样需要后台线程），并忽略

---

### Requirement: 解析产出 provenance，渲染器只读不重算

#### Scenario: 每个旋钮带来源标签
- **WHEN** 解析完成
- **THEN** 每个旋钮有一条 `ResolvedKnob`，其 `source` ∈ {`cli`,`env`,`user-config`,`app-config`,`default`}

#### Scenario: 被压过与被拒的值都被记录
- **WHEN** 同一旋钮在多层被设置，或某层的值因不可用 / 不被接受 / 非法被拒
- **THEN** `ignored` 列出每条 `(层, 原始值, 原因)`，原因 ∈ {被更高层覆盖, 不可用, 不接受该层, 非法值}

#### Scenario: 查询表面不重新读取 env
- **WHEN** `--info` / `--show-config` / `__cfg_source` 渲染某旋钮
- **THEN** 数据来自 `ResolvedKnob`
- **AND** **NOT** 各自再调 `std::env::var` 重算

---

### Requirement: 用户配置与应用配置是两个独立通道，逐 key 叠加

#### Scenario: 用户显式配置不再吞掉应用侧车
- **WHEN** 用户设了 `Z42_CONFIG=my.toml`（只含 `gc-mode`），应用侧车 `Z42_APP_CONFIG=app.runtimeconfig.toml`（含 `mode` 与 `safepoint-throttle`）
- **THEN** 三个旋钮**都生效**：`gc-mode` 来自 `[user-config]`，另两个来自 `[app-config]`
- **AND** **NOT** 因为用户设了 `Z42_CONFIG` 就整份丢弃侧车

#### Scenario: 同 key 时用户配置赢
- **WHEN** 两个文件都设了 `mode`
- **THEN** 生效值来自 `[user-config]`，`[app-config]` 的值记入 `ignored(被更高层覆盖)`

#### Scenario: launcher 用独立通道传侧车
- **WHEN** launcher 运行一个带 `.runtimeconfig.toml` 的应用
- **THEN** 它把侧车路径设进 `Z42_APP_CONFIG`
- **AND** **NOT** 设进 `Z42_CONFIG`

---

### Requirement: 工程 profile 由 build 烤进侧车，不再经环境变量注入

#### Scenario: build 生成侧车
- **WHEN** 工程 manifest 含 `[profile.debug].mode = "interp"`，运行 `z42 build`
- **THEN** 与 `dist/<name>.zpkg` 并列产出 `dist/<name>.runtimeconfig.toml`，其 `[runtime]` 表含 `mode = "interp"`
- **AND** 文件带生成标记头

#### Scenario: profile 可携带任意旋钮
- **WHEN** manifest 的 profile 段含 `mode` 之外的旋钮（如 `gc-mode`）
- **THEN** 它们同样被烤进侧车的 `[runtime]` 表

#### Scenario: 无 profile 段不产文件（非破坏）
- **WHEN** 工程没有 `[profile.*]` 段
- **THEN** 不产出侧车文件，`dist/` 内容与本 change 前逐字节一致

#### Scenario: 多 exe 每个产物一份侧车
- **WHEN** 工程声明多个 `[[exe]]`
- **THEN** 每个 `dist/<exe>.zpkg` 各配一份 `dist/<exe>.runtimeconfig.toml`

#### Scenario: 已存在的手写侧车不被覆盖
- **WHEN** 目标路径已存在且不含生成标记头
- **THEN** build 报错而非覆盖，并提示手写配置的正确放法

#### Scenario: launcher 不再注入 Z42_MODE
- **WHEN** 运行 `z42 run <工程目录>`
- **THEN** launcher **NOT** 设置 `Z42_MODE` 环境变量；profile 的 mode 经侧车走 `[app-config]` 层

#### Scenario: 链方向正确（回归门）
- **WHEN** manifest `[profile.debug].mode = "interp"`，同时用户 `Z42_CONFIG` 文件写 `mode = "jit"`
- **THEN** 生效 mode = jit（用户配置高于应用配置）

---

### Requirement: `z42vm` 提供 schema 与生效值两种结构化查询

#### Scenario: `--list-knobs` 输出 schema
- **WHEN** 运行 `z42vm --list-knobs`
- **THEN** 逐旋钮输出 CLI key / alias / TOML key / 类型 / 默认 / 可接受层 / 可用性 / tier / consumed_by / 说明
- **AND** 默认只含 `tier == Public` 的旋钮

#### Scenario: `--list-knobs --all` 含全部级别
- **WHEN** 运行 `z42vm --list-knobs --all`
- **THEN** 额外含 `Unsupported` 与 `Internal` 旋钮，每条标出其 tier

#### Scenario: `--show-config` 输出生效值与来源
- **WHEN** 运行 `z42vm --show-config`
- **THEN** 逐旋钮输出生效值 + 来源标签；对有 `ignored` 的旋钮追加解释行说明为什么某层的值没生效

#### Scenario: `--json` 产出机器可读形态
- **WHEN** 给上述两个命令加 `--json`
- **THEN** 输出单个合法 JSON 文档（可被 `serde_json` 解析），字段名稳定

#### Scenario: 查询命令不要求 `<FILE>`
- **WHEN** 运行 `z42vm --list-knobs` 或 `z42vm --show-config` 且不给文件参数
- **THEN** 正常输出并以 0 退出（与现有 `--info` 一致）

#### Scenario: `--info` 复用同一渲染器
- **WHEN** 运行 `z42vm --info`
- **THEN** 其旋钮块由与 `--show-config` 相同的渲染器产生，含全部旋钮

---

### Requirement: z42 脚本可只读地查询运行时设置

#### Scenario: 读取生效值
- **WHEN** z42 代码调用 `Std.Runtime.RuntimeConfig.Get("gc-mode")`
- **THEN** 返回分层解析后的生效值字符串；该旋钮取默认时返回 `null`

#### Scenario: 读取来源
- **WHEN** 环境有 `Z42_GC_MODE=concurrent`，z42 代码调用 `RuntimeConfig.Source("gc-mode")`
- **THEN** 返回 `"env"`

#### Scenario: 读取可用性
- **WHEN** 在无 `jit` feature 的 build 上调用 `RuntimeConfig.IsAvailable("jit-profile")`
- **THEN** 返回 `false`

#### Scenario: 枚举与转储
- **WHEN** 调用 `RuntimeConfig.Names()` / `RuntimeConfig.Dump()`
- **THEN** `Names()` 返回全部旋钮的 key 数组；`Dump()` 返回 `"key=value|source"` 形式的扁平数组

#### Scenario: 读取说明
- **WHEN** 调用 `RuntimeConfig.Describe("gc-mode")`
- **THEN** 返回该旋钮的一行说明；未知 key 返回 `null`

#### Scenario: 无写入表面
- **WHEN** 检查 `Std.Runtime.RuntimeConfig` 的公开成员
- **THEN** **NOT** 存在任何 `Set` / 修改类方法（配置在 boot 后冻结）

#### Scenario: builtin 追加注册不改动既有 BuiltinId
- **WHEN** 比较本 change 前后 `corelib` builtin 表
- **THEN** 新增的 `__cfg_*` 全部追加在表尾，已有条目索引不变

---

### Requirement: 运行配置文件格式只有 TOML

#### Scenario: `.toml` 行为不变
- **WHEN** `Z42_CONFIG` 指向 `.toml` 文件
- **THEN** 读其 `[runtime]` 表，行为与本 change 前逐字段一致

#### Scenario: `.json` 给迁移提示而非静默忽略
- **WHEN** `Z42_CONFIG` 或 `Z42_APP_CONFIG` 指向 `.json` 文件
- **THEN** 明确报错，消息说明 z42 的运行配置格式是 TOML 并给出 `.toml` 的正确写法
- **AND** **NOT** 静默把该文件层当作不存在

#### Scenario: 坏 TOML 仍显式报错（回归）
- **WHEN** 配置文件不是合法 TOML
- **THEN** 明确 error（不静默吞、不降级为默认）

## MODIFIED Requirements

### Requirement: `--info` 的旋钮块来源

**Before:** `print_build_info` 自行 `std::env::var(knob.name)` 逐旋钮重算来源，与 `RuntimeConfig::resolve` 是两份实现。
**After:** 读 `ResolvedKnob`，与 `--show-config` 共用渲染器。展示内容为超集（新增 tier / 可接受层 / 可用性 / ignored 解释行），已有行的信息不减少。

### Requirement: 优先级链扩为五层，CLI 在顶、应用侧车在底

**Before:** `env > [runtime] 文件 > 默认`；`--mode` 作为 `main.rs` 特例位于其上；应用侧车与用户配置共用 `Z42_CONFIG`（互斥）；profile 经 launcher 注入 `Z42_MODE` 落在 env 层。
**After:** `cli > env > user-config > app-config > default`；侧车走独立的 `Z42_APP_CONFIG` 通道并与用户配置逐 key 叠加；工程 profile 由 build 烤进侧车，落在 `app-config` 层。

## Pipeline Steps
- [ ] Lexer / Parser / TypeChecker / IR Codegen — 无关
- [x] VM 配置解析与诊断（`config/*`、`main.rs`）
- [x] corelib builtin 表（追加 6 条）
- [x] stdlib 源码（`Std.Runtime.RuntimeConfig`）
- [x] z42c driver 产物写出（侧车生成，P5）

## IR Mapping
无（不新增 IR 指令 / 不改 zbc·zpkg 格式；设置走侧车文件，不烤进产物字节）。
