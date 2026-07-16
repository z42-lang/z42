# Tasks: GetFields() 隐藏 auto-property 后备字段

> 状态：🟢 已完成 | 创建：2026-07-16 | 完成：2026-07-16
**变更说明：** `Type.GetFields()` 过滤掉编译器合成的 auto-property 后备字段（`__prop_<Name>`）。
**原因：** 对齐 C#——auto-property 的后备字段（C# 的 `<Name>k__BackingField`）不出现在 GetFields，属性经 GetProperties 可见。此前 z42 把 `__prop_*` 后备字段暴露进 GetFields（`ExportedTypeExtractor` OwnFieldNames 含 `__prop_X`）。完成 reflection.md `reflection-future-properties` 的「隐藏 auto-property backing field」遗留项。
**变更类型：** 小 feat / 正确性修（纯运行期，无 zbc 格式 bump、零编译器改动 → 自举天然稳）。
**文档影响：** `docs/design/language/reflection.md`（Deferred 标落地）。

- [x] 1.1 `src/runtime/src/corelib/reflection.rs`：加 `is_autoprop_backing`（`__prop_` 前缀）+ `builtin_type_fields` 实例/静态两循环过滤
- [x] 1.2 `src/libraries/z42.core/tests/reflection.z42`：加 `test_getfields_hides_autoprop_backing`（PropHolder 只有 auto-prop → GetFields 空、无 `__prop_*`）
- [x] 1.3 `reflection.md`：Deferred 剩余项标落地
- [x] 1.4 验证：完整 `./xtask test` **GREEN**（XTASK_EXIT=0，0 fail，test_getfields_hides_autoprop_backing PASS，自举不动点 7/7）
- [x] 1.5 归档到 archive/2026-07-16-hide-autoprop-backing-field

## 备注
- `__prop_<Name>` 是 IrGen 合成后备字段命名（IrGen.z42:225 `"__prop_" + pd.Name`）；`__` 前缀保留、用户不应占用 → 过滤安全。
- 连带：GetMembers() 也不再显示 `__prop_*`（其字段部分复用 `__type_fields`）。
