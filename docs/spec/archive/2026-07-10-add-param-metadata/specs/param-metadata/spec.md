# Spec: 参数元数据反射

## ADDED Requirements

### Requirement: SIGS 持久化参数元数据

#### Scenario: 必填参数计数
- **WHEN** z42c 编译 `void F(int a, int b = 2, int c = 3)`
- **THEN** SIGS 该函数 `min_arg == 1`（仅 a 必填；b/c 有默认值）

#### Scenario: 无可选参数
- **WHEN** z42c 编译 `void F(int a, int b)`
- **THEN** `min_arg == 2`（全必填）

#### Scenario: varargs
- **WHEN** z42c 编译 `void F(int a, params int[] rest)`
- **THEN** `params_from == 1`（rest 的逻辑 index）

#### Scenario: 无 varargs
- **WHEN** z42c 编译 `void F(int a, int b)`
- **THEN** `params_from == 0xFF`

#### Scenario: 参数名
- **WHEN** z42c 编译 `void F(int width, int height)`
- **THEN** SIGS 每参 `name_str_idx` 指向 `"width"` / `"height"`

### Requirement: ParameterInfo 暴露 IsOptional / IsParams / Name（权威）

#### Scenario: IsOptional
- **WHEN** 反射 `GetParameters()` 取到 pos >= min_arg 的参数
- **THEN** `ParameterInfo.IsOptional == true`；pos < min_arg 则 false

#### Scenario: IsParams
- **WHEN** 反射取到 varargs 位（pos == params_from）的参数
- **THEN** `ParameterInfo.IsParams == true`；其它 false

#### Scenario: Name 权威（无 debug 符号也准）
- **WHEN** 反射取参数（即便无 DBUG 局部变量表）
- **THEN** `ParameterInfo.Name` == 源码参数名（来自 SIGS name_str_idx，非 `arg{n}` 退化）

## MODIFIED Requirements

**Before:** `ParameterInfo.Name` 从 DBUG 局部变量表猜（无 debug → `arg0/arg1`）；无 IsOptional/IsParams。
**After:** `Name` 源自 SIGS `name_str_idx`（DBUG 作回退）；新增 `IsOptional`（min_arg 派生）/
`IsParams`（params_from 派生）。

## IR Mapping

- SIGS 每函数追加 `min_arg:u16` + `params_from:u8`（紧接 `method_flags`）；每参数在 `param_type` 后
  追加 `name_str_idx:u32`。无新 opcode / IR 指令。

## Pipeline Steps
- [ ] Lexer / Parser — 无（`Param.{Name,Default,IsParams}` 已有）
- [ ] TypeChecker — 无
- [x] IR Codegen — `IrFunction.{MinArg,ParamsFrom,ParamNames}` + IrGen 三分支填 + ZbcWriter/ZbcReader/ZpkgReader
- [x] VM interp — `Function.{min_arg,params_from,param_names}` + reflection ParameterInfo
