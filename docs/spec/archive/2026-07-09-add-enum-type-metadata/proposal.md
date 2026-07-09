# Proposal: enum 类型元数据（unify-type-metadata P1-a）

## Why

现状（已查证）:enum 在 z42 里**只是编译期常量**——`Direction.North` → int 0
（`SymbolCollector.EnumConsts/EnumTypes` 名→值映射),**运行时无 enum 类型实体**,enum 就是 int。
成员值**只存在 TSIG**（每 enum: name + 每成员 {name, i64 value}）供 z42c 跨包 `Enum.Member` 解析。
`class_flags bit5` 已为 IsEnum **预留但未用**（bytecode.rs:125 "bit5 = enum, when IsEnum lands"）。
`Type.IsEnum` / `Enum.GetValues` 反射**不存在**。

这是 `unify-type-metadata` initiative 的 **P1-a 第一砖**:把 enum 从"只在 TSIG 的编译期常量"提升为
**TYPE 段里的类型实体** + 反射暴露。一举两得——① 落地 roadmap 0.3.12 的 `IsEnum`（enum 作类型
实体);② enum 成员值有了 TYPE 的家（删 TSIG 的第一步:该字段不再只靠 TSIG）。

## What Changes

- **z42c writer**:enum 类型 emit 进 TYPE 段——`class_flags bit5=enum` + 追加 enum 成员块
  `{member_count:u16, (name_str_idx:u32, value:i64)×count}`。additive zbc format bump。
- **Rust reader**:`read_type` 读 enum flag + 成员块 → `ClassDesc`/`TypeDesc` 加 `is_enum` + 成员表。
- **typeof(EnumType)**:解析到该 enum 的 Type 实体（现在 enum 无 TYPE 条目 → typeof 拿不到）。
- **反射 API**（z42.core）:`Type.IsEnum` + `Std.Enum.GetNames(Type)` / `GetValues(Type)`（MVP 返
  int 值)/ `GetName(Type, i64)`。
- zbc minor bump（additive）+ regen fixtures + 反射单测 + 全 GREEN + 自举不动点。

## Scope（允许改动的文件）

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcFormat.z42` | MODIFY | `ZbcVersion.Minor` bump + `CLASS_FLAG_ENUM` 常量 |
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcWriter.z42` | MODIFY | `BuildType` emit enum flag + 成员块；`InternPoolStrings` 预扫 enum 成员名 |
| `src/compiler/z42c.ir/src/IrModule.z42` | MODIFY | `IrClassDesc` 加 enum 标记 + 成员 {name,value}（若尚无） |
| `src/compiler/z42c.semantics/src/IrGen.z42` | MODIFY | EnumDecl → IrClassDesc(enum)，填成员名+值 |
| `src/compiler/z42c.semantics/src/SymbolCollector.z42` | MODIFY | enum 类型进类符号表（供 typeof 解析），保留常量映射 |
| `src/runtime/src/metadata/zbc_reader.rs` | MODIFY | `ZBC_VERSION_MINOR` bump + `read_type` 读 enum flag + 成员块 |
| `src/runtime/src/metadata/bytecode.rs` | MODIFY | `ClassDesc` 加 enum 成员字段；`CLASS_FLAG_ENUM` 用起来 |
| `src/runtime/src/metadata/types.rs` | MODIFY | `TypeDesc` 加 `is_enum` + `enum_members`（name↔i64） |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | `Type.IsEnum` builtin + enum 值枚举 builtin |
| `src/libraries/z42.core/src/Type.z42` | MODIFY | `IsEnum` 属性 |
| `src/libraries/z42.core/src/Enum.z42` | NEW/MODIFY | `Std.Enum.GetNames/GetValues/GetName` |
| `src/compiler/z42c.project/src/ZpkgWriter.z42` | MODIFY | zpkg minor bump（耦合 zbc）；TSIG enum 块**不动**（P1 仍双份） |
| `src/runtime/src/metadata/zbc_reader.rs`（zpkg 常量） | MODIFY | `ZPKG_VERSION_MINOR` bump |
| `docs/design/runtime/zbc.md` / `zpkg.md` | MODIFY | TYPE enum 块布局 + changelog |
| `.claude/rules/version-bumping.md` | MODIFY | 版本常量表更新 |
| `src/tests/zbc-format/*` + `zpkg-format/*` | MODIFY | regen |
| `src/compiler/z42c.semantics/tests/zbc/zbc_tests.z42` | MODIFY | golden hex 重截（若 empty 受影响则同步） |
| `src/tests/types/enum_reflect.z42` | NEW | enum 反射端到端（IsEnum/GetNames/GetValues） |
| `src/libraries/z42.core/tests/enum_metadata/` | NEW | Enum API [Test] |

**只读引用**:`ExportedTypeExtractor.z42`（enum 提取现状）、`ImportedSymbolLoader.z42`
（EnumConsts/EnumTypes 合并）、`docs/spec/changes/unify-type-metadata/design.md`（D1/D6）。

## Out of Scope

- 强类型 enum 值（enum 值作独立 boxed 类型而非 int）:MVP 返 int;强类型化延后。
- 从 TSIG **删** enum 块:P1 仍保留双份（TSIG 不动),删在 P3。
- `[Flags]` enum 位运算反射、`Enum.Parse`:延后（本 change 只做元数据 + 枚举 API 三件）。

## Open Questions

- [ ] `Enum.GetValues` 返 `int[]` 还是 `object[]`?（design 倾向 MVP `int[]`,与当前 enum=int 一致）
