# Proposal: 反射式数组 + 属性 attribute 反射 API 下沉

## Why

serde M2（`add-json-serde`）新增的 5 个反射 runtime native 当初被声明为 **z42.json 库自有
`extern`**（`JsonReflect.z42`），因为格式-bump 冷启动的种子 stdlib 尚无对应公开 API（当时的保守选择）。
这些 native 已随 nightly 发布、runtime 已注册，应把它们下沉为 **`Std.Array` / `Std.Reflection.PropertyInfo`
的公开反射 API**，让其它 stdlib 与用户代码都能用「反射式数组建/读写」与「属性 attribute 读取」，并让
z42.json 改用公开 API 之后**删掉自有 extern 去重**。

> **自举安全性（关键，已核实）**：本 change **零格式 bump**（native 已存在、无 zbc/zpkg 格式变化）。
> z42.json 引用新 `Std.Array` API 在冷启动**不会** E0401——`bootstrap-seed.md` axis② 明确豁免 stdlib
> 源（「stdlib 源自身不受此轴约束，由自建的当前 z42c 编译」），且 `xtask build stdlib` 的 workspace
> 构建把**全体成员 fresh dist 排在 `Z42_LIBS` 种子之前**（`z42c.driver/src/Main.z42:490-505`），拓扑序
> 保证 z42.json 后建时解析到刚建好的 fresh z42.core（含新 API）。故 Phase 1 + Phase 2 可放**同一个 PR**，
> 本地 warm 全绿即可验证（workspace 解析代码路径 warm/cold 相同）。

## What Changes

- **Phase 1（公开 API，照搬 C# `System.Array`——User 定「参考 C#」）**：
  - `Std.Array` 加：静态 `CreateInstance(Type,int)` + 实例 `GetValue(int)` / `SetValue(object value,int index)`
    （长度用**既有 `Length` 字段**，C# 风格），绑既有 native `__array_create` / `__array_get` / `__array_set`。
  - `Std.Reflection.PropertyInfo` 加属性 attribute API：`GetCustomAttributes()` / `GetAttribute(Type)`
    （镜像 `FieldInfo`），绑既有 native `__property_custom_attributes`。
- **runtime（受控小改，为忠实 C# `SetValue(value,index)` 顺序，见 design Decision 4）**：
  - `corelib/array.rs` `builtin_array_set` 重排为读 `(array, value, index)`；删死 native `__array_length`
    （C# 用 `.Length` 取代）+ `corelib/mod.rs` 对应注册行。**无格式 bump，self-host 字节不动点仍成立**。
- **Phase 2（z42.json 改用 + 删 extern）**：
  - `JsonBinder` / `JsonSerializer` / `JsonMember` 的调用点从 `JsonReflect.*` 切到公开 API（含 `(Array)o` 下转型）。
  - `JsonReflect.z42` 删除 5 个 `extern`（`__array_*` ×4 + `__property_custom_attributes`）+ `PropAttr` 辅助
    （其逻辑并入 `PropertyInfo.GetAttribute`）。集合反射辅助（`GenericArg`/`List*`/`Dict*`）**保留不动**
    （它们用的是已公开的反射 API）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.core/src/Array.z42` | MODIFY | Phase 1：加 C# 风格 `CreateInstance`(静态)/`GetValue`/`SetValue`(实例)，绑 `__array_*` |
| `src/libraries/z42.core/src/Reflection/PropertyInfo.z42` | MODIFY | Phase 1：加 `__attrCache` + `__customAttributes` extern + `GetCustomAttributes` / `GetAttribute` |
| `src/runtime/src/corelib/array.rs` | MODIFY | 重排 `builtin_array_set` 为 `(array,value,index)`；删死 `builtin_array_length` |
| `src/runtime/src/corelib/mod.rs` | MODIFY | 删 `("__array_length", …)` native 注册行 |
| `src/libraries/z42.json/src/JsonBinder.z42` | MODIFY | Phase 2：`NewArray`/`ArraySet` → `Array.CreateInstance`/`Array.SetValue` |
| `src/libraries/z42.json/src/JsonSerializer.z42` | MODIFY | Phase 2：`ArrayLength`/`ArrayGet` → `Array.GetLength`/`Array.GetValue` |
| `src/libraries/z42.json/src/JsonMember.z42` | MODIFY | Phase 2：`JsonReflect.PropAttr(...)` → `p.GetAttribute(...)` |
| `src/libraries/z42.json/src/JsonReflect.z42` | MODIFY | Phase 2：删 5 个 extern + `PropAttr` 辅助（保留集合反射辅助） |
| `src/libraries/z42.core/tests/reflection_api_downsink.z42` | NEW | Phase 1 API 的 [Test]：反射数组 roundtrip + 属性 attr 读取 |
| `src/libraries/z42.core/src/README.md` | MODIFY | 功能索引加 Array 反射 + PropertyInfo attr 行 |
| `docs/book/src/stdlib/json-serde.md` | MODIFY | 「反射底座的自举约束」节改写为「已下沉公开 API」 |
| `docs/roadmap.md` | MODIFY | Deferred `json-serde-future-public-reflection-api` 标记完成 |

**只读引用**（理解上下文，不改）：
- `src/libraries/z42.core/src/Reflection/FieldInfo.z42` — attr API 镜像模板
- `src/runtime/src/corelib/reflection.rs` — `__property_custom_attributes` native（lenient，不改）
- `src/compiler/z42c.driver/src/Main.z42` — workspace 构建解析序（确认自举安全，不改）

## Out of Scope

- **runtime 仅两处**（见 design Decision 4）：`builtin_array_set` 重排 + 删死 `__array_length`。`__array_create`
  / `__array_get` / `__property_custom_attributes` **不改**（arg 读取已兼容 C# 调用形态）。无 zbc/zpkg 格式变化。
- **不引入 `PropertyInfo.__qualified` VM 字段**：attr 查找复用现有 `__getterQualified`/`__setterQualified`
  推导（native 自行 strip `get_`/`set_`），零 VM 侧写入改动。
- **不动集合反射辅助**（`JsonReflect.GenericArg`/`List*`/`Dict*`）：它们用已公开 API，非本次下沉对象。
- **不新增 `Array.GetLength(object)`**：C# 用 `.Length` 属性（z42 既有字段）取代。

## Open Questions

- [ ] **反射设计 doc 落点**：`docs/design/language/reflection.md` 是现存反射 SoT，但 doc-system D2 规定
  `docs/design/` 不再更新、知识上浮 book。book 尚无反射页。→ 本 change 是否顺带新建 `docs/book` 反射页
  （较大迁移），还是仅更新 `reflection.md` 的 API 列表 + json-serde.md？（design Decision 3 给推荐）
