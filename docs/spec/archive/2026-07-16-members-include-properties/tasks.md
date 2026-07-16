# Tasks: GetMembers() 纳入 properties

> 状态：🟢 已完成 | 创建：2026-07-16 | 完成：2026-07-16
**变更说明：** `Type.GetMembers()` 在字段 + 方法之后追加 PropertyInfo（`__type_members` 扩展调 `__type_properties`）。
**原因：** 对齐 C# `GetMembers()`（属性与其 get_/set_ 访问器方法一并出现）；完成 reflection.md `reflection-future-properties` 的「properties 纳入 GetMembers()」遗留项。
**变更类型：** 小 feat（纯运行期，无 zbc 格式 bump、零编译器改动 → 自举天然稳）。
**文档影响：** `docs/design/language/reflection.md`（Deferred 标落地）。

- [x] 1.1 `src/runtime/src/corelib/reflection.rs` `builtin_type_members`：追加 `builtin_type_properties`
- [x] 1.2 `src/libraries/z42.core/tests/reflection.z42`：加 `test_getmembers_includes_properties`（PropHolder 的 Size/Tag 属性出现在 GetMembers）
- [x] 1.3 `reflection.md`：`reflection-future-properties` 剩余项「properties 纳入 GetMembers()」标落地
- [x] 1.4 验证：完整 `./xtask test` **GREEN**（XTASK_EXIT=0，0 fail，test_getmembers_includes_properties PASS，自举不动点 7/7）——首跑即绿（debug-vm fix 已生效）
- [x] 1.5 归档到 archive/2026-07-16-members-include-properties

## 备注
- 访问器方法 `get_/set_` 仍保留在 methods 切片（GetMethods）——与 C# 一致，属性视图叠加。
