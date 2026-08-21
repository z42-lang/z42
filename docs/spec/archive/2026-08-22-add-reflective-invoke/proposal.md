# Proposal: 反射式调用补全（泛型方法 Invoke + 构造函数反射）

## Why

两条互补的反射保真度缺口，一并补齐（User 裁决捆绑）：

1. **泛型方法反射式调用（G2）**：M1（#240）落地了泛型方法**直接调用**（`Foo<T>()` + 方法体
   `typeof(T)`/`new T()`/`default(T)` 在调用点具化），但显式 Defer 了**反射式调用**——给定 `MethodInfo`
   运行期绑定类型实参再 `Invoke`。这是 roadmap G 流条目，也是 **L 流招牌 `Deserialize<T>` serde** 的既定
   前置。M1 已铺好 `frame.method_type_args` 帧槽 + 物化 opcode，复用之。

2. **构造函数反射层级（补 C# 保真度）**：z42 当前反射层级**扁平**——`MethodInfo : MemberInfo`，
   **无 `MethodBase` 抽象基、无 `ConstructorInfo`**；构造函数不作单独反射，建实例仅
   `Activator.CreateInstance(Type)`（无参、且**不跑构造函数**、只分配+字段零初始化）。对齐 C#
   `MemberInfo → MethodBase → {MethodInfo, ConstructorInfo}`，并让 `ConstructorInfo.Invoke(args)` 支持
   **带参构造**（重开此前 Deferred 的能力）。构造函数反射与 serde M2 相关（反序列化建带参/record 实例）。

参照 C# `System.Reflection` 语义。泛型方法静态 + 实例都覆盖。

## What Changes

### A. 反射类型层级对齐 C#
```
MemberInfo
 └─ MethodBase              ← 新增基类（共享 Name/IsStatic/GetParameters/__qualified）
     ├─ MethodInfo          ← reparent（保留 ReturnType/IsVirtual/泛型方法 API）
     └─ ConstructorInfo     ← 新增（Invoke = 带参构造）
```

### B. 泛型方法反射（G2）—— **无需格式 bump**
> **实施期发现（2026-08-21，改写原方案）**：zbc/zpkg **SIGS 段早已预留方法类型形参槽**——writer
> （`ZbcWriter.z42:443`）恒写 `tpCount=0`，但 z42 reader（`ZbcReader.z42:496`）、Rust reader
> （`zbc_reader.rs:886` → `FuncSig.type_params`）**全链路已读**。格式**自描述**（tpCount 告知后随几个），
> 故填真实类型形参**不改字节布局**（非泛型方法 tpCount=0 逐字节不变；泛型方法多出的字节旧新 reader 都能
> 消费）。**且现有 stdlib/z42c 源零泛型方法声明**（grep 实测 0）→ writer 改动不影响任何现有方法字节 →
> **自举字节不动点 gen1==gen2 不受影响**。**⇒ 原计划的格式 bump（版本号/fixture/pinned test/两代自举）
> 整个取消。**

1. **producer 侧填元数据（无格式变更）**：`IrFunction` 加**方法级**类型形参名字段；`FunctionEmitter` 从
   `md.TypeParams.Names`（M1 已解析）填入；`ZbcWriter` 把 `:443` 的硬编码 `0` 换成真实 `tpCount + 名字`
   （+ 空约束包，where 约束 Deferred）。reader（z42 + Rust）已就绪，不动。
2. **`MethodInfo` API**（参 C#）：`IsGenericMethod` / `IsGenericMethodDefinition` /
   `GetGenericArguments()` / `MakeGenericMethod(params Std.Type[])` → 构造后的 `MethodInfo`（无独立子类型）。
3. **native**：反射侧 `resolve_func_sig`/`build_method_info` 露出已读到的 `FuncSig.type_params`
   （若运行期 `Function` 未携带则一并 thread FuncSig→Function）；`__method_make_generic`（arity/泛型性
   校验 + 克隆盖 `__typeArgs`）；`builtin_method_invoke` 读 `__typeArgs` → 线程进 callee
   `frame.method_type_args`（复用 M1 物化）。

### C. 构造函数反射
1. **`Type.GetConstructors() → ConstructorInfo[]`**（native `__type_constructors`）：按 ctor 命名约定
   （`<ClassFQ>.<ClassSimpleName>[$N]`，见 `IrInstr.z42:426`）枚举方法表，无额外格式字段。
2. **`ConstructorInfo.Invoke(object[] args) → object`**（native `__ctor_invoke`）：分配实例（同
   `__activator_create` 的分配 + typeArgs 具化）+ 以新对象为 receiver(reg0) 跑 ctor 函数 + 返回 this。
   **这是带参构造能力**（重开 Deferred）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| **A. 层级** | | |
