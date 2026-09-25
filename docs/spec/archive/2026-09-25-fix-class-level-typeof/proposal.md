# Proposal: 类级 `typeof(T)` 产出真实类型，不再是占位名

> 状态：🔴 DRAFT，待 User 确认。类型：**vm**（新增 corelib builtin + 语言可见语义变更）⇒ 走完整流程。

## Why

`class Box<T>` 的方法体里 `typeof(T)` 今天**产占位名**——`typeof(T).FullName` 得到字符串
`"T"` 而不是 `"Std.Int32"`。这是手册逐章实跑抓出的 8 个真 bug 之一（坑点 ③）。

两条事实决定了它现在就该修：

1. **载体本来就在**。同一个类级型参的 `default(T)` **一直是对的**——`DefaultOf` 指令读
   `regs[0].instance.type_args[idx]`（`exec_address.rs:78-88`）。`typeof(T)` 要的是同一个
   载体、换个产出（产 `Std.Type` 而非零值）。
2. **方法级那条路已经通了**。`add-generic-methods` 给方法级型参做了 `MethodTypeArg`；
   `#815`（`Type` 值相等）合入后 `typeof(U) == typeof(int)` 已为真。**只剩类级这一格。**

不做的代价：泛型代码拿不到自己的类型实参，反射 / 序列化 / 诊断消息里全是 `"T"`；
而文档（`generic-methods.md` §「🔴 `typeof(T)` 对类级型参产出占位名」、
`docs/learn/src/types/generics.md` 小结「四个边界」）不得不把它写成语言的既定限制。

### 归档设计稿预先给了做法

`add-generic-methods/design.md:47-50` 的 D3 明写延后理由（「无 serde 依赖、避免 Scope 蔓延
……**若后续需要，另开 change 用同款范式补类级**」）——这次就是「后续」到了。
⚠️ 与前几次「Deferred 的理由已腐坏」不同，**D3 的理由当时成立、现在也没被推翻**，
它只是把做法留给了本 change。

## What Changes

- **新 corelib builtin `__class_type_arg(receiver, index)`**：读实例 `type_args[index]` →
  `make_type_from_name` 产 `Std.Type`；越界 / 非对象 / 空 type_args → 沿用今天的占位
  constructed type（**与 `method_type_arg` 逐字同款的优雅降级**）。
- **绑定期**：`typeof(T)` 中 T 是**类级**型参且当前在**实例语境**时，标 `IsClassLevel` +
  `ClassParamIndex`（复用已存在但全仓零调用的 `TypeEnv.ClassParamIndexOf`）。
- **发射期**：类级分支发 `BuiltinInstr(dst, "__class_type_arg", [this, idx])`，
  照 `_emitMethodOf` / `_emitBoxPrim` 的既有范式——**不新增 IR 指令、不 bump 任何格式**。
- 三处文档与一处活示例同步（见 Scope）。

### 🔑 关键判断：不需要新 IR 指令

原记录标红「这条需不需要新 IR 指令（类似 `DefaultOfInstr` 的 `TypeofOfInstr`）？若需要 ⇒
格式 bump + 两代自举纪律，成本完全不同级」。**实测结论：不需要。**

- `_emitMethodOf` 的抬头注释（`TypeOpEmitter.z42:84-87`）白纸黑字立了先例：
  「**不新增 IR 指令、不 bump 格式**：照 `_emitBoxPrim` 的范式复用既有 Builtin opcode」。
- `Instruction::Builtin` 在 JIT 里是**按名 / BuiltinId 的通用派发**
  （`jit/translate/call.rs:41-64`）⇒ 新 builtin **白送 JIT 支持**，无需动 translate。
- z42c 与 stdlib 源码**零处**使用类级 `typeof(T)`（全仓只有注释提到）⇒ 自举字节不动点
  不受影响、无需 fingerprint bump。
