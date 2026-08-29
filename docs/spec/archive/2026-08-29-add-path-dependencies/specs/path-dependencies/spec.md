# Spec: 本地路径依赖（path dependencies）

## ADDED Requirements

### Requirement: `[dependencies]` 支持 path 表形式

`[dependencies]` 的每一项，值可为字符串（版本）或表 `{ version?, path? }`。含 `path` 者为**本地路径依赖**：依赖工程的源位于 `path`（相对本 manifest 所在目录）。

#### Scenario: 表形式 path 依赖解析
- **WHEN** manifest 含 `"z42.repl" = { path = "../repl" }`
- **THEN** 解析出 `DepEntry{ Name="z42.repl", Version="", Path="../repl" }`

#### Scenario: 表形式带 version + path
- **WHEN** `"foo" = { version = "0.1.0", path = "../foo" }`
- **THEN** `DepEntry{ Name="foo", Version="0.1.0", Path="../foo" }`

#### Scenario: 纯字符串依赖回落（无 path）
- **WHEN** `"z42.core" = "0.1.0"`
- **THEN** `DepEntry{ Name="z42.core", Version="0.1.0", Path="" }`（Path 空 = 名字依赖，走 Z42_LIBS）

### Requirement: 单工程 build 先建 path 依赖闭包

`z42c build <toml>` 编译消费方前，按拓扑序（叶子在前）构建全部传递 path 依赖，各建进依赖工程自己的 dist，并把这些 dist 并入消费方的 libsDirs。

#### Scenario: 自动先建直接 path 依赖
- **WHEN** `bar.toml` 含 `"foo" = { path = "../foo" }`，执行 `z42c build bar.toml`
- **THEN** 先编译 `../foo` 产出 `foo.zpkg`（进 foo 自己的 dist），再编译 bar；bar 对 foo 的符号解析成功（非 `<error>` / undefined）

#### Scenario: 传递 path 依赖
- **WHEN** `bar → { path=../foo }`，`foo → { path=../baz }`
- **THEN** 构建序为 baz → foo → bar（叶子在前）

#### Scenario: path 依赖成环
- **WHEN** `a → { path=../b }` 且 `b → { path=../a }`
- **THEN** 报错并中止（`path dependency cycle`），不进入编译

#### Scenario: path 指向的目录无 manifest
- **WHEN** `path` 解析出的目录不含恰好一份 `*.z42.toml`
- **THEN** 报明确错误（缺失 / 多份），中止

### Requirement: 打包判据按真-stdlib（path 依赖 = 私有组件）

打包"是否复制进消费方产物"的判据 = **依赖是否真属标准库**（在 `src/libraries/<name>/` 或已在 shipped libs），而非名字前缀。path 依赖一律视为私有组件、复制（colocate）进消费方产物目录；真 stdlib 不复制、运行期走 Z42_LIBS。`_bundleExeDeps`（z42c）与 `_pubBundleProjectDeps`（z42b）判据一致。

#### Scenario: path 依赖 colocate 进 payload
- **WHEN** `z42.interactive`（exe）含 `"z42.repl" = { path = "../repl" }`，build + publish 完成
- **THEN** `z42.repl.zpkg` 复制进 z42.interactive.zpkg 所在 payload 目录；运行期由 `app.rs` entry-dir search 从同目录解析（不依赖 SDK libs 里有 z42.repl）

#### Scenario: 真 stdlib 依赖不复制
- **WHEN** 同一 exe 含 `"z42.core" = "0.1.0"`（在 `src/libraries/`）
- **THEN** `z42.core.zpkg` 不复制进产物（运行期走 Z42_LIBS）—— z42.project 等真 stdlib 同理

#### Scenario: 名字带 z42. 但非 stdlib 的 path 依赖仍私有
- **WHEN** 依赖名 `z42.repl` 但不在 `src/libraries/`、经 path 声明
- **THEN** 判为私有组件、复制进消费方（不因 `z42.` 前缀被误当 stdlib 跳过）

### Requirement: 被依赖工程 dist_dir 决定产物落点

path 依赖的产物落点由**被依赖工程自己的 `[build].dist_dir`** 决定；未声明则默认 `<projDir>/dist`。消费方 build 把该 dist 并入 libsDirs 供构建期解析，并按上一 Requirement 判据 colocate 进产物。

#### Scenario: z42.repl 建进自己的 dist
- **WHEN** `z42.repl` 经 path 依赖构建（未声明 dist_dir）
- **THEN** `z42.repl.zpkg` 落 `src/toolchain/interactive/repl/dist/`；消费方将其 colocate 进 z42i payload

> **打包合并（single-file）不在本 spec**：本 change 只做 colocated 分离 zpkg。"合成一个文件"由正交的 single-file（内嵌分离 zpkg 进 apphost）承担，独立 follow-up；single-zpkg 托管合并对标 .NET 砍掉。见 design D7/D8 + 部署模型 book 页。

### Requirement: 非标准库 native 依赖 colocate 在 zpkg 旁、运行期按名平铺解析（Supersedes #332）

非标准库 native 库（如 `libz42_repl`）跟随所属组件 colocate 在消费方 zpkg 目录旁；运行期用共享 resolver `resolve_native_beside(zpkg_dir, lib_name)` 按名在该目录平铺查找（唯一布局 `<zpkg-dir>/lib<name>.<suffix>`），不盲扫、无 rid 子目录。标准库 native（`<sdk>/native/` eager + `[Native(lib=)]`）不变。

#### Scenario: repl native 库归位 programs/z42i 并被解析
- **WHEN** SDK 打包完成，`libz42_repl.<suffix>` 位于 `<sdk>/programs/z42i/`（beside `z42.interactive.zpkg`）
- **THEN** REPL 启动经 `resolve_native_beside(<sdk>/programs/z42i/, "repl")` 定位到该库并 dlopen 成功（不再从共享 `bin/` 找）

#### Scenario: 按名定向不盲扫（消除 spurious WARN）
- **WHEN** 通用 z42vm 加载 interactive，目录含 `libz42_repl` 等非标准库 native
- **THEN** 只按名解析被声明需要的库（单一路径 stat），不 dlopen 目录里所有 `libz42_*` → 无 `ignoring unknown lib repl` WARN（golden 不被污染）

#### Scenario: 标准库 native 仍走 eager `<sdk>/native/`
- **WHEN** 代码用标准库 native（`[Native(lib=...)]`，如 compression）
- **THEN** 仍由 `<sdk>/native/` eager 扫 + 注册解析，行为不变（本 change 不碰）

> **`[native.dependencies]` app 声明面不在本 spec**：当前唯一非标准库 native 是 repl（由 packaging 直接 colocate）；本 change 铺 runtime resolver + publish 复制骨架，app 经 manifest 声明 native 依赖 + 发布期按 rid 拍平复制 → Deferred（见 design D9 Deferred）。

## Pipeline Steps

受影响阶段（path 依赖为构建编排层，不触及 lex/parse/typecheck 语义）：
- [ ] Lexer — 无
- [ ] Parser / AST — 无
- [ ] TypeChecker — 无
- [x] Manifest 解析（`z42.project`）— DepEntry.Path
- [x] Build 编排（driver `_build` + PathDepPlan）— 闭包构建 + libsDirs
- [x] 打包（`_bundleExeDeps`）— 私有组件复制

## IR Mapping

无。path 依赖不产生新 IR / zbc / zpkg 格式；产物中依赖仍以名字（`declaredDeps` / DEPS zpkg basename）表达。
