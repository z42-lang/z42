# Design: 参数元数据（unify-type-metadata P1-d）

## Architecture

```
AST Param{Name,Default,IsParams}                    Rust 运行期
   │ IrGen（三发射分支）                              Function.min_arg / params_from / param_names ─┐
   ▼                                                                                              │
IrFunction.MinArg/ParamsFrom/ParamNames ──ZbcWriter──► SIGS +min_arg:u16 +params_from:u8         │
   （min_arg=Default==null 计数；ParamsFrom=末参 IsParams?idx:0xFF）  每参 +name_str_idx:u32       │
                                        │ read_sigs                                               │
   ZbcReader/ZpkgReader ◄── 读侧对称     ▼                                                          │
                              build_method_info → ParameterInfo.{Name(权威)/IsOptional/IsParams} ◄─┘
```

SIGS 每函数条目布局（P1-d 后）：
`name / param_count / ret_tag / ret_idx / exec / is_static / visibility / method_flags /
 **min_arg:u16 / params_from:u8** / (param_type:u32, **name_str_idx:u32**)×param_count / tp / attrs / param-attrs`

## Decisions

### Decision 1: min_arg（函数级）表达 IsOptional，不做 per-param has_default
**问题**：IsOptional 用函数级 `min_arg:u16`（必填数）还是 per-param `has_default:bit`？
**决定**：本砖用 **`min_arg:u16`**。可选参数 C# 语义**恒尾随** → `IsOptional(pos) = pos >= min_arg`
足够。per-param `has_default:bit` 留 P1-d2——届时它与 `default_const`（DefaultValue 的值）**一起加**
才有意义（只加 bit 不加值，信息量不超过 min_arg）。

### Decision 2: params_from:u8（0xFF=无）表达 varargs
z42c `add-params-varargs` 已有 `params_from` 概念（TSIG 里）。SIGS 加 `params_from:u8`：末参是
`params T[]` 时 = 其 index，否则 `0xFF`。`ParameterInfo.IsParams(pos) = (params_from != 0xFF && pos == params_from)`。

### Decision 3: 参数名进 SIGS（name_str_idx），DBUG 作回退
参数名此前反射从 DBUG 局部变量表猜（`reflection-future-parameter-names`），无 debug 符号退化
`arg{n}`。SIGS 每参加 `name_str_idx:u32` → 权威源名。reflection `build_method_info` 优先 SIGS name，
DBUG 猜测作回退（兼容旧 zbc 无此字段的路径——实则 strict-pin 不会遇到，但代码简洁）。

### Decision 4: DefaultValue 值编码（本砖含，字面量折叠；User 裁决「一起做」）
默认值的**值**（`ParameterInfo.DefaultValue`）：SIGS 每参 name_str_idx 后加 `default_kind:u8 + payload`。
- **kind**：0=无 / 1=null / 2=i64（int/long/char，8B LE）/ 3=f64（float/double，8B IEEE754 LE）/
  4=bool（1B）/ 5=str（u32 str_idx，需预扫 intern）。
- **IrGen 折叠**：`Param.Default` 是 `Expr`；对**字面量**直取——`IntLitExpr.Value`（parse i64）/
  `FloatLitExpr.Value`（parse f64）/ `BoolLitExpr.Value` / `StringLitExpr`（unescape 后入池）/
  `CharLitExpr.Value`（i64）/ `NullExpr`（kind=1）。**非字面量**（常量表达式/enum 成员/命名常量）→
  kind=0（DefaultValue=null，但 IsOptional 仍 true）。覆盖字面量默认值即 MVP；常量折叠留 follow-up。
- **反射**：kind → `ParameterInfo.DefaultValue`（Value::{Null,I64,F64,Bool,Str}）；kind=0 → null。
- **IrFunction 侧**：`ParamDefaultKinds:int[]` + `ParamDefaultI64:long[]` + `ParamDefaultF64:double[]`
  + `ParamDefaultStr:string[]`（并行数组按参数 index；z42 无 union → 分列存，writer 按 kind 取）。

### Decision 5: 非 gated + 两代自举（同 P1-b/c，纪律已固化）
每函数 +3 字节（min_arg u16 + params_from u8）+ 每参 +4 字节（name_str_idx）→ 非 gated。
writer + 全 4 reader（Rust zbc_reader + z42c ZbcReader + z42c ZpkgReader）同提交对称改。
zbc 1.24→1.25 / zpkg 0.28→0.29；两代自举 0.28→0.29，gen1-stdlib EMPTY Z42_LIBS。
**三发射分支全覆盖**（normal/extern/abstract）——P1-c extern 漏设 MethodFlags 的教训。

## Implementation Notes

- `IrFunction.MinArg:int`（默认 = ParamCount，即全必填）/ `ParamsFrom:int`（默认 0xFF）/
  `ParamNames:string[]`（含 this？——**不含 this**，与逻辑参数对齐；SIGS param 块含 this 在 index 0，
  name 用 `"this"` 占位或空——待实施定，倾向 this 名 = `"this"`）。
- IrGen 计算：`MinArg` = `Params` 中 `Default==null` 的个数（尾随可选假设）；`ParamsFrom` = 末参
  `IsParams` ? 其逻辑 index（+this offset）: 0xFF；`ParamNames[i]` = `Params[i].Name`。
- **this 参数**：SIGS param_count 含 this（index 0）。name_str_idx[0] = `"this"`；min_arg/params_from
  以**含 this 的 index** 口径还是逻辑口径？——倾向逻辑口径（不含 this），反射侧 `start = is_static?0:1`
  已处理 this skip，IsOptional/IsParams 的 pos 是逻辑 position（0-based，不含 this），min_arg/params_from
  也用逻辑口径，一致。实施时对齐 `build_method_info` 的 `pos` 计算。
- Rust：`Function.min_arg:u16` / `params_from:u8` / `param_names:Box<[String]>`（reader 灌入，同
  is_static/visibility/method_flags 路径）。注意现有 `resolve_func_sig` 已从 DBUG 取 param_names——
  改为优先 SIGS names。

## Testing Strategy

- 单元：z42c golden hex（empty/f5 再增字节——Main/F 无参 → min_arg=0/params_from=0xFF/无 name；
  但**函数级 min_arg+params_from 恒 +3 字节** → golden 变）；Rust read 往返 + pinned 25/29。
- 反射 [Test]：`reflection.z42` 加方法（必填+可选+params+命名参数）→ 断言 IsOptional/IsParams/Name。
- VM 验证：xtask test 全 stage + 自举不动点 7/7 + cargo lib + zbc_compat + lazy_loader（fixtures regen）。

## Deferred / Future Work

### fold-nonliteral-param-defaults：非字面量默认值的值
- **来源**：本砖 Out of Scope（DefaultValue 只折字面量）。
- **触发原因**：常量表达式/enum 成员/命名常量默认值需常量折叠器，本砖只直取字面量。
- **前置依赖**：本砖（字面量 DefaultValue + kind 编码已落，扩展只加 kind 分支）。
- **触发条件**：反射需读非字面量默认值的值时。
- **当前 workaround**：非字面量默认值 `DefaultValue=null`（kind=0），但 `IsOptional` 仍 true。
