# Spec: 编译期函数内联 + 可独立开关的 OptSet

## ADDED Requirements

### Requirement: OptSet 决定启用哪些优化（可独立开关）

#### Scenario: debug 默认 None、release 默认 All
- **WHEN** `z42c build <toml>`（无 `--release`、无覆盖）
- **THEN** OptSet = None，`IrOptPipeline` 所有 pass 都跳过，产物未优化、忠实可调试
- **WHEN** 加 `--release`
- **THEN** OptSet = All（const-fold/copy-prop/dce/algebraic/inline 全跑）

#### Scenario: 用户自助勾选单个优化
- **WHEN** `z42c build <toml> --opt inline`（debug，profile 默认 None）
- **THEN** 只启用 `Inline`，其余优化不跑（可独立开启单个优化）

#### Scenario: 从 release 默认减去某优化
- **WHEN** `z42c build <toml> --release --no-opt inline`
- **THEN** OptSet = All − Inline（跑其余优化但不内联，用于调试 release 但不想被内联干扰）

#### Scenario: toml 逐项 + CLI 覆盖
- **WHEN** toml `[optimize] inline=true`，debug 构建
- **THEN** OptSet 含 Inline（toml 覆盖 debug 默认）
- **WHEN** 再加 `--no-opt inline`
- **THEN** 不含 Inline（CLI > toml）

#### Scenario: 未知优化名报错
- **WHEN** `--opt frobnicate`（或 toml 写未知项）
- **THEN** 明确报错退出（非静默忽略）

### Requirement: 优化 pass 独立性（正确性不互相依赖）

#### Scenario: 任一 pass 单独开启都正确
- **WHEN** 只启用某一个优化（`--opt <X>`，其余全关），对同一程序编译执行
- **THEN** 执行结果与不优化时**逐字节一致**（每个 pass 自洽，不依赖其它 pass 先跑）

### Requirement: 函数内联正确性与资格

#### Scenario: 小函数直接调用被内联
- **WHEN** OptSet 含 Inline，caller 直接调用一个 ≤24 IR 的同模块非递归函数
- **THEN** 调用点被 callee 体取代（无 `Call`），执行结果逐字节一致

#### Scenario: 单调用点恒内联
- **WHEN** 一个函数全模块仅被调用一次
- **THEN** 即使超过体积阈值也被内联

#### Scenario: 递归 / VCall / 跨包 / 异常表 / ref-out 不内联（v1）
- **WHEN** callee 递归、或调用是 `VCall`、或 callee 跨包/含异常表/有 ref·out 参数/是闭包
- **THEN** v1 跳过，保留原调用

#### Scenario: 内联保留类型信息
- **WHEN** 内联一个操作 I64 的 callee
- **THEN** caller `reg_types` 扩展含 callee 类型 → 内联后 JIT i64 特化 / typed CmpBr 仍生效

## MODIFIED Requirements

### Requirement: IrOptPipeline 门控
**Before:** `IrOptPipeline.Run(irm)` 无条件跑所有 pass。
**After:** `IrOptPipeline.Run(irm, optSet)`；逐 pass `if Has(optSet, X) then run X`，固定稳妥顺序，只跑被勾选的。

## IR Mapping
- 内联不新增 IR 指令 / 不改 zbc·zpkg 格式（纯 IR→IR）。消除 `Call`，产出 callee 展开体。

## Pipeline Steps
- [ ] Lexer / Parser / TypeChecker — 无
- [x] IR Codegen — `IrOptPipeline` 逐 pass 门控 + 新 `IrInline`；`IrGen`/`PackageCompile`/`Z42cCompiler` 透传 optSet
- [x] 配置面 — `Main.z42` CLI `--opt`/`--no-opt`；ProjectManifest toml `[optimize]`
- [x] VM interp / JIT — 无改动
