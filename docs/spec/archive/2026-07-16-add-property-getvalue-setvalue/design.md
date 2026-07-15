# Design: PropertyInfo.GetValue / SetValue

## Architecture

```
PropertyInfo (z42.core)                 corelib/reflection.rs
  Name / PropertyType / CanRead/Write   builtin_type_properties
  __getterQualified  ◄──────────────────  灌槽（PropAccum.getter_qualified）
  __setterQualified  ◄──────────────────  灌槽（PropAccum.setter_qualified）
  GetValue(obj)  ──[Native __property_get_value]──►  invoke_qualified(getterQ, [obj])
  SetValue(obj,v)──[Native __property_set_value]──►  invoke_qualified(setterQ, [obj,v])
                                                        │
                              (shared helper 抽自 builtin_method_invoke)
                                                        ▼
                                          exec_function → 原类型异常经 set_pending_thrown
```

## Decisions

### Decision 1: 访问器限定名存 PropertyInfo 槽（而非运行期按 target 类型现查）
**问题**：GetValue 要定位 `get_<Name>` 函数。两法：A 存声明期就知道的 getter/setter `__qualified` 到 PropertyInfo 槽；B GetValue 时按 `target.GetType()` 现场搜 `get_<Name>`。
**决定**：选 A。`builtin_type_properties` 派生属性时**本就手握**每个访问器的 `qualified`（`accumulate_property` 的入参），顺手写槽零额外解析；且与 `MethodInfo.__qualified` 表示统一。B 需运行期名字搜索 + 处理重载，更重。
**代价**：按声明类访问器调用，不做虚 override 派发（记 Deferred）——与 MethodInfo.Invoke 同限制，可接受。

### Decision 2: 复用 builtin_method_invoke 的执行通路（抽 helper）
**问题**：invoke 逻辑（module.func_index 查找 → arity check → exec_function → Thrown 经 set_pending_thrown 传播）已在 `builtin_method_invoke`。
**决定**：抽 `invoke_qualified(ctx, qualified, receiver_and_args: Vec<Value>) -> Result<Value>`，GetValue/SetValue/Method.Invoke 三者共用。避免复制异常传播这类易错逻辑。
**注**：属性访问器均为实例方法（getter/setter 都占 reg0 = this），helper 直接收「已组装好的 call_args」即可，is_static 判定留调用方（属性 MVP 恒实例）。

### Decision 3: 无 getter/setter 抛 Std.Exception（不静默返 null）
**问题**：GetValue on 只写属性 / SetValue on 只读属性。
**决定**：抛 `Std.Exception`（对齐 C# 抛 ArgumentException/InvalidOperation 的语义方向；z42 MVP 用通用 Std.Exception）。不返 null——静默失败会掩盖调用方 bug。`__getterQualified == null` 即判定无 getter。

## Implementation Notes

- **PropAccum 扩字段**：`getter_qualified: Option<String>` / `setter_qualified: Option<String>`，在 `accumulate_property` 的 is_get 分支写入（与 getter_type/setter_type 同处）。
- **灌槽**：`builtin_type_properties` 的 `alloc_named` 追加两槽（`Value::Str` 或 `Value::Null`）。PropertyInfo.z42 声明对应隐藏字段方能被 `read_obj_slot` 读到。
- **arity**：getter call_args = `[target]`（1）；setter = `[target, value]`（2）。走 `invoke_arity_check` 复用。
- **异常通道**：Thrown → `ctx.set_pending_thrown(val)` + `bail!("__z42_reflected_throw__")`，由 `exec_call::builtin` 再抛，保持原类型（与 Method.Invoke 逐字一致）。

## Testing Strategy

- **e2e golden**（`src/libraries/z42.core/tests/property_get_set/`）：读实例属性 / 写后读回 / 继承属性 / 只读属性 SetValue 抛 / 只写属性 GetValue 抛 / 访问器内 throw 原类型捕获；interp + jit 双模。
- **GREEN gate**：`cargo build` + `xtask test e2e` + `xtask test stdlib`（PropertyInfo [Test]）+ `xtask test compiler`（自举不回归——本变更零编译器改动，byte-identical 应天然稳）。
- **无格式 bump** → 无 fixture 字节 golden 变更、无 version-bumping checklist。
