# Tasks: 反射基元/泛型类型名统一

> 状态：🟢 已完成 | 创建：2026-09-01 | 完成：2026-09-01 | 类型：vm（反射行为）

## 进度概览
- [x] 阶段 1: 基元真句柄解析（问题 1）
- [x] 阶段 2: 泛型 FullName 合成（问题 2）
- [x] 阶段 3: 测试（编写）
- [x] 阶段 4: 文档同步
- [x] 阶段 5: 验证 GREEN（`xtask test` 全 stage 全绿：e2e 277/0 · cross-zpkg · stdlib 含 json 11/11 · z42c 自举不动点 3/3 · vscode-syntax；runtime `cargo test --lib` 993/0）

## 阶段 1: 基元真句柄解析（问题 1）
- [x] 1.1 `type_object.rs`：新增 `primitive_fqn(name) -> Option<&'static str>`（标签+关键字两套→Std.* FQN）
- [x] 1.2 `type_object.rs`：`make_type_from_name` 在 canonical 兜底前插入 `primitive_fqn`→`try_lookup_type`→`make_type_object`
- [x] 1.3 `type_object.rs`：更新「C#-style aliases」相关注释为「基元解析真 Std.* 句柄」

## 阶段 2: 泛型 FullName 合成（问题 2）
- [x] 2.1 `type_object.rs`：`make_constructed_type` 合成 `__fullName = base + "<" + argFulls.join(",") + ">"`（递归含嵌套）
- [x] 2.2 确认 `Name` / `__typeArgs` / `IsGenericTypeDefinition` 不受影响（只覆盖 `__fullName` 槽）
- [x] 2.3 （GREEN 发现·Scope 扩展）`metadata/types.rs` `default_value_for` 识别 FQ wrapper 名（`Std.Int32`…→零值）+ `types_tests.rs` 单测。根因：反射 `MakeGenericMethod(typeof(int)).Invoke` 的 `default(T)` 经 method_type_args 现携带 FQ 名（typeof(int) 现真句柄），消费端须认得该词汇，否则 `default(int)` 返 null 而非 0（挂 `reflect_generic_method` e2e）。
- [x] 2.4 （GREEN 发现·Scope 扩展）JSON serde 集合检测按去实参**基名**匹配：`JsonBinder`/`JsonSerializer` 的 `fn == "Std.Collections.List"` 改为截 `<` 前基名。根因：issue 2 让 `List<int>` 的 FullName 变 `Std.Collections.List<Std.Int32>`，精确匹配失效 → List 被当对象序列化成 `{"Count":3}` / 反序列化空（挂 z42.json 12 个 serde 用例）。同步 `json-serde.md` 分派伪代码。

## 阶段 3: 测试
- [x] 3.1 `reflection_tests.rs`：`primitive_fqn` 映射单测（合成 FullName 需 z42.core → 由 golden 覆盖）
- [x] 3.2 新增 `src/tests/types/primitive_type_identity.z42`：typeof≡GetType 恒等 + 各基元（Assert 式）
- [x] 3.3 新增 `src/tests/types/generic_fullname.z42`：List<int>/typeof/多实参/嵌套 FullName（Assert 式）
- [x] 3.4 更新既有断言 `.Name=="int"/"string"/…`→`"Int32"/"String"/…`：array_element_type / inherited_static_fields / generic_type_definition / get_properties / instance_generic_args / nested_generic_args / instance_nested_generic_args / typeof / static_fields_reflect / enum_underlying_type / z42.core reflection.z42
- [x] 3.5 核查：type_flags.z42（IsValueType False→True + 注释）；generic_predicates / interface_class_predicates / nested_types 经核查不受影响（IsPrimitive/IsClass 值不变）；struct_generic_container 无名字断言

## 阶段 4: 文档同步
- [x] 4.1 `src/runtime/src/corelib/README.md`：lenient/合成 Type 注释（primitive 不再 synthetic）
- [x] 4.2 `src/libraries/z42.core/src/Type.z42`：类头注释订正（基元真句柄、成员非空）
- [x] 4.3 `src/libraries/z42.json/src/JsonBinder.z42`：更新两词汇注释（FullName 现统一 Std.*）
- [x] 4.4 `docs/design/language/reflection.md`：订正 line 35/59/72 + 构造型泛型段（Name=Int32 / FullName=Std.Int32 / 真句柄 / 泛型 FullName 含实参）

## 阶段 5: 验证 GREEN
- [ ] 5.1 `cargo build --release`（z42vm）无错
- [ ] 5.2 `xtask test e2e`（含 types golden）全绿
- [ ] 5.3 `xtask test stdlib`（z42.json serde 回归）全绿
- [ ] 5.4 `xtask test compiler`（自举，反射改动不扰编译器）全绿
- [ ] 5.5 `xtask test` 完整 gate 全绿
- [ ] 5.6 spec scenarios 逐条覆盖确认 + z42vm 探针手验前后差异

## 备注
- 问题 3（REPL 缺失方法报错不一致）不在本变更，另立 `fix-repl-missing-method-error`。
- 无 zbc/zpkg 格式变更、无 version bump（`__fullName`/`__name` 是运行期对象槽）。
