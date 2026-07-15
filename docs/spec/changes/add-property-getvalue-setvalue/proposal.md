# Proposal: PropertyInfo.GetValue / SetValue（反射读写属性值）

> 状态：🔴 DRAFT，待 User 6.5 确认。
> 子系统：**runtime**（反射 builtin）+ **stdlib**（z42.core PropertyInfo）。

## Why

反射的属性面目前只有**只读元数据**（`PropertyType` / `CanRead` / `CanWrite`），拿不到/改不了实例上的属性值。`PropertyInfo.z42` 头注释写「GetValue/SetValue 需 Invoke，随泛型实例化 0.5.x」——**该判断已过期**：非泛型 `Method.Invoke` 已于 2026-06-30（add-method-invoke-non-generic）落地。属性访问器 `get_<Name>` / `set_<Name>(value)` 都是**非泛型实例方法调用**，因此 GetValue/SetValue 现在即可用既有 invoke 通路实现，**不依赖泛型实例化、不动 zbc 格式、不改编译器**。

不做的代价：属性反射停在「能看不能用」，序列化 / 数据绑定 / 通用 mapper 等场景无法基于反射读写属性。

## What Changes

- **PropertyInfo 携访问器限定名**：`__type_properties` 派生每个 PropertyInfo 时，把 getter / setter 的 `__qualified`（如 `geometry.Point.get_X`）写入隐藏槽 `__getterQualified` / `__setterQualified`（VM 写、运行期对象槽，**不持久化、无格式 bump**）。
- **两个反射 builtin**（`corelib/reflection.rs`）：
  - `__property_get_value(propInfo, target) -> object`：以 `__getterQualified` 走 `exec_function`（target 作 reg0 接收者，无参），返回 getter 结果；无 getter 抛 `Std.Exception`。
  - `__property_set_value(propInfo, target, value)`：以 `__setterQualified` 调 setter（target + value）；无 setter 抛。
  - 复用从 `builtin_method_invoke` 抽出的共享 helper `invoke_qualified(...)`（异常原类型经 `set_pending_thrown` 传播，与 Method.Invoke 一致）。
- **stdlib**：`PropertyInfo.z42` 加隐藏字段 `__getterQualified` / `__setterQualified` + `GetValue(object obj)` / `SetValue(object obj, object value)`（`[Native]` extern）；更新过期头注释。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.core/src/Reflection/PropertyInfo.z42` | MODIFY | 加 2 隐藏字段 + GetValue/SetValue；订正头注释 |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | PropAccum 加 getter/setter qualified；builtin_type_properties 灌槽；新增 2 builtin + 抽 invoke_qualified helper |
| `src/runtime/src/corelib/mod.rs` | MODIFY | 注册 `__property_get_value` / `__property_set_value` |
| `src/libraries/z42.core/tests/reflection.z42` | MODIFY | 加 [Test]：读写 roundtrip / 只读属性 SetValue 抛 / 继承属性读写（复用既有 PropHolder/PropChild）|
| `docs/design/language/reflection.md` | MODIFY | PropertyInfo 段更新；`reflection-future-properties` 的 GetValue/SetValue 标落地；订正过期依赖 |
| `docs/roadmap.md` | MODIFY | §15 反射行 / Deferred 索引（如涉及）|

**只读引用**：
- `src/runtime/src/corelib/reflection.rs` `builtin_method_invoke`（复用 invoke 通路）
- `src/libraries/z42.core/src/Reflection/MethodInfo.z42`（`[Native]` extern 写法参照）

## Out of Scope

- **虚属性的运行期覆盖派发**：GetValue/SetValue 以**声明类**访问器限定名调用（与 MethodInfo.Invoke 按 `__qualified` 一致），不按 target 运行期类型做虚派发。虚属性 override 精确派发 → Deferred。
- **静态属性**：MVP 仅实例属性（`target` 作 reg0）；静态属性 GetValue(null) → Deferred。
- **索引器属性**（`this[i]`）：需带索引实参，Deferred。
- 泛型方法 Invoke / MakeGenericType / Activator.CreateInstance<T> 仍在 0.4.x G 流。

## Open Questions

- [ ] 无（设计自包含）。待 User 6.5 确认 + 授权取 `stdlib` 锁（现由 DRAFT 状态的 converge 持有，footprint 与本变更零重叠）。
