# Spec: 内联 env 旋钮的收编

> Capability `runtime-settings` 的 delta。父提案 [proposal.md](../../proposal.md)，方案 [design.md](../../design.md)。

## ADDED Requirements

### Requirement: 8 个旋钮从四层都可设置

#### Scenario: CLI 可设
- **WHEN** 运行 `z42vm --set jit-threshold=5 --show-config`
- **THEN** 该旋钮显示值 5、来源 `[cli]`
- **AND** **NOT** 报 "cannot be set from [cli]"

#### Scenario: 配置文件可设
- **WHEN** `[runtime]` 表含 `stackalloc = "stats"`
- **THEN** 该值生效于 `user-config` / `app-config` 层

#### Scenario: 优先级链对它们同样成立
- **WHEN** 同一旋钮在 CLI 与 env 都设置
- **THEN** CLI 赢，env 值记入 `ignored(被更高层覆盖)`

#### Scenario: 消费点读到的是分层解析结果
- **WHEN** 通过配置文件设置 `jit-threshold`
- **THEN** JIT 模块构造时用的就是该值（**NOT** 只看环境变量）

---

### Requirement: 四个开关旋钮是真布尔

#### Scenario: falsey 值真的关闭
- **WHEN** 设置 `no-fusion` 为 `false` / `0` / `off` / `no` 之一
- **THEN** fusion **不**被关闭（该旋钮为假）
- **AND** **NOT** 像 Flag 语义那样"设了就算开"

#### Scenario: truthy 值开启
- **WHEN** 设置为 `true` / `1` / `on` / `yes` 之一
- **THEN** 该旋钮为真

#### Scenario: 非布尔值是类型错误
- **WHEN** 设置为 `maybe`
- **THEN** 产生一条诊断（含 "expected a boolean"），该旋钮取默认

#### Scenario: 四个旋钮都完成转换
- **WHEN** 读取 `Z42_NO_FUSION` / `Z42_NO_TYPED_FUSION` / `Z42_FUSION_DEBUG` /
  `Z42_JIT_DEBUG_PROMOTE` 的 `value`
- **THEN** 均为 `ValueKind::Bool`
- **AND** 表内**不再有** `ValueKind::Flag` 的使用者（若有，须各自说明为什么活该保留）

---

### Requirement: `Z42_STACKALLOC` 的拼写错误被报出来

#### Scenario: 合法值行为不变
- **WHEN** 设置为 `off` / `0` / `heap` / `stats` / `on`
- **THEN** 分别是关 / 关 / 关 / 统计 / 开——与本 change 前一致

#### Scenario: 拼错不再静默变成"开"
- **WHEN** 设置为 `of`（`off` 的 typo）
- **THEN** 产生一条诊断（含 "expected one of"），落默认（开）
- **AND** **NOT** 静默当作"开"而让 triage 的人以为已经关掉了优化

---

### Requirement: 阈值旋钮的既有语义保持

#### Scenario: 默认值不变
- **WHEN** 未设置
- **THEN** `jit-threshold` = 2、`osr-threshold` = 10000

#### Scenario: 0 仍被 clamp 到 1
- **WHEN** 设置为 `0`
- **THEN** 生效值为 1

#### Scenario: 非整数从静默落默认变成明说
- **WHEN** 设置为 `abc`
- **THEN** 产生一条诊断（含 "expected an integer"），落默认

---

### Requirement: 登记表不再声称"内联 env 读"

#### Scenario: 防腐门反转
- **WHEN** 枚举 `KNOWN_KNOBS`
- **THEN** 除显式标 `ENV_ONLY` 的测试脚手架与三个元旋钮外，`sources` 均为四层全收
- **AND** 没有条目的 `consumed_by` 仍含 "(inline env read)"

## MODIFIED Requirements

### Requirement: 8 个旋钮的可接受层

**Before:** `LayerMask::ENV_ONLY` —— 它们在 `consumed_by` 处直接 `std::env::var`，
CLI/文件层到不了那行代码；标成四层全收会让 `--list-knobs` 说谎。
**After:** `LayerMask::ALL`，因为消费点已改读 `runtime_config()`。

### Requirement: 四个开关旋钮的值类型

**Before:** `ValueKind::Flag`（存在即启用；`=0` 仍然生效）。
**After:** `ValueKind::Bool`。理由见 design.md Decision 2：Flag 惯例活不过配置文件。

## Pipeline Steps
- [x] VM 配置解析与消费点（`config/*`、`jit/`、`interp/`、`metadata/`、`corelib/`）
- [ ] Lexer / Parser / TypeChecker / IR Codegen — 无关

## IR Mapping
无。