| `src/libraries/z42.core/src/Reflection/MethodBase.z42` | NEW | 新基类：Name/IsStatic/GetParameters/__qualified/抽象 Invoke 契约 |
| `src/libraries/z42.core/src/Reflection/MethodInfo.z42` | MODIFY | reparent `: MethodBase`；加泛型方法 API；上移共享成员 |
| `src/libraries/z42.core/src/Reflection/ConstructorInfo.z42` | NEW | `: MethodBase`；`Invoke(object[])` 带参构造 |
| `src/libraries/z42.core/src/Type.z42` | MODIFY | `GetConstructors()` |
| **B. 泛型方法元数据（producer 侧，无格式 bump）** | | |
| `src/libraries/z42.ir/src/IrModule.z42` | MODIFY | `IrFunction` 加**方法级** `TypeParams: string[]` + `TypeParamCount`（默认空） |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcWriter.z42` | MODIFY | SIGS 段：intern 方法类型形参名 + `:443` 硬编码 0 换成真实 tpCount+名字（空约束包） |
| `src/compiler/z42c.semantics/src/FunctionEmitter.z42` | MODIFY | 把 `md.TypeParams.Names/.Count`（方法级）填入 IrFunction |
| **C. native（反射消费）** | | |
| `src/libraries/z42.core/src/Reflection/MethodInfo.z42` | (见 A) | 泛型 API + `__typeArgs` 槽 |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | `resolve_func_sig`/`build_method_info` 露出 type_params；`__method_make_generic`；invoke 线程 typeArgs；`__type_constructors`；`__ctor_invoke` |
| `src/runtime/src/metadata/bytecode.rs` | MODIFY（按需） | 运行期 `Function` 若未携带 type_params → 加字段（thread FuncSig.type_params→Function） |
| `src/runtime/src/metadata/resolver.rs`/`merge.rs` | MODIFY（按需） | thread FuncSig.type_params → Function（若上一条需要） |
| `src/runtime/src/corelib/mod.rs` | MODIFY | 注册 `__method_make_generic`/`__type_constructors`/`__ctor_invoke` |

> **不再改**（原格式-bump 计划已取消）：`ExportedTypes.z42`(ExportedMethodZ)、`ZbcReader.z42`、
> `TsigReconcile.z42`、`ZpkgWriter.z42`/`ZbcFormat.z42`(版本号)、`zbc_reader.rs`(读已就绪)、
> `zbc_reader_tests.rs`(pinned)、格式 fixture。reader 全链路已读 tpCount+names，仅 producer 侧填数。
| **测试 + 文档** | | |
| `src/tests/generic-method-invoke/` | NEW | golden：静态/实例 Invoke、typeof/default/new 反射==直接、throw 保类型 |
| `src/tests/ctor-reflection/` | NEW | golden：GetConstructors 枚举、带参 Invoke 建实例、无参 ctor |
| `src/libraries/z42.core/tests/reflection_generic_method/` | NEW | MethodInfo 泛型反射 [Test] |
| `src/libraries/z42.core/tests/reflection_constructor/` | NEW | ConstructorInfo/MethodBase [Test] |
| `docs/book/src/language/generic-methods.md` | MODIFY | 反射式调用节 |
| `docs/book/src/stdlib/reflection.md` | MODIFY | MethodBase/ConstructorInfo/泛型方法反射 API（不存在则新写 + 挂 SUMMARY） |
| `docs/roadmap.md` | MODIFY | G2 ✅ + 构造函数反射 + Deferred 索引 |
| 格式-bump fixture（`*.zbc`/`*.zpkg` header hand-patch + changelog） | MODIFY | version-bumping.md checklist |

**只读引用**：`interp/mod.rs`（frame.method_type_args + exec_function_from_regs）、`interp/exec_call.rs`
（M1 直接调用填帧）、`IrInstr.z42`（M1 opcode + CtorName 约定）、`Activator.z42`（分配逻辑参照）、
`docs/spec/changes/add-generic-methods/**`（M1 设计）、`.claude/rules/version-bumping.md` /
`bootstrap-seed.md`。

## Out of Scope

- **`Type.GetConstructor(Std.Type[])` 按参数类型的重载解析** — 调用方用 `GetConstructors()` +
  `GetParameters()` 自选；类型匹配重载解析留 Deferred（独立 can of worms）。
- **G3 `Activator.CreateInstance<T>`**（泛型 Activator）— 另开。
- **反射式 `where` 约束运行期校验**（泛型方法 + ctor）— M1 编译期已查直接调用；反射式留 Deferred。
- **开放泛型类上的泛型方法双层交叉具化** — 留 Deferred。
- **serde 引擎本身** — G2/ctor 落地后另开 M2。

## Open Questions

- [x] ~~格式 bump 时机 / 单 PR vs 拆分~~ — **实施期发现格式 bump 不需要**（SIGS 段已预留方法类型形参槽，见 What Changes B 顶注）→ 议题消失；仍单 PR。
- [x] ConstructorInfo 是否捆进 — User 裁决：**捆进**（含带参构造；defer 重载解析）。
- [ ] GetConstructors 枚举靠命名约定 vs is-ctor 元数据位 — IMPL 期定；因不再有格式 bump，若约定脆弱需另评估（不搭便车了）。
- [ ] MethodBase 是否用 z42 抽象方法承载 Invoke 契约（z42 抽象能力核对）— IMPL 期定；倾向各具体子类各带 Invoke native，MethodBase 只放共享数据成员。
- [ ] 运行期 `Function` struct 是否已携带 type_params（FuncSig 已读）— IMPL 期确认；未携带则 thread FuncSig→Function。
