# Design: 反射基元/泛型类型名统一

## Architecture

```
typeof(T) ──Typeof opcode──┐
obj.GetType() ─────────────┤
FieldType/PropertyType ────┼──► make_type_from_name(name) ──► [primitive? → Std.* 真句柄]
数组元素 / 泛型实参 ───────┘                              └► [class 名 → 真句柄]
                                                          └► [兜底 → 合成 Type(规范名)]

typeof(G<A>) / G<A> 实例 ──► make_constructed_type(base, [argNames])
                              ├─ 逐 arg → make_type_from_name（递归，基元→Std.*）
                              ├─ 写 __typeArgs 槽（供 GetGenericArguments）
                              └─ 合成 __fullName = base.FullName + "<" + argFulls.join(",") + ">"
```

`FullName` / `Name` 是 `Std.Type` 对象上 VM 构造期写入的槽；`builtin_type_full_name` 只读槽（不改）。
所有反射入口都汇聚到 `make_type_from_name` / `make_constructed_type`，故这两处即根因修复点。

## Decisions

### Decision 1: 基元解析到真句柄（根因）而非只改合成名

**问题：** `typeof(int)` 是 handle-less 合成 Type，名字为关键字别名 `int`；`(5).GetType()` 是真
`Std.Int32` 句柄。二者对同一类型不一致，且合成路径附带 `IsValueType=false`、`GetMethods()=0` 两个错值。

**选项：**
- A（根因）——`make_type_from_name` 把基元关键字/标签映射到 `Std.*` FQN 并解析真句柄。`typeof(int)`
  从此 ≡ `(5).GetType()`：Name `Int32`、FullName `Std.Int32`、IsValueType `true`、成员可枚举。
- B（表层）——保持合成，仅把合成名从 `int` 改成 `Std.Int32`/`Int32`。仍与 GetType 在 handle-ness 上分叉
  （成员数 0 vs 11、IsValueType false vs true 不修）。

**决定：** 选 A。C# 中每个基元只有**一个** `Type`（`typeof(int) == 5.GetType()`）；A 是唯一让两路径真正
恒等的方案，并顺带修复 `IsValueType`/成员两个既有 bug。符合 philosophy「根因修复 / 最终方案优先」。

### Decision 2: `primitive_fqn` 同时覆盖标签与关键字两套拼写

**问题：** VM 有两套基元词汇——字段槽用关键字（`int`/`long`/`str`），函数签名用标签（`i32`/`i64`/`str`），
`make_type_from_name` 两者都会收到。

**决定：** 新增 `primitive_fqn(name) -> Option<&'static str>`，把两套拼写都映射到同一 FQN：

| 关键字 / 标签 | FQN |
|---|---|
| `sbyte` / `i8` | `Std.SByte` |
| `byte` / `u8` | `Std.Byte` |
| `short` / `i16` | `Std.Int16` |
| `ushort` / `u16` | `Std.UInt16` |
| `int` / `i32` | `Std.Int32` |
| `uint` / `u32` | `Std.UInt32` |
| `long` / `i64` | `Std.Int64` |
| `ulong` / `u64` | `Std.UInt64` |
| `float` / `f32` | `Std.Single` |
| `double` / `f64` | `Std.Double` |
| `bool` | `Std.Boolean` |
| `char` | `Std.Char` |
| `string` / `str` | `Std.String` |

（FQN 常量部分已在 `well_known_names.rs`；`Std.Byte`/`Std.SByte`/`Std.Int16`/`Std.UInt*` 用字面量。
与 `array.rs` 既有 `int_wrapper_fqn` 表同源，可考虑抽共用，但避免跨模块耦合，先各自持有。）

### Decision 3: 泛型 FullName 在构造期合成，逗号无空格，递归

**决定：** 在 `make_constructed_type` 里，实参解析完后合成 `__fullName = base_full + "<" + join(argFull, ",") + ">"`：
- 逗号**无空格**（与 typeof arg 拼写一致、稳定可比）。
- 递归天然成立：实参经 `make_type_from_name` → 若本身是构造型则再走 `make_constructed_type`，其 `__fullName`
  已含自己的 `<…>`。
- `Name` 不动（基简名 `List`）；`__typeArgs` 不动（`GetGenericArguments` / `IsGenericTypeDefinition` 不变）。

### Decision 4: JsonBinder 保留双分支、只更新注释

**问题：** `JsonBinder.FromJson` 按 `t.FullName` 分派，历史注释称 FieldType 用关键字 `int`、GetType 用
`Std.Int32` 两套词汇，代码 `fn == "int" || fn == "Std.Int32"` 双容错。本变更后 FieldType.FullName 亦为
`Std.Int32`，`"int"` 分支经反射不再触达。

**决定：** 分派逻辑本就对 `Std.Int32` 恒正确（且 book `json-serde.md` 早已以 `Std.Int32` 为轴），故**行为零变**。
保留 `|| "int"` 防御分支（非版本兼容层，是防御式容错），仅更新失真注释。彻底删双分支属独立清理，Out of Scope。

## Implementation Notes

- **插入点**：`make_type_from_name` 内，现有 class-registry / lazy / dotless 查找**之后**、`canonical_type_name`
  兜底**之前**插入 `primitive_fqn` → `try_lookup_type` → `make_type_object`。`int` 无 `.`、`resolve_dotless_simple`
  不命中（无简名为 `int` 的类型）、且小写不触发 force-load，故落到此处天然正确；FQ 拼写（`Std.Int32`）在更早的
  `try_lookup_type` 已命中、不进 `primitive_fqn`。
- **兜底保留**：`primitive_fqn` 命中但 `try_lookup_type` 失败（z42.core 未加载）→ 继续走 `canonical_type_name`
  合成路径，不 panic（lenient 约定）。
- **FullName 读取辅助**：合成时读各 arg Type 的 `__fullName` 用 `read_type_str_slot(&[arg.clone()], "__fullName")`；
  读 base 现有 `__fullName` 同法，再 `set_field_value` 覆盖。
- **README lenient 注释**：primitive 不再属「合成 Type」——改为「array，及 z42.core 缺失时的 primitive 兜底」。
- **Type.z42 文档**：删除/订正「Primitives 返回 handle-less Type、成员查询为空」的类头注释。

## Testing Strategy

- **单元测试**（`reflection_tests.rs`）：`primitive_fqn` 两套拼写映射全覆盖；`make_constructed_type` 合成
  `List<Std.Int32>` / 嵌套 / 多实参 FullName。
- **Golden e2e**（`src/tests/types/`）：
  - `primitive_type_identity.z42` —— `typeof(int) ≡ 5.GetType()`（FullName/Name/IsValueType/成员数），各基元覆盖。
  - `generic_fullname.z42` —— `List<int>` 实例与 `typeof`、`Dictionary<string,int>`、嵌套 `List<List<int>>`。
  - 更新 6 个既有断言 `.Name=="int"` → `"Int32"`（array_element_type / inherited_static_fields /
    generic_type_definition / get_properties / instance_generic_args / nested_generic_args）。
- **回归护栏**：`xtask test stdlib`（z42.json serde 用例，验 FullName 分派仍绿）+ `xtask test e2e`（types golden）。
- **完整 GREEN**：`xtask test` 全 stage（含 compiler 自举，确保反射改动不扰编译器）。
- **确定性手验**：`z42vm <app.zpkg>` 直跑探针程序（本次探索用的免-launcher harness）比对前后。
