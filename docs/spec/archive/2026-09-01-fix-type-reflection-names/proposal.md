# Proposal: 反射基元/泛型类型名统一到 Std.* 全限定形

## Why

反射类型名当前对**同一个类型**在两条路径上撒谎不一致，且泛型实例名丢信息：

1. **基元路径不一致**：`typeof(int)` 走 handle-less 合成 Type → `FullName="int"`、`Name="int"`、
   `IsValueType=false`（**错**，int 是值类型）、`GetMethods().Length=0`；而 `(5).GetType()` 走真
   `Std.Int32` 句柄 → `FullName="Std.Int32"`、`Name="Int32"`、`IsValueType=true`、`GetMethods().Length=11`。
   同一类型 `int`，两条反射路径给出不同答案，且 `typeof(int) != 5.GetType()`（违反 C# `System.Type` 恒等语义）。

2. **泛型实例 FullName 丢实参**：`List<int>`（实例 `.GetType()` 与 `typeof(List<int>)` 皆然）的
   `FullName="Std.Collections.List"`，缺泛型实参——尽管 `GetGenericArguments().Length==1`（实参确实被携带）。
   应为 `Std.Collections.List<Std.Int32>`。

根因：`make_type_from_name`（[type_object.rs:116-122](../../../../src/runtime/src/corelib/reflection/type_object.rs)）对基元关键字/标签走 `canonical_type_name` 回落到 **C# 关键字别名**（`int`）并构造 handle-less 合成 Type，而不解析到真实存在的 `Std.Int32` struct 句柄；`make_constructed_type` 把实参写进 `__typeArgs` 槽但 `__fullName` 只写基名。

不做的后果：反射面持续对基元类型二义（关键字 vs FQ 两套词汇）、`typeof(int)` 无成员/错判值类型、泛型名不可读；反射驱动的序列化/比较（如 JSON serde、attribute 匹配）要一直背两套词汇的容错分支。

## What Changes

- `make_type_from_name`：基元关键字（`int`）与标签（`i32`）在回落前先映射到 FQ wrapper 名（`Std.Int32`）
  并解析**真句柄**。使 `typeof(int) ≡ (5).GetType()`（Name `Int32`、FullName `Std.Int32`、IsValueType `true`、
  成员可枚举）。统一作用于 `FieldType` / `PropertyType` / `ParameterType` / 数组元素 / 泛型实参（皆经此函数）。
- `make_constructed_type`：构造型泛型的 `__fullName` 合成为 `<基名><<实参FullName,…>>`（递归含嵌套泛型），
  使 `FullName` = `Std.Collections.List<Std.Int32>`；`Name` 保持基简名 `List`。
