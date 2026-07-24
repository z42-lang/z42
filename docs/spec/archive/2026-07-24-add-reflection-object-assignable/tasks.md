# Tasks: object 通用可赋值性（object.IsAssignableFrom）

> 状态：🟢 已完成 | 创建：2026-07-24 | 完成：2026-07-24 | 分支：feat/reflection-object-assignable（隔离 worktree z42-reflinst，User 授权）
> 类型：feat（小；纯 runtime，闭 add-reflection-assignable-from 的装箱语义剩余项）

**变更说明：** `typeof(object).IsAssignableFrom(t)` 此前对值类型/用户类/接口/数组恒 false，仅对
object 自身 true。现对任意非 null `t` 返 true（值类型装箱 + 所有引用类型/接口/数组皆派生自
object，镜像 C#）。`int.IsAssignableFrom(long)` 仍 false（不过度放宽）。

**原因：** `typeof(object)` 是 handle-less Type（名 `"object"`，无 Std.Object TypeDesc 句柄），
`builtin_type_is_assignable_from` 对无句柄操作数落 FullName 相等 → 只有 object==object 为 true。

**修复（纯 runtime）：** `builtin_type_is_assignable_from` 在句柄/FullName 匹配前特判：`this` 名
（句柄名或 `__fullName` 槽）为 `"object"` 或 `"Std.Object"` → 对已确认非 null 的 `c` 直接返 true。

**文档影响：** `docs/design/language/reflection.md`（get-interface-byname 剩余项标记装箱语义落地）。

- [x] 1.1 `reflection.rs` `builtin_type_is_assignable_from`：object-root 特判
- [x] 1.2 `src/tests/types/object_assignable_from.z42`：e2e（int/double/bool/char/string/Foo/IBar/int[]/object → true；int<-long / int<-object / Foo<-IBar → false）——interp+jit 空输出 exit0
- [x] 1.3 全绿：types e2e（assignable_from ✓ + object_assignable_from ✓ 无回归；唯一 FAIL=fold_param_defaults 为 PR #25 测试跨 worktree 泄漏、非本变更）+ stdlib z42.core 44/0
- [x] 1.4 `docs/design/language/reflection.md` 标记
- [x] 1.5 归档 + PR

## 备注
- z42c 零改动 → 自举 trivially byte-identical（跳过 test compiler，无 z42c 源改动）。
- 未过度放宽：特判仅 this=object；其它 IsAssignableFrom 路径（子类/接口/句柄）不变，`int<-long` 仍 false。
- 剩余：大小写不敏感 GetInterface / 泛型变体接口 延后（另开）。
