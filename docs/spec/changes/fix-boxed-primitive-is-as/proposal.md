# fix-boxed-primitive-is-as — `object` 里的基元值 `is`/`as` 基元类型恒 false/null

> 类型：`fix`（bug 修复，IMPL→GREEN→COMMIT）。占用：`compiler` + `runtime`。

## 症状

```z42
object o = "hi";
bool b = o is string;      // ❌ false（应 true）
string s = o as string;    // ❌ null（应 "hi"）
object n = 5;
bool c = n is int;         // ❌ false（应 true）
```

基元值（string/int/bool/…）存进 `object` 变量后，`is`/`as` 该基元类型**永远不匹配**。
pre-existing（纯 nightly 编译器就这样），z42.ir 收敛的冒烟测试撞见后定位。

## 根因（两处，缺一不可）

1. **编译器**（`ExprEmitter._emitIs` / `_emitCast`）：`is`/`as` 的目标类型名走 `QualifyClass`，
   对基元别名 `string` 无 imported-ns 命中 → 落 `Qualify` **误加当前 ns 前缀** → 发出
   `is_instance %x, I.string`（`I` 是当前 namespace）。基元关键字不该被 ns 限定。
2. **运行时**（`interp/exec_object.rs::is_instance` / `as_cast`）：`match` 只处理 `Value::Object`
   / `Value::Array` / `Value::Null`，**基元值（`Value::Str`/`I64`/…）落 `_ => false`** ——从不按其
   stdlib 类名匹配（z42 不把基元装箱成 object，故 `object o = "hi"` 里 o 仍是裸 `Value::Str`）。

## 修复

1. **编译器**：新增 `EmitContext.QualifyTypeName`——基元别名 → FQ 包装类（`string`→`Std.String`、
   `int`→`Std.Int32`…），否则 `QualifyClass`。`_emitIs`/`_emitCast` 改用它。
2. **运行时**：`is_instance` / `as_cast` 的 `match` 加基元臂——基元值经 `primitive_class_name`
   取其 stdlib 类名，匹配 `class_name`（或 `Std.Object` 基类）。

## 验证

- `o is string`→true、`o as string`→"hi"、`n is int`→true（interp + jit）。
- self-host 5/5 byte-identical（编译器改动）；cargo test（运行时）；test compiler 全绿。
