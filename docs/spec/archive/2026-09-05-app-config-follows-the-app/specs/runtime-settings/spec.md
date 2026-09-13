# Spec: app-config 层由 app 文件推导

> Capability `runtime-settings` 的 delta。父提案 [proposal.md](../../proposal.md)，方案 [design.md](../../design.md)。

## ADDED Requirements

### Requirement: 运行一个 app 时，它旁边的侧车自动成为 app-config 层

#### Scenario: 直跑 zpkg
- **WHEN** `dist/demo.runtimeconfig.toml` 存在，运行 `z42vm dist/demo.zpkg`
- **THEN** 该文件的 `[runtime]` 表作为 `app-config` 层生效
- **AND** **NOT** 像此前那样被无视（需要有人先设 `Z42_APP_CONFIG`）

#### Scenario: 嵌入方同样适用
- **WHEN** 经 `z42_host_run_app(file, ..)` 运行同一个 app（桌面自包含 / wasm / iOS / Android）
- **THEN** 同样生效

#### Scenario: 侧车名由 app 文件的 stem 派生
- **WHEN** app 文件是 `app.zpkg`
- **THEN** 查找 `app.runtimeconfig.toml`（替换扩展名，不是追加）

#### Scenario: 显式设置优先
- **WHEN** `Z42_APP_CONFIG` 已设为另一个文件
- **THEN** 用它，**NOT** 用推导出的路径

#### Scenario: 没有侧车是常态，不是问题
- **WHEN** app 旁边没有同名 `.runtimeconfig.toml`（多数工程没有 `[profile.*]` 旋钮）
- **THEN** 没有 app-config 层，且**不产生任何 warn**

#### Scenario: 显式指向不存在的文件仍然 warn
- **WHEN** `Z42_APP_CONFIG` 被设成一个不存在的路径
- **THEN** 一行 warn（用户明确说了要用它），该层不存在

#### Scenario: 用户配置仍然压过它
- **WHEN** 侧车与 `Z42_CONFIG` 都设了同一个 key
- **THEN** 用户层赢；侧车的值记入 `ignored(被更高层覆盖)`

#### Scenario: 查询命令带上 app 也能看到该层
- **WHEN** 运行 `z42vm --show-config dist/demo.zpkg`
- **THEN** 输出反映该 app 跑起来会用的设置（含 app-config 层）

#### Scenario: 不给 app 文件时行为不变
- **WHEN** 运行 `z42vm --show-config`（无文件）
- **THEN** 没有 app-config 层，输出与本 change 前一致

---

### Requirement: 发现约定只有一处实现

#### Scenario: apphost 不再自己发现
- **WHEN** 检查 spawn apphost 的 `exec_app`
- **THEN** 它只设 `Z42_LIBS`，**NOT** 计算或设置 `Z42_APP_CONFIG`
- **AND** 经它启动的 app 仍然读到侧车（由 z42vm 推导）

#### Scenario: launcher 保留转发
- **WHEN** launcher 运行一个带侧车的 app
- **THEN** 它仍设 `Z42_APP_CONFIG`——它为 `version` pin 已把该文件读进来，路径在手，
  转发已知值不是重复发现（design.md Decision 1）

## MODIFIED Requirements

### Requirement: app-config 层的来源

**Before:** 只有 `Z42_APP_CONFIG` 环境变量。于是 `z42vm <app>` 直跑、以及一切嵌入形态
（wasm / iOS / Android / 桌面自包含）都拿不到 app 自己的运行配置。
**After:** `Z42_APP_CONFIG`（显式）**或**由 app 文件路径推导（约定）。

## Pipeline Steps
- [x] VM 配置装配（`config/source.rs`、`main.rs`）
- [x] 嵌入入口（`z42-host`）
- [x] apphost（删冗余）
- [ ] Lexer / Parser / TypeChecker / IR Codegen — 无关

## IR Mapping
无。
