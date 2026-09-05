# Spec: launcher 转发 `--set`

> Capability `runtime-settings` 的 delta。父提案 [proposal.md](../../proposal.md)。

## ADDED Requirements

### Requirement: `z42 run` 把 `--set` 转发给 z42vm

#### Scenario: 基本转发
- **WHEN** 运行 `z42 run --set gc-mode=concurrent <app>`
- **THEN** app 内 `RuntimeConfig.Get("gc-mode")` 为 `concurrent`、`Source("gc-mode")` 为 `cli`

#### Scenario: 可重复
- **WHEN** 给出两个 `--set`
- **THEN** 两个旋钮都生效

#### Scenario: 值里含 `=` 原样透传
- **WHEN** `--set log=z42::jit=debug,z42=warn`
- **THEN** VM 收到完整的 value（切分归 VM，launcher 不碰）

#### Scenario: flag 位置不影响
- **WHEN** `--set` 出现在 app 路径之前或之后
- **THEN** 行为相同

#### Scenario: launcher 不校验 key
- **WHEN** `z42 run --set no-such-knob=1 <app>`
- **THEN** 错误来自 **z42vm**（未知旋钮 + 最近邻建议 + exit 2），**NOT** launcher 自己的校验

---

### Requirement: `z42 repl` 同样转发 `--set`

#### Scenario: REPL 生效且不串进 z42i 参数
- **WHEN** `z42 repl --set log=z42=debug -c '1 + 2'`
- **THEN** 正常求值出 `3`，且 `--set` 不作为 z42i 自己的参数出现

---

### Requirement: 未识别的 flag 明确报错

#### Scenario: app 之前的未知 flag
- **WHEN** `z42 run --bogus <app>`
- **THEN** 报错并列出已知 flag
- **AND** **NOT** 把 `--bogus` 当作 app 路径去查找工程

#### Scenario: app 之后的未知 flag
- **WHEN** `z42 run <app> --bogus`
- **THEN** 同样报错
- **AND** **NOT** 静默丢弃

#### Scenario: `--` 之后的一切仍属于程序
- **WHEN** `z42 run <app> -- --bogus`
- **THEN** 正常运行，`--bogus` 作为程序参数传入

## MODIFIED Requirements

### Requirement: `z42 run` 对未知 `--flag` 的处理

**Before:** app 路径未定时把它当作 app 路径（于是去找一个叫 `--bogus` 的工程）；已定时静默丢弃。
**After:** 明确报错并列出已知 flag。

## Pipeline Steps
- [x] toolchain（launcher 参数分发）
- [ ] 编译 pipeline / VM 语义 — 无关

## IR Mapping
无。
