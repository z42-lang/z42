# Proposal: 参数元数据（unify-type-metadata P1-d）

## Why

反射 `ParameterInfo` 当前只有 `Name`（且**从 DBUG 局部变量表猜**——无 debug 符号时退化为
`arg0/arg1`）、`Position`、`ParameterType`。缺 `IsOptional`（可选参数）、`IsParams`（varargs）、
`DefaultValue`（默认值）。这些参数元数据 z42c 已为跨包 TSIG 计算（`_requiredCount` / varargs
`params_from`），但**没进 SIGS**（运行期反射拿不到）。

unify-type-metadata 目标是「运行期反射元数据 = 单一真相」。P1-d 把参数元数据持久化进 SIGS，让
`ParameterInfo` 权威化。第四砖。

## What Changes

SIGS 每函数 / 每参数追加（**非 gated**）：

- **`min_arg:u16`**（函数级，紧接 `method_flags` 后）：必填参数个数 → `ParameterInfo.IsOptional`
  （`Position >= min_arg` 即可选；C# 语义可选参数恒尾随）。
- **`params_from:u8`**（函数级，`0xFF`=无 varargs）：`params` 参数起始 index → `ParameterInfo.IsParams`。
- **`name_str_idx:u32`**（每参数，与 param_type 同块）：参数源名 → `ParameterInfo.Name` **权威化**
  （取代 DBUG 猜测；无 debug 符号也准）。
- **`default_kind:u8 + payload`（每参数）**：默认值的**类型化常量** → `ParameterInfo.DefaultValue`。
  kind：0=无 / 1=null / 2=i64（int/long/char，8B）/ 3=f64（float/double，8B）/ 4=bool（1B）/
  5=str（u32 str_idx）。IrGen 从 `Param.Default` **字面量**折出（IntLit/FloatLit/BoolLit/StringLit/
  CharLit/null）；非字面量默认值（常量表达式/enum 成员）→ kind=0（DefaultValue 不可得，但 IsOptional
  仍由 min_arg 表达）。

反射：`ParameterInfo` 加 `IsOptional` / `IsParams` / `DefaultValue`；`Name` 改从 SIGS name_str_idx
（DBUG 回退保留）。

**格式 bump**：zbc 1.24→1.25 / zpkg 0.28→0.29（两代自举，同 P1-b/c）。

## Out of Scope

- **非字面量默认值的值**（`int x = SOME_CONST` / enum 成员 / `1+2` 等常量表达式）：DefaultValue 返回
  null（kind=0），但该参数 `IsOptional` 仍 true。覆盖字面量默认值（绝大多数）即达 MVP；常量折叠默认值
  留 follow-up（记 reflection.md Deferred）。
- 委托元数据 / 跨包 impl 反射（P1-e）；删 TSIG（P3）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.ir/src/IrModule.z42` | MODIFY | `IrFunction` + `MinArg:int` / `ParamsFrom:int`（0xFF=无）/ `ParamNames:string[]` |
| `src/compiler/z42c.semantics/src/IrGen.z42` | MODIFY | 三发射分支（normal/extern/abstract）填 MinArg（Default==null 计数）/ ParamsFrom（末参 IsParams）/ ParamNames |
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcFormat.z42` | MODIFY | `ZbcVersion.Minor` 24→25 |
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcWriter.z42` | MODIFY | WriteSigEntries 写 min_arg/params_from（method_flags 后）+ 每参 name_str_idx |
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcReader.z42` | MODIFY | 读侧对称：min_arg/params_from/param name |
| `src/compiler/z42c.project/src/ZpkgReader.z42` | MODIFY | 读侧对称：ReadModuleSigs 消费 min_arg/params_from/param name |
| `src/compiler/z42c.project/src/ZpkgWriter.z42` | MODIFY | `ZpkgWriterZ.Minor` 28→29 |
| `src/runtime/src/metadata/bytecode.rs` | MODIFY | `Function` + `min_arg:u16`/`params_from:u8`/`param_names:Box<[String]>`（或复用现有 param 名路径） |
| `src/runtime/src/metadata/zbc_reader.rs` | MODIFY | read_sigs 读 min_arg/params_from/param name；Function 灌入；bump 常量 25/29 |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | resolve_func_sig 返回 min_arg/params_from/names；build ParameterInfo 塞 IsOptional/IsParams + Name 权威 |
| `src/libraries/z42.core/src/Reflection/ParameterInfo.z42` | MODIFY | `IsOptional` / `IsParams` / `DefaultValue` |
| `docs/design/language/reflection.md` | MODIFY | 参数元数据反射节 + ParameterInfo 成员表 + DefaultValue Deferred |
| `docs/design/runtime/zbc.md` / `zpkg.md` / `.claude/rules/version-bumping.md` | MODIFY | changelog + 常量表 |
| `docs/roadmap.md` | MODIFY | Deferred Backlog Index 加 `add-param-default-values` |
| `src/tests/zbc-format/*` + `zpkg-format/*` | MODIFY | regen |
| `src/compiler/z42c.semantics/tests/zbc/zbc_tests.z42` / `z42c.project/tests/zpkg/zpkg_tests.z42` | MODIFY | golden hex + header pin |
| `src/runtime/src/metadata/zbc_reader_tests.rs` | MODIFY | pinned 版本常量 25/29 |
| `src/runtime/src/**/*_tests.rs`（Function 字面量） | MODIFY | 补 min_arg/params_from/param_names 字段 |
| `src/libraries/z42.core/tests/reflection.z42` | MODIFY | IsOptional/IsParams/Name [Test] |

**只读引用**：
- `src/compiler/z42c.syntax/src/Decl.z42` — `Param.{Name,Default,IsParams}` 源
- `src/compiler/z42c.semantics/src/ExportedTypeExtractor.z42` — 复用 `_requiredCount`/params_from 计算口径
- `docs/spec/archive/2026-07-10-add-method-modifiers/` — P1-c 同款范式（三发射分支/两代自举/读侧对称）

## Open Questions

- [ ] `min_arg` vs per-param `has_default:bit`：本砖用 `min_arg:u16`（函数级，够表达 IsOptional）；
      per-param `has_default` 留 P1-d2 与 default_const 一起（届时 IsOptional 可改读 per-param bit）。倾向 min_arg。
- [ ] 参数名 str_idx 与现有 DBUG 猜测的关系：SIGS name 优先，DBUG 作回退（倾向）。
