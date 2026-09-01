# Proposal: 值类型（struct/enum）+ Type 对象的 Object 方法与数组名对齐 C#

## Why

值类型和 Type 对象的 `Object` 继承方法派发全面失配 C#（均以一致工具链 interp+jit 双模复现）：

| 场景 | 现状 | C# 应为 |
|------|------|---------|
| `a.GetType()`（struct 实例） | ❌ `VCall: function Demo.A.GetType not found` | `Demo.A` |
| `a.ToString()` / `a.GetHashCode()` / `a.Equals(x)`（struct） | ❌ 均 `VCall not found` | 正常 |
| `E.Red.GetType()`（enum 实例） | ❌ `Std.Int32`（enum 底层 i64） | `Demo.E` |
| `typeof(A).GetType()`（Type 对象） | ❌ `null` → 链式 `.FullName` 崩 `FieldGet on Null` | `Std.Type` |
| `typeof(int[]).FullName` | `Std.Array`（丢元素类型） | `Std.Int32[]`（全路径元素名 + `[]`） |

对照：**本地 class 的 `c.GetType()`/`c.ToString()` 正常**（`Demo.C`/`C`）——只有值类型 + Type 对象 + 数组这几类 receiver 坏。

根因：
- **struct**：[ClassExtractor.z42:133-140](../../../../src/compiler/z42c.semantics/src/ClassExtractor.z42) 用 `if (!isStruct)` **故意排除** struct 的 Object 四方法（注释误称「镜像 C# ExcludeFromImplicitObject」——实际 C# struct 经 `ValueType : Object` **有** GetType/ToString/…，只是靠装箱/constrained-call 派发，并非排除）。加上 [CallEmitter.z42:103](../../../../src/compiler/z42c.semantics/src/CallEmitter.z42) 把 struct 实例调用一律发**静态 `Call {Struct}.{方法}`**，Object 方法在 struct 上无函数体 → 崩。**runtime 其实已支持装箱 struct 的 Object 协议**（[exec_vcall.rs:200-224](../../../../src/runtime/src/interp/exec_vcall.rs)）。
- **enum**：enum 值运行期是裸 i64，`GetType` 走 `primitive_class_name(I64)→Std.Int32`。值类型 sealed → 编译期静态类型即运行期类型，可折叠。
- **Type 对象**：GetType 对 imported `Std.Type` receiver 返回 null（[SymbolCollector.z42:85](../../../../src/compiler/z42c.semantics/src/SymbolCollector.z42) 注释已警告「GetType 返回类型退化 → FieldGet on Null」）——待实施期钉准派发点。
- **数组**：[type_object.rs build_type_ex](../../../../src/runtime/src/corelib/reflection/type_object.rs) 给数组合成名 `Std.Array` 而非按元素类型 `{elemFullName}[]`。

不做的后果：值类型无法参与任何反射/序列化/日志（`GetType`/`ToString` 是最基础的 Object 协议），与 C# 心智模型严重不符。

## What Changes

- **GetType on 值类型 receiver（struct/enum）**：编译器折叠为 `typeof(静态类型)`（值类型 sealed，静态==运行期类型）。一举修 struct.GetType + enum.GetType。
- **ToString/Equals/GetHashCode on struct receiver**（struct 未自声明时）：装箱 receiver（`__box_struct`）+ 发 VCall → 命中 runtime 已有的装箱-struct Object 协议。
- **Type 对象 GetType**：根因修复，使 `typeof(X).GetType()` 返回 `typeof(Std.Type)`（非 null）。
- **数组 FullName/Name 全路径**：runtime `build_type_ex` 令数组 Type 的 FullName = `{元素FullName}[]`（如 `Std.Int32[]`）、Name = `{元素Name}[]`（`Int32[]`），元素经 `make_type_from_name` 解析（已含 change A 的 Std.* 解析）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/CallEmitter.z42` | MODIFY | 值类型 receiver 的 Object 方法路由：GetType 折叠 typeof；ToString/Equals/GetHashCode 装箱 + VCall |
| `src/compiler/z42c.semantics/src/ClassExtractor.z42` | MODIFY | 视实施决定：让 struct 的 Object 四方法可被绑定解析（不再无条件排除），或保留排除仅在 CallEmitter 路由（二选一，design 定） |
| `src/compiler/z42c.semantics/src/SymbolCollector.z42` | MODIFY | Type 对象 GetType 返回 null 的根因修复（imported Std.Type GetType 派发） |
| `src/runtime/src/corelib/reflection/type_object.rs` | MODIFY | 数组 Type FullName/Name 全路径（`{elem}[]`） |
| `src/runtime/src/corelib/reflection/reflection_tests.rs` | MODIFY | 数组全路径名单测 |
| `src/tests/types/value_type_object_methods.z42` | NEW | struct GetType/ToString/Equals/GetHashCode + enum GetType + Type 对象 GetType |
| `src/tests/types/value_type_object_methods.expected_output.txt` | NEW | 期望输出（若用 golden 模式；否则 Assert 式无此文件） |
| `src/tests/types/array_type_fullname.z42` | NEW | `typeof(int[]).FullName == Std.Int32[]` 等 |
| `docs/design/language/reflection.md` | MODIFY | 值类型 Object 方法派发 + 数组全路径名机制 |
| `docs/book/src/runtime/struct-value-semantics.md` | MODIFY | struct 的 Object 方法（装箱派发）补充（如相关） |

**只读引用**：
- `src/runtime/src/interp/exec_vcall.rs` — 装箱-struct Object 协议（目标 runtime 行为，已存在）
- `src/compiler/z42c.semantics/src/AccessEmitter.z42` — `__box_struct` 用法参照
- `src/compiler/z42c.semantics/src/RecordSynth.z42` — record 的 GetType/ToString 合成参照
- `src/runtime/src/interp/exec_vcall.rs` 的 `primitive_class_name` — enum→i64→Std.Int32 现状

## Out of Scope

- **enum 的 ToString/Equals/GetHashCode**：本次 enum 只修 GetType（用户诉求聚焦于此）；enum 其余 Object 方法若也缺，另立。
- **数组协变 / 多维 / jagged 的 typeof 语法**：沿用现有 Deferred。
- **值类型作为 `object`/接口的隐式装箱转换**（已有 `__box_prim`/`__box_struct` 路径）——本次只补方法调用派发。

## Open Questions

- [ ] ③ Type 对象 GetType 返回 null 的精确派发点（imported-class GetType）——实施期第一步先钉准，再决定修在 SymbolCollector 还是 CallEmitter。
- [ ] struct Object 方法：`ClassExtractor` 解除排除 vs 纯 CallEmitter 路由——design 阶段定（倾向后者，改动面小、不扰 struct 无 vtable 的既有假设）。
