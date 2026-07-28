# Spec: 多 exe 目标（P3–P5）

> Capability: `multi-exe-targets`。父提案 [proposal.md](../../proposal.md)，方案 [design.md](../../design.md)「多 exe 目标」节。
> 接回归档特性 `add-multi-exe-target`（迁移时丢失消费逻辑）。P3 build 侧 → P4 run 侧 → P5 publish 侧。

## ADDED Requirements

### Requirement: `[[exe]]` 多目标构建产出 per-exe 产物

#### Scenario: 多 exe → 多 zpkg
- **WHEN** manifest 含 `[[exe]]` 两项（server / cli），各带 `name` + `entry`
- **THEN** `z42 build` 产出 `dist/server.zpkg` 与 `dist/cli.zpkg`，各自 META 段 entry 分别为 `App.Server.Main` / `App.Cli.Main`

#### Scenario: exe 专属源集
- **WHEN** 某 `[[exe]]` 声明 `src = [...]`（SrcCount > 0）
- **THEN** 该 exe 只编译其 `src` 源集；未声明（SrcCount == 0）→ 沿用 `[sources]`

#### Scenario: 无 `[[exe]]` 时行为不变（非破坏）
- **WHEN** manifest 无 `[[exe]]`（ExeCount == 0）
- **THEN** 走现有单入口路径，产 `dist/<project.name>.zpkg`，产物逐字节与本 change 前一致

#### Scenario: 自举不动点
- **WHEN** 用改动后的 z42c 自编译 z42c 源（本身无 `[[exe]]`）
- **THEN** gen1 == gen2 byte-identical（多 exe 循环不影响单入口路径）

### Requirement: `default-run` 字段

#### Scenario: 解析 default-run
- **WHEN** `[project]` 段含 `default-run = "server"`
- **THEN** `ProjectInfo` 携带该默认；多 exe 无显式选择时用它

### Requirement: `z42 run --bin` 选择目标

#### Scenario: 按 bin 名跑
- **WHEN** 多 exe 工程 `z42 run <dir> --bin cli`
- **THEN** build（增量）后跑 `dist/cli.zpkg`（其 entry 已烤好，无需入口覆盖）

#### Scenario: 多 exe 无 --bin 用 default-run
- **WHEN** 多 exe 工程 `z42 run <dir>`，manifest 有 `default-run = "server"`
- **THEN** 跑 `dist/server.zpkg`

#### Scenario: 多 exe 无 --bin 无 default-run → 报错
- **WHEN** 多 exe 工程 `z42 run <dir>`，无 `default-run` 且无 `--bin`
- **THEN** 明确报错，列出所有可选 exe 名（不猜、不默认取第一个）

#### Scenario: --bin 名不存在
- **WHEN** `--bin nope` 但 manifest 无该 exe
- **THEN** 报错列出有效 exe 名

### Requirement: `z42 publish` 每 exe 一个 app

#### Scenario: 多 exe 各产 app
- **WHEN** 多 exe 工程 `z42 publish`
- **THEN** 每个 `[[exe]]` 产一个 apphost（指向其 `dist/<name>.zpkg`），各可独立分发；复用现有 apphost-per-zpkg，不改 payload

## MODIFIED Requirements

### Requirement: `[[exe]]`（ExeTarget）从 parsed-but-dead 变为被消费

**Before:** `ManifestLoader._parseExes` 解析出 `ProjectManifest.Exes`，但 driver / z42b / launcher 全走 `ProjectInfo.Entry` 单入口，`Exes` 无任何消费方。
**After:** driver 遍历 `pm.Exes` 产多产物；launcher `--bin` 按 exe 名选产物；publish 按 exe 产 app。`Exes` 成为多目标构建的 SoT。

## Pipeline Steps
- [x] Manifest 解析（`ManifestLoader` — `_parseExes` 现成 + `default-run` 补）
- [x] IR Codegen / 产物写盘（`Main.z42` 多产物循环 + `ZpkgWriter` entry 烘焙现成）
- [ ] Lexer / Parser / TypeChecker — 无关

## IR Mapping
无新 IR 指令；不改 zbc·zpkg 格式（entry 已在 META 段，复用现有烘焙/读取链）。
