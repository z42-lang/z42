# Tasks: 对象内联 struct 字段反射（P4b-B）

- [x] `struct_reflect::compute_class_inline` 复刻 `_computeInlineLayout`（只打包 struct 字段、跳过非 struct）
- [x] `struct_reflect::struct_field_fq`（字段是否内联 struct → 返 FQ 名 / None）
- [x] `reflection.rs`：抽 `snapshot_struct_leaf`（GetValue 读，装箱嵌套 + 对象内联共用）
- [x] `reflection.rs`：抽 `write_struct_leaf`（SetValue 写，补齐嵌套引用叶子写屏障，装箱 + 对象共用）
- [x] `reflection.rs`：`builtin_field_get_value` 的 `Value::Object` 臂加 `object_inline_struct_field_get`（Some→字节路径 / None→slot）
- [x] `reflection.rs`：`builtin_field_set_value` 的 `Value::Object` 臂加 `object_inline_struct_field_set`
- [x] `struct_reflect_tests.rs`：`class_td` helper + 3 个类级内联布局单测
- [x] golden `reflection/struct_field` 扩对象内联用例（含嵌套 `Frame{Line edge}`）+ expected 更新
- [x] GREEN：cargo test --lib（892 passed）+ 手动 golden interp/jit 双模式匹配
- [ ] GREEN：`xtask test`（不传 Z42_HOME）+ self-host 5/5 gen1==gen2 byte-identical
- [x] docs：book `struct-value-semantics.md` 加节 + 收敛面 ✅；roadmap Deferred 移除本项
- [ ] 归档 + PR（格式中立，分支保护 User 手动合）
