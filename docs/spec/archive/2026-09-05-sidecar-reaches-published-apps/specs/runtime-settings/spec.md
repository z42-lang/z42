# Spec: 配置文件层到达已发布的 app

> Capability `runtime-settings` 的 delta。父提案 [proposal.md](../../proposal.md)，方案 [design.md](../../design.md)。

## ADDED Requirements

### Requirement: 非-z42vm 入口也读配置文件层

#### Scenario: 嵌入方读到用户配置
- **WHEN** 宿主设了 `Z42_CONFIG` 指向含 `[runtime] gc-mode = "concurrent"` 的文件，并经
  `z42_host_run_app` 运行一个 app
- **THEN** 生效 gc_mode = concurrent，来源标为 `user-config`
- **AND** **NOT** 像此前那样被静默忽略

#### Scenario: 嵌入方读到应用侧车
- **WHEN** 同上但用 `Z42_APP_CONFIG`
- **THEN** 该层生效，来源标为 `app-config`

#### Scenario: 两层在库入口同样逐 key 叠加
- **WHEN** 两个文件都设了 `mode`，只有侧车设了 `safepoint-throttle`
- **THEN** `mode` 取用户层、`safepoint-throttle` 取侧车层

#### Scenario: 库入口对坏配置只 warn，绝不退出
- **WHEN** `Z42_CONFIG` 指向非法 TOML（或 `.json`），经库入口初始化
- **THEN** stderr 一行说明、该层视为不存在、其余层照常解析
- **AND** **NOT** 终止进程（宿主可能是 iOS app / Android JNI / wasm）

#### Scenario: 可注入路径不受影响（非破坏）
- **WHEN** 调用 `RuntimeConfig::from_getter(fake_env)`
- **THEN** 结果与本 change 前逐字段一致（不读任何文件）

---

### Requirement: spawn apphost 把 app 侧车指给 z42vm

#### Scenario: 侧车存在则注入
- **WHEN** apphost 运行 `<dir>/app.zpkg` 且 `<dir>/app.runtimeconfig.toml` 存在
- **THEN** 子进程 z42vm 的环境含 `Z42_APP_CONFIG=<dir>/app.runtimeconfig.toml`

#### Scenario: 侧车不存在则不注入
- **WHEN** 同目录没有该文件
- **THEN** **NOT** 设置 `Z42_APP_CONFIG`

#### Scenario: 调用方显式设置优先
- **WHEN** 环境已有 `Z42_APP_CONFIG`
- **THEN** apphost 不覆盖它

#### Scenario: 侧车名由 zpkg 的 stem 派生
- **WHEN** app zpkg 名为 `app.zpkg`
- **THEN** 查找的是 `app.runtimeconfig.toml`（替换扩展名，不是追加）

---

### Requirement: `z42 publish` 把侧车带进部署布局

#### Scenario: payload 布局
- **WHEN** manifest 声明 `[platform.desktop].payload` 且 `dist/<name>.runtimeconfig.toml` 存在
- **THEN** 侧车被拷到 payload zpkg 的同目录、同 stem

#### Scenario: 自包含布局随 zpkg 改名
- **WHEN** 自包含 publish 把 zpkg 拷成 `appDir/app.zpkg`
- **THEN** 侧车被拷成 `appDir/app.runtimeconfig.toml`

#### Scenario: 没有侧车不是错误
- **WHEN** 工程没有 `[profile.*]` 运行时旋钮（因而 build 没产侧车）
- **THEN** publish 正常完成，不报错、不产空文件

#### Scenario: 端到端——profile 到达已发布的 app
- **WHEN** 工程写 `[profile.release] gc-trace = true`，`z42 publish` 后直接运行产出的二进制
- **THEN** 该设置生效（stderr 出现 GC trace 行）
- **AND** **NOT** 像此前那样完全无效且无提示

## MODIFIED Requirements

### Requirement: `RuntimeConfig::from_env` 的层数

**Before:** 只有 env + 内置默认（`resolve(get, None)`）——文件层由 `z42vm` 的 `main()` 单独装配，
故一切嵌入方静默丢失 L3/L4。
**After:** env + `Z42_CONFIG` + `Z42_APP_CONFIG` + 默认。CLI 层仍只属于 `z42vm` 二进制。

## Pipeline Steps
- [x] VM 配置解析（`config.rs`）
- [x] apphost（rust stub）
- [x] publish 编排（z42）
- [ ] Lexer / Parser / TypeChecker / IR Codegen — 无关

## IR Mapping
无。
