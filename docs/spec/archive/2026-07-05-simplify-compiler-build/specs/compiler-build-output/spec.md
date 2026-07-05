# Spec: 编译器构建产物 —— 自包含 exe + 干净布局

## ADDED Requirements

### Requirement: exe build 复制非标准库依赖到输出

#### Scenario: exe 项目产物自包含
- **WHEN** `z42c build <exe.z42.toml>`，该 exe 声明依赖 `{某本地 lib A, 某 stdlib z42.io}`
- **THEN** 输出 dist 目录含 `<exe>.zpkg` **和** `A.zpkg`（本地/非 stdlib 依赖），**不含** `z42.io.zpkg`（stdlib）

#### Scenario: lib 项目产物保持干净
- **WHEN** `z42c build <lib.z42.toml>`（kind=lib）
- **THEN** 输出 dist 目录**只有** `<lib>.zpkg`（+ `.zsym`），不复制任何依赖

#### Scenario: 自包含 exe 可就地运行
- **WHEN** exe 的 dist 含其非 stdlib 依赖，`z42vm <dist>/<exe>.zpkg` 运行，`Z42_LIBS=<stdlib dist>`
- **THEN** z42vm 从 entry 目录（dist）解析本地依赖、从 `Z42_LIBS` 解析 stdlib，成功运行（依赖 `src/runtime/src/main.rs` 既有 `search_dirs = [entry dir, libs]`）

### Requirement: 编译器自建经 `z42c build --workspace`，无 scratch 目录

#### Scenario: 自建产物落各 member dist，无拼接目录
- **WHEN** `xtask build compiler`
- **THEN** 7 个 member 产物落 `artifacts/build/compiler/<member>/<profile>/dist/`；**不存在** `selfbuild-runlibs/` 或 `dogfood/` 目录

#### Scenario: 自举字节不动点保持
- **WHEN** `xtask test compiler`（--workspace 自建后）
- **THEN** 7/7 zpkg present + gen 逐字节 identical（不动点绿）

### Requirement: 编 stdlib 直接跑自包含 driver

#### Scenario: 无 dogfood 拼接
- **WHEN** `xtask build stdlib`
- **THEN** 跑 `artifacts/build/compiler/z42c.driver/<profile>/dist/z42c.driver.zpkg`（已自带兄弟包），`Z42_LIBS=<stdlib dist>`；22 库全建成；**不存在** `dogfood/`

## MODIFIED Requirements

### Requirement: 编译器产物目录名

**Before:** `artifacts/build/z42c/<member>/<profile>/dist/`
**After:** `artifacts/build/compiler/<member>/<profile>/dist/`（镜像 `src/compiler/`，与 `libraries/` 一致）

### Requirement: xtask 种子/工具链 env

**Before:** 种子 z42c 目录可经 `Z42C_DIR` 显式覆盖（本会话新加）
**After:** 删 `Z42C_DIR`；种子 z42c 一律从 SDK 根（`Z42_TOOLCHAIN`/`Z42_HOME`/apphost/`./.z42`）的 `programs/z42c` 派生。不新增 env。

## Pipeline Steps

受影响（本变更主要是构建编排 + 产物组装，非语言前端）：
- [x] z42c.project 产物组装（ZpkgBuilder：exe 复制依赖）
- [ ] Lexer / Parser / TypeChecker / IR Codegen — 不涉及
- [x] xtask 构建编排（compiler/stdlib self-build）
- [x] 运行时依赖解析 — 只读依赖既有 `search_dirs`，不改
