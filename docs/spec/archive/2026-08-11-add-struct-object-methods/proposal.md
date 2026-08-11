# Proposal: struct 合成对象协议方法（Equals / GetHashCode / ToString）

> 「struct 值类型完备化」工作流 **PR2b**（PR2a 装箱身份的收口）。保留「unboxed struct 无 vtable、编译器
> 合成值方法」既有设计（`z42.core/Object.z42`）——PR2b 就是把那句「compiler synthesises」落地。

## Why

blob 值 struct 现在能装箱进 `object` 且答 `GetType`/`is`/`as`（PR2a），但**没有** `Equals`/`GetHashCode`/
`ToString`——`ExportedTypeExtractor` 的 `if (!isStruct)` 门把 Object 四方法排除、struct 无 vtable。因此
`o.Equals(x)` / 放进 `Dictionary`/`HashSet`（要 `GetHashCode`+`Equals`）/ `o.ToString()` 在 boxed struct 上
不可用（PR2a 的 boxed vcall 臂对这些方法直接 bail「由 PR2b 提供」）。

值类型的 `Equals` 必须是**字段级值相等**（C# `ValueType.Equals`），正好复用 PR1（`_emitLeafEqChecks`）。

## What Changes

- **编译器为每个 blob 值 struct 合成 3 个 IR 方法**（`IrGen` 类成员循环末尾注入，与合成 ctor 同位；用户已
  显式声明同名则不合成）：
  - `{FQ}.Equals$1(object other)` → `other is P` 则拆箱逐叶子值比较（复用 `_emitStructEquality`），否则 false。
  - `{FQ}.GetHashCode$0()` → 逐叶子 hash 合并（FNV 风格，`__str_hash_code` 同款）。
  - `{FQ}.ToString$0()` → 类型名字符串（C# `ValueType.ToString` 默认=类型名，非字段 dump；字段 dump 留后续）。
- **boxed-struct vcall 派发**（改 PR2a 的 `exec_vcall.rs` BoxedStruct 臂 + `jit/helpers/vcall.rs`）：**先 unbox
  `this` 到当前帧 arena StructRef**，prepend `{type_name}.{method}$arity` 候选命中合成方法；保留 `GetType`
  特判。
- **元数据**（可选，反射/SIGS 报告这些方法）：`ExportedTypeExtractor` 给 struct 也注入四方法签名（去掉/放宽
  `if (!isStruct)`）——使 `struct.GetType().GetMethods()` 见 Equals 等。
- **D5 定案**：`==`/`!=` on boxed struct = 值相等（延续 PR2a `PartialEq`）；`.Equals()` = 合成叶子方法。
  文档标注 float NaN 边角二者微差（`==` 按位 / `Equals` 浮点==）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/IrGen.z42` | MODIFY | 类成员循环末尾：blob struct 且未显式声明 → 合成 Equals/GetHashCode/ToString 并 `_pushFunc` |
| `src/compiler/z42c.semantics/src/FunctionEmitter.z42` | MODIFY | 新增 `EmitSynthStructEquals` / `EmitSynthStructHashCode`（EmitContext 脚手架 + 直接 emit body） |
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | MODIFY | 暴露/新增合成 body 发射入口（复用 `_emitStructEquality`/`_emitLeafEqChecks` + 新 hash 叶子遍历）；`Equals` 的 `is`+unbox+leaf 组装 |
| `src/compiler/z42c.semantics/src/ExportedTypeExtractor.z42` | MODIFY | struct 注入 Object 四方法签名（反射/SIGS 面；放宽 `if (!isStruct)`） |
| `src/runtime/src/interp/exec_vcall.rs` | MODIFY | BoxedStruct 臂：unbox this + prepend `{type_name}.{method}` 候选 |
| `src/runtime/src/jit/helpers/vcall.rs` | MODIFY | 同上（JIT 对称） |
| `src/tests/types/struct_object_methods.z42` | NEW | golden：`o.Equals`（同值/异值/异类型）、`GetHashCode`（同值同 hash）、Dictionary<P,V> 存取、ToString、`==` on boxed |
| `docs/book/src/runtime/struct-value-semantics.md` | MODIFY | 「合成对象方法」小节 + Deferred 更新 |

**只读引用**：`StubEmitter.z42`（单块 IrFunction 构造范式，供 ToString）、`ObjectMethods.z42`（四方法签名）、
`Boolean.z42`（手写 prim Equals/GetHashCode/ToString 范式）、`convert.rs`/`string.rs`（hash 约定）、
`loader.rs`（func_index 按名注册）。

## Out of Scope

- **struct 作泛型容器键**（`Dictionary<P,V>` / `HashSet<P>`）——泛型路径把 struct 键当**未装箱 StructRef**
  传入（`key.GetHashCode()` = 对 StructRef 的 VCall，且存进容器堆数组 = 帧作用域句柄逃逸 use-after-free）。
  正确解 = **泛型边界装箱**（struct→type-param K 且 K 存堆时装箱）或 P3 容器内联——属 P3/泛型工作，**PR2b
  前本就不工作**（非回归）。PR2b 只覆盖显式 boxed 对象协议（`((object)p).Equals/GetHashCode/ToString/==`）。
- **VCall on 未装箱 StructRef receiver**（同上根因）——留 P3/泛型。
- **ToString 字段 dump**（`P { x = 1, y = 2 }`）——PR2b 用类型名（C# 默认），字段 dump 留后续。
- **`IEquatable<T>.Equals(P)` 泛型重载**——只合成 `Equals(object)`（Object 覆写）；typed 重载后续。
- **struct 实现用户接口的方法派发**（`IFoo f = boxedStruct; f.Bar()`）——boxed vcall 派发地基本 PR 搭好，
  但用户接口方法的完整解析留 P4 面。
- P3 容器内联 / P4 跨包 / P5 JIT 值路径 / B-radical。

## Open Questions

- [ ] D5 已在本提案定为值相等 + 合成 Equals（NaN 边角文档标注）；若 User 倾向 C# 引用语义再议（但 z42
      owned-Box 表示下引用语义 ill-defined，不推荐）。
