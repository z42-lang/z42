# Proposal: 泛型 Activator.CreateInstance<T>（G3）

## Why
泛型反射三件套的前两件已落地：`MethodInfo.MakeGenericMethod().Invoke()` + `MakeGenericType()`（G2 #249）。
第三件 `Activator.CreateInstance<T>()`（roadmap 0.4.3 G3）尚缺——泛型工厂 / DI / 反序列化薄壳等场景
需要「以类型参数 T 直接无参构造」的便捷入口。当前只有非泛型 `CreateInstance(Type)`，用户须自己写
`(T)Activator.CreateInstance(typeof(T))`（serde `Deserialize<T>` 内部正是这么做）。补上泛型形收口三件套。

## What Changes
1. `Std.Reflection.Activator` 新增泛型静态方法 `CreateInstance<T>()`：返回 `new T()`（无参构造），
   实现为**既有非泛型 native 的薄泛型壳**（`(T)CreateInstance(typeof(T))`）。
2. **方法级泛型形参转发（#240 通用缺口修复）**：修复「外层泛型方法把自己的方法级形参 T 作为类型实参
   转发给嵌套泛型调用」（`Foo<T>() { Bar<T>() }`）。此前调用点发字面 "T" → 被调方 `typeof(T)`
   `make_type_from_name("T")` 落空丢 handle。改为发**转发标记 `$mta:<idx>`**，运行期按**调用方**
   frame.method_type_args[idx] 解析成具体名。这是 `CreateInstance<T>()` 在泛型方法内可用的前置。
- 无新 native、无新 IR opcode、**无 zbc/zpkg 格式改动**——`method_type_args` 本就是 string[]，
  `$mta:<idx>` 只是标记字符串，运行期在**拷入 callee frame 前**按调用方 frame 解析。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.core/src/Reflection/Activator.z42` | MODIFY | 加 `CreateInstance<T>()` 泛型方法 + 更新头注 |
| `src/libraries/z42.core/tests/reflection.z42` | MODIFY | `CreateInstance<T>` 往返 [Test]（用户类 / ctor 副作用 / 泛型方法内转发） |
| `src/compiler/z42c.semantics/src/Bound.z42` | MODIFY | `BoundCall.MethodTypeArgFwd`（与 MethodTypeArgs 平行的转发下标数组） |
| `src/compiler/z42c.semantics/src/MemberResolver.z42` | MODIFY | `_applyMethodTypeArgs` 填 fwd：类型实参是方法级形参 → 记 `MethodParamIndexOf` |
| `src/compiler/z42c.semantics/src/CallEmitter.z42` | MODIFY | `_methodTypeArgNames` 据 fwd 发 `$mta:<idx>` 标记 |
| `src/runtime/src/interp/mod.rs` | MODIFY | `resolve_forwarded_mta` helper（按调用方 frame 解析 `$mta:N`） |
| `src/runtime/src/interp/exec_call.rs` | MODIFY | 静态调用入口解析转发标记 |
| `src/runtime/src/interp/exec_vcall.rs` | MODIFY | 实例/null-recv vcall 入口解析转发标记 |
| `docs/book/src/language/generic-methods.md` | MODIFY | 方法级形参转发机制（`$mta:<idx>`）+ 边界更新 |
| `docs/roadmap.md` | MODIFY | 0.4.3 G3 标 ✅ + Deferred Backlog 条目更新 |

> `z42.core/src/README.md` 不改：该 README 未itemize Reflection/ 子目录（4 层，无 per-file 索引），
> Activator 泛型形的机制文档落 book/generic-methods.md。

**只读引用**：
- `src/runtime/src/corelib/reflection.rs`（`builtin_activator_create` / `make_type_from_name` — 理解 native + 短名兜底）
- `src/compiler/z42c.semantics/src/TypeEnv.z42`（`MethodParamIndexOf` — 方法级形参下标映射）
- `src/compiler/z42c.semantics/src/TypeOpTyper.z42`（`typeof(T)` → MethodTypeArgInsn，理解方法级形参物化）

## Out of Scope
- 带参 `CreateInstance<T>(args...)` / `CreateInstance(Type, object[])`（参数化构造）→ Deferred（用 `ConstructorInfo.Invoke` 已可，#249）。
- **嵌套构造泛型里的方法级形参转发**（`Bar<List<T>>` 内的 T）→ Deferred：只做**顶层**类型实参转发
  （`Bar<T>`）；`$mta` 在尖括号内的解析（make_type_from_name 角括号解析里嵌转发）留后续。
- 值类型 / 数组 / 基元的 `CreateInstance<T>`（非引用类默认构造语义）→ Deferred。

## Open Questions
- [ ] 无。
