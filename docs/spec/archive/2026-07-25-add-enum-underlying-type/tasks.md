# Tasks: Type.GetEnumUnderlyingType（enum 底层类型）

> 状态：🟢 已完成 | 创建：2026-07-25 | 完成：2026-07-25 | 分支：feat/enum-underlying-type（隔离 worktree z42-reflinst，User 授权）
> 类型：feat（小；纯 runtime + Std.Type，无格式 bump）

**变更说明：** `Type.GetEnumUnderlyingType()` 返 enum 底层整型。非 enum 抛 Std.Exception（镜像 C#）。

**调查更正（重要）：** 早先判定「需格式 bump 持久化声明底层类型」——**错**。IrGen 的 enum 发射
（IrGen.z42:379+）**完全忽略** `EnumDecl.baseType`：z42 一律以 i64（long）背书 enum（成员值 i64、
`Enum.GetValues` 返 `long[]`、声明的 `: byte` 被丢弃）。故底层类型恒 `long`，返 `typeof(long)` 即
准确、**无需格式 bump**。

**修复（纯 runtime + Std.Type）：**
- `reflection.rs`：`builtin_type_enum_underlying`（IsEnum → `make_type_from_name("long")`；非 enum
  `bail!` → catchable Std.Exception）。
- `corelib/mod.rs`：注册于 BUILTINS 末尾（preserve-BuiltinIds）。
- `Std/Type.z42`：`GetEnumUnderlyingType()` extern。

**文档影响：** `docs/design/language/reflection.md`（标 GetEnumUnderlyingType 落地 + 更正「需 bump」判定）。

- [x] 1.1 `reflection.rs`：builtin_type_enum_underlying
- [x] 1.2 `corelib/mod.rs`：注册于 BUILTINS 末尾
- [x] 1.3 `Std/Type.z42`：GetEnumUnderlyingType()
- [x] 1.4 `src/tests/types/enum_underlying_type.z42`：e2e（enum→long / 非 enum 抛 / 与 GetValues 一致）——interp+jit 空输出 exit0
- [x] 1.5 全绿：BuiltinId 保序 4/0 + types e2e 84/0（enum 无回归）+ stdlib z42.core 44/0
- [x] 1.6 `docs/design/language/reflection.md` 标记 + 更正
- [x] 1.7 归档 + PR

## 备注
- z42c 零改动 + 无格式 bump → 自举 byte-identical。
- 真正尊重声明的 `: byte`（改 enum 值宽度）是语言语义改动 + 需格式 bump，另属 enum 类型系统工作，非本项。
- 剩余：带类型 enum 值（Tier 2，需 boxing）。