- ⚠️ `BuiltinId = BUILTINS 表下标、会被烤进 zbc` ⇒ **只可表尾追加**
  （`builtin_table.rs` 抬头铁律）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/corelib/reflection/generics.rs` | MODIFY | 新增 `builtin_class_type_arg`（读实例 type_args → `Std.Type`）|
| `src/runtime/src/corelib/builtin_table_ext.rs` | MODIFY | **表尾追加** `("__class_type_arg", reflection::builtin_class_type_arg)` |
| `src/compiler/z42c.semantics/src/BoundExprOp.z42` | MODIFY | `BoundTypeof` 增 `IsClassLevel` / `ClassParamIndex` 两字段（并订正 D3 注释）|
| `src/compiler/z42c.semantics/src/TypeOpTyper.z42` | MODIFY | `_bindTypeofExpr`：方法级之后补类级分支（含实例语境判据）|
| `src/compiler/z42c.semantics/src/TypeOpEmitter.z42` | MODIFY | `_emitTypeof`：类级分支发 `__class_type_arg(this, idx)` |
| `src/tests/generics/class_level_typeof.z42` | NEW | e2e 正面用例（单型参 / 多型参 / 引用与值型参 / `== typeof(int)`）|
| `src/tests/generics/class_level_typeof_edges.z42` | NEW | e2e 边界用例（静态语境 / 继承基类 / 方法级同名遮蔽 均保持占位，不崩）|
| `src/runtime/src/corelib/reflection/reflection_tests.rs` | MODIFY | Rust 单测：builtin 的越界 / 非对象 / 空 type_args 三条降级路径 |
| `examples/types/generics/gaps/typeofgap.z42` | MODIFY | 活示例：期望值从「实际 T」改为真类型；`isInt()` 现为 true |
| `examples/types/generics/gaps/run.console` | MODIFY | transcript 重放（**列号 / 输出以实跑为准**）|
| `docs/reference/src/language/generic-methods.md` | MODIFY | 删「🔴 类级产占位名」段，改为「两级都具化」+ 留下的两条边界 |
| `docs/learn/src/types/generics.md` | MODIFY | 第 18 章坑点段（241-256 行）与小结「四个边界」订正 |
| `docs/internals/src/compiler/generics.md` | MODIFY | 补机制段：类级型参具化的两个载体（`DefaultOf` 指令 / `__class_type_arg` builtin）及为何不用新 opcode |
| `docs/roadmap.md` | MODIFY | Deferred Backlog：`generic-methods-future-classlevel-typeof` 标已落地 + 新登记继承/静态那条（第二刀）|

📌 **Scope 表的两处实施期订正**（按纪律记录，非静默扩张）：

- ➖ `src/runtime/src/corelib/reflection/mod.rs` **不需要改** —— 它已有
  `pub use self::generics::*;`，新 `pub fn` 自动导出。
- ➕ `docs/roadmap.md` **补入 Scope** —— tasks 4.6 本就列了它（「D3 延后项消化」），
  是 Scope 表初稿漏登。实际有 Deferred Backlog 条目 `generic-methods-future-classlevel-typeof`
  需标已落地，属「延后项被消化」⇒ 阶段 9 文档同步的三处正交项之一，必改。
- ⚠️ `docs/learn/src/types/generics.md` 的改动**顺带订正了小结里两条早已修好的旧边界**
  （`new T()` 遇基元会崩 = #803 已修、泛型 ctor 实参不检查 = #812 已修；均已实跑复核）。
  该文件在 Scope 内、且是我必须重写的**同一句**，留着错的等于知情交付错文档。

**只读引用**（理解上下文必须读，不修改）：

- `src/runtime/src/interp/exec_address.rs` — `default_of` / `method_type_arg` 的载体与降级范式
- `src/runtime/src/corelib/reflection/type_object.rs` — `make_type_from_name` / `make_constructed_type`
- `src/compiler/z42c.semantics/src/TypeEnv.z42` — `ClassParamIndexOf` / `MethodParamIndexOf`
- `src/compiler/z42c.semantics/src/ExprTyper.z42` — `_bindDefault` 的「方法级就近优先」范式
- `src/runtime/src/corelib/builtin_table.rs` — BuiltinId = 表下标的铁律
- `docs/agent/rules/bootstrap-seed.md` — 确认本 change 不触发两代纪律

## Out of Scope

- 🔴 **继承链的 `type_args` 装配**（`class Derived : Box<int>` 的实例上 `typeof(T)` /
  `default(T)` 都拿不到 `int`）。**User 已裁决拆第二刀**：它是另一套机制（`ObjNew` 需携带
  基链实参 + 按声明类而非扁平下标寻址），且要 fingerprint bump。实测证据：
  `DerivedG<U> : Box<int>` 里派生自己的 `U` 与基类的 `T` **都想占扁平数组下标 0**，
  扁平寻址在这一形态上无解。本刀这两个形态**保持今天的占位行为，不引入「看起来对的错类型」**。
- 🔴 **静态语境**（`static` 方法里的类级 `typeof(T)`）：保持占位。顺带实测发现
  `default(T)` 在静态语境会**静默读第一个实参的 type_args**
  （`class Box<T> { static Peek(Box<int> o) { T z = default(T); } }` 实测产 `0` 而非 `null`）——
  这是既有的静默 bug，**记入备注、不在本刀顺手修**（属第二刀）。本刀的实例语境判据
  正是为了不把这个隐患一起搬进 `typeof`。
- 泛型反射三件套（`MakeGenericType` / 泛型 `Invoke`）——属 `plan-generic-reflection` G 流。
- 新诊断码：本刀不报任何新错误（静态语境不判红，属破坏性变更，另议）。

## Open Questions

- [x] 需不需要新 IR 指令？→ **不需要**（实测，见上）。
- [x] 载体走 builtin 还是新 opcode 还是纯脱糖？→ **User 裁决：builtin**（纯脱糖会让任何泛型类
      的发射码依赖 `Std.Reflection` API，引入跨包依赖边——`_depHasFunction` 就是为这类问题存在的）。
- [x] 继承 / 静态两个「载体本来就空」的形态？→ **User 裁决：拆第二刀**，本刀保持占位。
