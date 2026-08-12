# Tasks: 装箱 struct 引用身份 + struct 字段反射（P4b）

> 状态：🟢 实现完成，跑全 GREEN | 创建：2026-08-12 | 路 B2（装箱进 ScriptObject）+ 合为一个 change（User 裁决）

## 进度概览
- [x] 阶段 0: DRAFT + User 裁决（路 B2 / 合并 scope；SetValue 事实校正=值语义盒不可写穿→改共享盒解决）
- [x] 阶段 1: 表示层 `Value::BoxedStruct` → `GcRef<ScriptObject>` + inline_region_sizes 对 is_struct 读 struct_layout
- [x] 阶段 2: 装箱/拆箱（__box_struct→box_struct_blob alloc_object / unbox_struct / __struct_hash_code）改走 ScriptObject
- [x] 阶段 3: GC（mark/gen/trace/scan/size/跨代屏障 owner+new）+ is/as/GetType/vcall/array 约 40 处臂改读法；删 BoxedStructData
- [x] 阶段 4: 引用身份验证（cargo 882 绿；struct_boxing/object_methods/generic_container/array golden 双模式 EXIT=0）
- [x] 阶段 5: `struct_reflect.rs` 复刻布局 + 三层校验 + canon/tag_from_name 镜像 + 短名命名空间解析 + 7 单元测试
- [x] 阶段 6: 反射 GetValue（基元/引用/嵌套 boxed 快照）+ SetValue（就地写穿 + 引用叶子写屏障 + 基元实参拆箱）
- [ ] 阶段 7: golden e2e（reflection/struct_field interp+jit 匹配）✅ + xtask test 全 GREEN + self-host 5/5（进行中）+ 文档 ✅ + 归档 + PR

## 范围裁定（实施期）
- **(A) 装箱 struct 字段反射** = 本 change 交付（主路径，golden 端到端验证）。
- **(B) 对象内联 struct 字段反射**（`class C{ Point pt; }` 的 `GetValue(fi_pt, c)`）= **Deferred follow-up**：
  需另复刻**类级**内联布局（字段→对象相对 offset，与 struct `_compute` 不同）；现读 P3b dead-slot 得 Null（pre-existing，非回归）。

## 阶段 1: 表示层（types.rs）
- [ ] 1.1 `Value::BoxedStruct(GcRef<ScriptObject>)`（disc 17 不变）；删 `BoxedStructData`
- [ ] 1.2 `inline_region_sizes()` 对 `is_struct()` 读 `struct_layout`（size + ref_count）
- [ ] 1.3 `Value::visit`（:1172）/ `PartialEq`（:1233）/ `object_size_bytes` / `value_to_str` / `is_heap_ref` 的 BoxedStruct 臂改随 GcRef（同 Object）
- [ ] 1.4 编译通过（`cargo build`）——先让类型系统暴露所有触及点

## 阶段 2: 装箱/拆箱
- [ ] 2.1 `__box_struct`（convert.rs）：StructRef → `alloc_object(struct_td, 空 slots)` + 拷 bytes/refs 进对象 → `BoxedStruct(gc)`；幂等；按 type_name 查 TypeDesc
- [ ] 2.2 `unbox_struct`（exec_struct.rs）：`BoxedStruct(gc)` → 读 `gc.struct_bytes/refs` → arena StructRef
- [ ] 2.3 `__struct_hash_code`：读对象 struct_bytes/refs
- [ ] 2.4 若需：给 heap 加「alloc struct-typed object 并填 struct_bytes/refs」的便捷入口（或 alloc_object 后 borrow_mut 填）

## 阶段 3: GC + 各消费臂（约 44 处）
- [ ] 3.1 `arc_heap.rs` mark/mark_if_unmarked/trace_children/scan_object_refs 的 BoxedStruct 臂 → 随 GcRef（同 Object）
- [ ] 3.2 `exec_object.rs`/`exec_vcall.rs`/`exec_array.rs` + `jit/helpers/{vcall,object,array}.rs` + `corelib/object.rs` 的 BoxedStruct 臂改读 `gc.type_desc.name`/`struct_bytes`/`struct_refs`
- [ ] 3.3 array get_boxed/set_boxed：从 struct[] 元素字节造 `BoxedStruct(gc)`（alloc struct 对象）/ 从 `BoxedStruct(gc)` 读回元素
- [ ] 3.4 更新 `exec_struct_tests.rs`/`types_tests.rs` 构造 BoxedStruct 的单测

## 阶段 4: 引用身份验证
- [ ] 4.1 `cargo test --lib` 全绿
- [ ] 4.2 golden `struct_boxing_identity.z42`：`object b=a` 改 b 反射后 a 见 / 传参改盒可见（引用身份）
- [ ] 4.3 现有 struct_boxing/struct_object_methods golden 双模式 EXIT=0（无回归）

## 阶段 5: 布局复刻（corelib/struct_reflect.rs, NEW）
- [ ] 5.1 `canon`/`tag_from_name` 镜像
- [ ] 5.2 `struct_field_leaves(ctx, type_name) -> Vec<FieldLeaf>`（复刻 _compute）
- [ ] 5.3 `validate(computed, delivered)`：三层校验
- [ ] 5.4 单元测试：4 型 leaves + validate 通过/篡改 bail + tag_from_name 全基元

## 阶段 6: 反射 GetValue/SetValue（reflection.rs）
- [ ] 6.1 GetValue BoxedStruct 臂：基元/引用/嵌套(D4 boxed 副本)
- [ ] 6.2 SetValue BoxedStruct 臂：就地写穿 + 引用叶子写屏障
- [ ] 6.3 (B) Object 内联 struct 字段 GetValue/SetValue（若纳入；否则拆 follow-up）
- [ ] 6.4 顺序铁律：先判 boxed/is_struct 字段再触 slots

## 阶段 7: 验证 + 文档 + 归档
- [ ] 7.1 golden `src/tests/reflection/struct_field/`
- [ ] 7.2 `cargo test --lib` + `xtask test`（**不传 Z42_HOME**）+ self-host 5/5 byte-identical
- [ ] 7.3 docs：book struct-value-semantics（引用身份表示）+ reflection（复刻布局+校验+写穿）+ roadmap P4b 完成
- [ ] 7.4 归档 → `archive/2026-08-12-add-boxed-struct-identity/` + PR（分支保护 User 手动合）

## 关键风险
- **44 处臂遗漏**：阶段 1.4 先改类型、靠 `cargo build` 编译错误穷举所有触及点。
- **GC 正确性**：BoxedStruct 随 GcRef 必须与 Object 完全同路（漏一处→盒指对象被误回收）。cargo GC 单测 + golden 覆盖。
- **self-host**：纯 runtime，应 5/5 逐字节不变；破了说明误动 codegen。
- **(B) 对象级复合布局**：过复杂则拆 follow-up。
