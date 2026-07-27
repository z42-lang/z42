# Spec: 运行时设置解析（P0）

> Capability: `runtime-settings`。父提案 [proposal.md](../../proposal.md)，方案 [design.md](../../design.md)。
> 本 spec 只定义 **P0** 的可验证行为（VM 端分层解析地基 + 注册表 SoT 收敛）。

## ADDED Requirements

### Requirement: KNOWN_KNOBS 是运行时旋钮的唯一 SoT，含 env↔TOML 映射

#### Scenario: 每个旋钮声明 env 名与 TOML key
- **WHEN** 读取 `KNOWN_KNOBS` 任一条目
- **THEN** 它同时提供 `name`（`Z42_*` env 名）、`toml_key`（kebab-case、去 `Z42_` 前缀小写，如 `Z42_GC_MODE`→`gc-mode`）、`default_hint`、`consumed_by`

#### Scenario: 注册表不变式保持
- **WHEN** 运行 `config.rs` 的注册表单测
- **THEN** KNOWN_KNOBS 仍按 `name` 字母序、无重复；每个 `RuntimeConfig` 路径字段仍在表内

#### Scenario: 补齐漏网旋钮
- **WHEN** 枚举 KNOWN_KNOBS
- **THEN** 含 `Z42_JIT_PROFILE`（consumed_by `jit/lazy.rs`）与 `Z42_TARGET`（consumed_by `reserved (not yet implemented)`）
- **AND** `Z42_CONFIG`（`[runtime]` 配置文件路径指针）也在表内

### Requirement: `Z42_GC_MINOR_THRESHOLD` 描述与实现语义一致

#### Scenario: 修正失真描述
- **WHEN** 读取 `Z42_GC_MINOR_THRESHOLD` 的 `description` / `default_hint`
- **THEN** 描述反映实际语义（年轻代存活率阈值，0.0–1.0），默认标注 `0.75`（survival ratio）
- **AND** **NOT** 出现旧的 "bytes of allocation" / "64 KiB" 措辞

### Requirement: RuntimeConfig 分层解析 env > 配置文件 > 默认

#### Scenario: 环境变量命中优先于配置文件
- **WHEN** `Z42_GC_MODE=concurrent` 且 `[runtime]` 表含 `gc-mode = "stw"`
- **THEN** 解析结果 gc_mode = concurrent（env 赢）

#### Scenario: 配置文件命中优先于默认
- **WHEN** `Z42_GC_MODE` 未设，`[runtime]` 表含 `gc-mode = "concurrent"`
- **THEN** 解析结果 gc_mode = concurrent（文件赢默认）

#### Scenario: 三源皆缺回落默认
- **WHEN** env 未设、无配置文件（或无该 key）
- **THEN** 解析结果 = 内置默认（如 gc_mode = stw-mark-sweep）

#### Scenario: 非破坏——无配置文件时行为不变
- **WHEN** `Z42_CONFIG` 未设，其余 env 与现状相同
- **THEN** `resolve(env, None)` 逐字段等于旧 `from_env()` 的结果
- **AND** `xtask test` 的 e2e / golden 产物逐字节不变

### Requirement: `Z42_CONFIG` 指向的 TOML `[runtime]` 段被读入

#### Scenario: 加载存在的配置文件
- **WHEN** `Z42_CONFIG=/path/cfg.toml`，该文件含 `[runtime]\ngc-mode = "concurrent"`
- **THEN** 解析时 `[runtime]` 段作为配置文件层参与分层，gc_mode = concurrent（env 未覆盖时）

#### Scenario: 文件缺失不致命
- **WHEN** `Z42_CONFIG` 指向不存在的路径
- **THEN** 配置文件层视为 None，回落 env/默认，不 panic（warn 级 log）

#### Scenario: 解析错误显式报错
- **WHEN** `Z42_CONFIG` 指向的文件不是合法 TOML
- **THEN** 明确 error（不静默吞、不降级为默认）

### Requirement: `--info` 枚举旋钮 schema

#### Scenario: 展示 env↔TOML↔默认映射
- **WHEN** 运行 `z42vm --info`
- **THEN** 逐旋钮打印 `name (env) | toml_key | default_hint | consumed_by`
- **AND** 若 `Z42_CONFIG` 生效，打印其路径

## MODIFIED Requirements

### Requirement: JIT profiling 开关纳入 RuntimeConfig

**Before:** `jit/lazy.rs` 直接 `env::var("Z42_JIT_PROFILE").is_ok()`，游离于中央配置外。
**After:** 经 `runtime_config().jit_profile: bool` 读取，纳入分层解析与 `--info` 枚举。行为（是否开 profiling）不变。

## Pipeline Steps
仅涉及 VM 配置层，不触及编译 pipeline：
- [ ] Lexer / Parser / TypeChecker / IR Codegen — 无关
- [x] VM 配置解析（`config.rs` / `main.rs` / `jit/lazy.rs`）

## IR Mapping
无（不新增 IR 指令 / 不改 zbc·zpkg 格式）。
