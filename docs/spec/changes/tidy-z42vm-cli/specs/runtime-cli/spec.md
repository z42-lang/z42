# Spec: z42vm 命令行整理

> 新 capability `runtime-cli`。父提案 [proposal.md](../../proposal.md)。

## ADDED Requirements

### Requirement: 帮助文本只面向用户

#### Scenario: 不含内部溯源信息
- **WHEN** 运行 `z42vm --help`
- **THEN** 输出**NOT** 含 change 名（如 `complete-runtime-settings`）、日期、
  或 spec 路径（如 `docs/review.md`）

#### Scenario: 选项按职责分组
- **WHEN** 运行 `z42vm --help`
- **THEN** 选项分在「执行 / 运行时配置 / 自省 / 诊断」四组下

#### Scenario: 三个自省命令的区别可辨
- **WHEN** 阅读 `--info` / `--list-knobs` / `--show-config` 的帮助
- **THEN** 各自说清「构建信息快照」/「有哪些旋钮」/「旋钮当前是什么值、来自哪层」

---

### Requirement: 统计输出是一个 flag

#### Scenario: 默认文本
- **WHEN** `z42vm --stats <app>`
- **THEN** 退出时打印文本计数器块

#### Scenario: 指定 JSON
- **WHEN** `z42vm --stats=json <app>`
- **THEN** 打印单行 JSON

#### Scenario: 旧的两 flag 形态不再存在
- **WHEN** 运行 `z42vm --print-stats-on-exit`
- **THEN** clap 报未知参数（pre-1.0 不留别名）

---

### Requirement: 修饰符用错地方明确报错

#### Scenario: `--json` 不配合查询命令
- **WHEN** `z42vm --json <app>`
- **THEN** 报错并退出码 2，消息指出 `--json` 只对 `--list-knobs` / `--show-config` 有效
- **AND** **NOT** 静默忽略

#### Scenario: `--all` 同理
- **WHEN** `z42vm --all <app>`
- **THEN** 同上

#### Scenario: 配合使用时正常
- **WHEN** `z42vm --list-knobs --all --json`
- **THEN** 正常输出 JSON

## MODIFIED Requirements

### Requirement: 统计输出的 CLI 形态

**Before:** `--print-stats-on-exit` 开关 + `--stats-format <text|json>` 修饰符（后者单独出现无意义）。
**After:** `--stats [FORMAT]`，不给值即 text。

## Pipeline Steps
- [x] VM CLI（`main.rs`）
- [x] 调用方（`scripts/xtask_profile.z42`）
- [ ] 编译 pipeline — 无关

## IR Mapping
无。
