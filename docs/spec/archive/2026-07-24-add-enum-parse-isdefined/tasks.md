# Tasks: Enum.Parse / Enum.IsDefined（enum 反射 Tier 1）

> 状态：🟢 已完成 | 创建：2026-07-24 | 完成：2026-07-24 | 分支：feat/enum-parse-isdefined（隔离 worktree z42-reflinst，User 授权）
> 类型：feat（小；纯 runtime + Std.Enum，读现有 enum_members，无格式 bump）

**变更说明：** 补齐 enum 反射的 `Enum.Parse(type, name) → value`（GetName 的逆，未命中抛 Std.Exception，
大小写敏感）+ `Enum.IsDefined(type, value) → bool`。

**背景更正：** enum 类型实体 + IsEnum + GetNames/GetValues/GetName 早由 add-enum-type-metadata
（2026-07-09）落地（reflection.md 的 isenum Deferred 条目陈旧）。本 change 只在**现有**
`TypeDesc.enum_members` 元数据上补 Parse/IsDefined，**无格式 bump、无 compiler 改动**。

**修复：**
- `reflection.rs`：`builtin_enum_parse`（名线性查 enum_members 返值；未命中 `bail!` → catchable
  Std.Exception，镜像 C# Enum.Parse）+ `builtin_enum_is_defined`（值线性查）。
- `corelib/mod.rs`：注册两 builtin 于 **BUILTINS 末尾**（preserve-BuiltinIds，同 repl builtin 先例）。
- `Std/Enum.z42`：`Parse` / `IsDefined` extern 方法。

**Open Question 裁决（本 change 采用）：** Parse 抛异常（C# 语义）；大小写敏感（z42 大小写敏感语言，
无 ignoreCase）；GetEnumUnderlyingType 延后（需格式 bump 持久化 underlying type）；Tier 2（带类型
enum 值）延后（需 boxing）。

**文档影响：** `docs/design/language/reflection.md`（更正 isenum 陈旧条目 + 标 Parse/IsDefined 落地）。

- [x] 1.1 `reflection.rs`：builtin_enum_parse + builtin_enum_is_defined
- [x] 1.2 `corelib/mod.rs`：注册于 BUILTINS 末尾（preserve-BuiltinIds）
- [x] 1.3 `Std/Enum.z42`：Parse + IsDefined
- [x] 1.4 `src/tests/types/enum_parse.z42`：e2e（Parse 命中/抛未知/大小写敏感抛/IsDefined/round-trip）——interp+jit 空输出 exit0
- [x] 1.5 全绿：BuiltinId 保序 cargo 单测 4/0 + types e2e 82/0（enum_reflect 无回归）+ stdlib z42.core 44/0
- [x] 1.6 `docs/design/language/reflection.md` 更正 + 标记
- [x] 1.7 归档 + PR

## 备注
- z42c 零改动 + 无格式 bump → 自举 trivially byte-identical（跳过 test compiler）。
- 新 stdlib API（Enum.Parse/IsDefined）：z42c/xtask 源不用它 → 无 seed-boundary（axis ③）问题。
- 剩余：GetEnumUnderlyingType（格式 bump）/ 带类型 enum 值（boxing）/ TryParse（out 参）延后。
