# Spec: Native 依赖声明面 + build-hook 产出 + 传递复制

## ADDED Requirements

### Requirement: `[native.<name>]` manifest 段解析

#### Scenario: 单个 native 声明
- **WHEN** manifest 含 `[native.z42_repl]`（可空表）
- **THEN** `ManifestLoader.ParseText` 产出的 `ProjectManifest.Natives` 含一项 `NativeSpec { Name = "z42_repl" }`，
  `NativeCount == 1`

#### Scenario: 多个 native 声明
- **WHEN** manifest 含 `[native.a]` 与 `[native.b]`
- **THEN** `Natives` 含两项，名分别为 `a`/`b`，按名稳定序（common-pitfalls §1）

#### Scenario: 无 `[native]` 段
- **WHEN** manifest 无 `[native]` 段
- **THEN** `Natives` 为空数组，`NativeCount == 0`（不报错）

### Requirement: `BuildHooks.ProvideNative` 相位

#### Scenario: 默认 no-op
- **WHEN** 项目 hooks 未 override `ProvideNative`
- **THEN** 调用它无副作用、不抛异常

#### Scenario: z42.repl 产 native
- **WHEN** 对 z42.repl 的 `ProjectHooks` 调 `ProvideNative(ctx)`（`ctx.Target.Rid` = 目标 rid）
- **THEN** 现场 `cargo build -p z42-repl` 成功后，`libz42_repl.<平台后缀>` 出现在 `ctx.Dirs.Dist/<rid>/`，
  且经 `ctx.AddOutput("native", <该路径>)` 登记

#### Scenario: cargo 缺失/失败
- **WHEN** `cargo` 不可用或构建失败
- **THEN** hook `ctx.Warn` 报错并**不**登记产物（与 xtask hook 一致的 best-effort 语义），不使整个 publish 崩溃

### Requirement: publish 期传递复制 native（`_pubBundleProjectNativeDeps`）

#### Scenario: 消费者拷传递 native
- **WHEN** publish 一个 exe（z42.interactive），其 path-dep 闭包含声明 `[native.z42_repl]` 的 dep（z42.repl）
- **THEN** 该 dep 的 `ProvideNative` 被跑，目标 rid 的 `libz42_repl.<suffix>` 被平铺进消费者 payload
  （`programs/z42i/libz42_repl.<suffix>`，无 rid 子目录）

#### Scenario: 目标 rid 选择
- **WHEN** publish `--rid <R>`
- **THEN** 只复制 `<dep-dist>/<R>/lib<name>.<suffix(R)>` 那一份；后缀按 R 的平台族派生

#### Scenario: 无 native 依赖
- **WHEN** 消费者闭包内无任何 `[native]` 声明
- **THEN** `_pubBundleProjectNativeDeps` 返回 0、payload 无额外文件（保持现有行为）

### Requirement: z42.repl 独立自包含

#### Scenario: 单独构建 z42.repl 携带 native
- **WHEN** 单独 build/publish z42.repl（`[build] hooks` 生效）
- **THEN** z42.repl 的 `dist/<rid>/libz42_repl.<suffix>` 存在 —— z42.repl 的 dist 自包含（zpkg + native），
  可被任意项目按 `[native]` 约定引用

## MODIFIED Requirements

### Requirement: SDK packaging 落地 z42i 的 native

**Before:** packaging 用 `_pkgStageReplCdylib` **硬编码**把 `libz42_repl.{dylib,so,dll}` 从共享 cargoOut 拷进
`programs/z42i/`；`_pkgBuildAndStageRuntime` 里独立 `cargo build -p z42-repl`；`_copyNativeLibs` 显式排除 repl。

**After:** `z42 publish` 经 `_pubBundleProjectNativeDeps` 已把 native 落进 z42i publish 输出；`[component.z42i]`
整目录拷 publish 即带上 native。`_pkgStageReplCdylib`、`_pkgBuildAndStageRuntime` 的 `cargo build -p z42-repl`、
`_copyNativeLibs` 的 repl 排除**全部删除**。运行期 `resolve_native_beside(programs/z42i/, "z42_repl")` 不变。

## Pipeline Steps

- [ ] Lexer —— 不涉及
- [ ] Parser / AST —— 不涉及（TOML 段，非语言语法）
- [ ] TypeChecker —— 不涉及
- [ ] IR Codegen —— 不涉及（本 change 不碰 z42c codegen；自举字节应不动）
- [x] Manifest 解析（z42.project stdlib）—— `_parseNative`
- [x] 构建/发布管线（z42.build 相位 + z42b publish）—— `ProvideNative` + `_pubBundleProjectNativeDeps`
- [x] Packaging（xtask）—— 删特殊处理