- 同步更新受影响的反射断言测试（`.Name=="int"` → `"Int32"` 等）、`Std.Type` 类文档注释、
  `JsonBinder` 两词汇注释、反射机制文档中过时的 `int`/synthetic 描述。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/corelib/reflection/type_object.rs` | MODIFY | 加 `primitive_fqn` 映射；`make_type_from_name` 基元解析到真句柄；`make_constructed_type` 合成含实参的 `__fullName`；更新注释 |
| `src/runtime/src/corelib/reflection/reflection_tests.rs` | MODIFY | 加单测覆盖 `primitive_fqn` 映射 + 构造型 FullName 合成 |
| `src/runtime/src/metadata/types.rs` | MODIFY | `default_value_for` 识别 FQ wrapper 名（`Std.Int32`…→零值）——反射 `MakeGenericMethod(typeof(int)).Invoke` 的 `default(T)` 现经 method_type_args 携带 FQ 名，消费端需认得该词汇 |
| `src/runtime/src/corelib/README.md` | MODIFY | 「lenient/合成 Type」注释：primitive 不再 synthetic（仅 array + z42.core 缺失兜底） |
| `src/libraries/z42.core/src/Type.z42` | MODIFY | 类文档注释：基元现解析真句柄、成员非空（原「handle-less，成员空」描述失真） |
| `src/libraries/z42.json/src/JsonBinder.z42` | MODIFY | 更新「两套词汇」注释；集合检测按去实参基名匹配（构造型 FullName 现含 `<…>`） |
| `src/libraries/z42.json/src/JsonSerializer.z42` | MODIFY | 集合检测（List/Dict）按去实参基名匹配——否则 `List<int>` 被当对象序列化成 `{"Count":3}` |
| `src/tests/types/primitive_type_identity.z42` | NEW | `typeof(int) ≡ 5.GetType()` 恒等（FullName/Name/IsValueType/成员）+ 各基元（Assert 式，无 golden 输出） |
| `src/tests/types/generic_fullname.z42` | NEW | `List<int>` / `typeof(List<int>)` / 嵌套 / 多实参 FullName 含 `<…>`（Assert 式） |
| `src/tests/types/array_element_type.z42` | MODIFY | `GetElementType().Name` 断言 `"int"/"string"`→`"Int32"/"String"` |
| `src/tests/types/inherited_static_fields.z42` | MODIFY | `FieldType.Name` 断言 `"int"`→`"Int32"` |
| `src/tests/types/generic_type_definition.z42` | MODIFY | 泛型实参 `Name` 断言 `"int"/"string"`→`"Int32"/"String"` |
| `src/tests/types/get_properties.z42` | MODIFY | 属性类型 `Name` 断言 `"int"/"string"`→`"Int32"/"String"` |
| `src/tests/types/instance_generic_args.z42` | MODIFY | 实参 `Name` 断言 `"int"`→`"Int32"` |
| `src/tests/types/nested_generic_args.z42` | MODIFY | 嵌套实参 `Name` 断言 `"int"/"string"`→`"Int32"/"String"` |
| `src/tests/types/instance_nested_generic_args.z42` | MODIFY | 嵌套实参 `Name` 断言 `"int"/"string"`→`"Int32"/"String"` |
| `src/tests/types/typeof.z42` | MODIFY | 基元 `Name` 断言 `"int"/"string"/"bool"`→`"Int32"/"String"/"Boolean"` + 过时注释 |
| `src/tests/types/type_flags.z42` | MODIFY | `typeof(int).IsValueType` 断言 `False`→`True`（真值类型）+ 过时注释 |
| `src/tests/types/static_fields_reflect.z42` | MODIFY | `FieldType.Name` 断言 `"int"/"string"`→`"Int32"/"String"` |
| `src/tests/types/enum_underlying_type.z42` | MODIFY | `GetEnumUnderlyingType().Name` 断言 `"long"`→`"Int64"` |
| `src/libraries/z42.core/tests/reflection.z42` | MODIFY | 反射 `Name` 断言 `"int"/"double"/"string"`→`"Int32"/"Double"/"String"` |
| `docs/design/language/reflection.md` | MODIFY | 修正 line 35/59/72 + 构造型泛型段过时描述（基元真句柄、Name=`Int32`、FullName=`Std.Int32`、泛型 FullName 含实参） |

**只读引用**：

- `src/runtime/src/corelib/reflection/type_query.rs` — 理解 `builtin_type_full_name` 只读槽
- `src/runtime/src/corelib/object.rs` — 理解 `(5).GetType()` 真句柄路径（目标行为参照）
- `src/runtime/src/interp/exec_vcall.rs` — `primitive_class_name` FQN 映射参照
- `src/runtime/src/corelib/array.rs` — 既有 `int_wrapper_fqn` 关键字→FQN 表参照
- `src/runtime/src/metadata/well_known_names.rs` — `STD_INT32` 等 FQN 常量
- `docs/book/src/stdlib/json-serde.md` — 已用 `Std.Int32` 分派轴（本变更使实现与之对齐）

## Out of Scope

- **问题 3（REPL 缺失方法报错不一致）**：属 REPL/编译器逐类型惰性 reconcile 完整性缺口，另立变更
  `fix-repl-missing-method-error` 处理。
- **`docs/design/language/reflection.md` → book 全量迁移**：598 行大文档迁移是独立工作；本变更只订正其中因本次改动而失真的具体行。
- **`Name` 含泛型 arity**（C# `` List`1 ``）：本变更 `Name` 保持基简名 `List`，不引入 backtick。
- **JsonBinder 容错分支精简**（删 `|| "int"` 死分支）：保留防御分支，仅更新注释；精简另议。

## Open Questions

- 无（三处设计选择已由 User 裁决：基元统一 `Std.Int32`/`Int32`、泛型 FullName 尖括号形、问题 3 另做）。
