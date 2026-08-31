# Tasks: 泛型值 struct 数组 runtime 修复 + KeyValuePair 值类型化

> 状态：🟢 已完成 | 创建：2026-08-31 | 完成：2026-08-31 | 类型：fix（最小化模式）

**变更说明：** 修 runtime `try_struct_backed` 按**擦除裸名**查类型（泛型 struct 单一 TypeDesc 注册在裸名下），
让 ≥2 字段泛型 `[Record] struct` 的**数组**正确获得 StructBytes 背衬；顺带把 `KeyValuePair` 从 class 改成
`[Record] public struct`（原被此 bug 阻塞——`Dictionary.Entries()` 返回 `KeyValuePair<K,V>[]`）。

**原因：** `exec_array.rs::try_struct_backed` 用带泛型实参的全名 `Kv<string,int>` 查类型，而泛型类型擦除只
注册裸名 `Kv` 的 TypeDesc → miss → 数组退化成引用背衬（元素 Null）→ `struct_fset_prim` 崩
`expected StructRef, got Null`。全名仍用于 `struct_backed`（保元素反射 `GetElementType()`）。

**文档影响：** z42.core README（KeyValuePair 值类型）、`docs/book` struct 数组/值语义机制页注记、
`KeyValuePair.z42` 注释（已更新）。

## 进度概览
- [x] 1.1 runtime fix（exec_array.rs 剥泛型实参用裸名查类型）——L1 同包
- [x] 1.2 KeyValuePair class → `[Record] public struct` + 注释
- [x] 1.3 dict_iter 加 `using Std.Collections`
- [x] 1.4 cross-zpkg 泛型 struct 数组回归测试（target/ext/main 三包）
- [x] 1.5 cargo test --lib（runtime 改动）—— 21 passed
- [x] 1.8 **L2 编译器 fix**（ExprEmitter 数组元素名剥实参再限定）——跨包
- [x] 1.6 完整 GREEN（xtask test，含 L1+L2 + gen1==gen2 3/3 逐字节 + cross-zpkg 全过）
- [x] 1.7 文档同步（book struct-value-semantics.md 泛型 struct 数组注记；README KeyValuePair 定位不变无需改）

> **两层 fix**：L1（runtime `exec_array.rs`）修同包泛型 struct 数组；L2（编译器 `ExprEmitter.z42`）修
> 跨包——真根因是 `array_new` 对泛型元素名 `Kv<K,V>` 用**错误 ns** 限定（`QualifyClass` 按裸键
> `ImportedClassNs` 查、含实参 miss → 回落当前 ns）。L2 剥实参到裸基名 QualifyClass 再回贴（镜像
> StructAlloc）。二者合起来跨包泛型 struct 数组彻底工作。无 zbc/zpkg 格式变更、gen1==gen2 零风险
> （自举源码唯一泛型 struct 数组构造 = Dictionary.Entries() 同包，L2 跨包修复不影响其字节）。

## 任务
- [x] 1.1 `src/runtime/src/interp/exec_array.rs::try_struct_backed`：查类型前 `element_type.split('<').next()`
      剥泛型实参用裸名 `try_lookup_type`；全名仍传 `struct_backed`（一处覆盖 interp/jit/数组字面量）。
- [x] 1.2 `src/libraries/z42.core/src/Collections/KeyValuePair.z42`：`public class` → `[Record] public struct`，
      重写注释（值语义、对齐 ValueTuple/C# BCL、消除 Entries() per-pair 堆对象）。
- [x] 1.3 `src/libraries/z42.collections/tests/dict_iter/source.z42`：加 `using Std.Collections;`
      （KVP struct 正确 track ns，暴露该文件本就缺的 using；`Std.Collections` 不在 prelude 免-using 集）。
- [x] 1.4 `src/tests/cross-zpkg/generic_struct_array_cross_pkg/`（target + main + expected）：
      跨包泛型 `[Record] struct` 数组 + 元素字段读 + 标量对照回归。
- [ ] 1.5 `cargo test --lib`（exec_array 改动，xtask test 不跑 Rust 单测）。
- [ ] 1.6 `xtask test` 完整 GREEN（含新 z42vm + struct KVP 重建 stdlib + dict_iter + 新 cross-zpkg）。
- [ ] 1.7 文档同步：z42.core README KeyValuePair 行、book struct-value-semantics/数组机制页注记。

## 备注
- fix 已本地验证：旧 vm 崩 `got Null`，新 vm interp+jit 均输出正确（泛型 struct 数组工作）。
- 教训：`.z42/bin/z42c` apphost **忽略 `Z42_LIBS`**（用内嵌种子 stdlib）——手动跨包验证 struct 表示变更
  必须用 `z42vm driver.zpkg` 并设 `Z42_LIBS`，否则「旧种子编 + 新库跑」的版本偏斜会伪装成 layout bug。
