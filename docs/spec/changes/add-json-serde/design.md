# Design: JSON serde（对象 ↔ JSON）

## Architecture

```
  Std.Json.JsonSerializer  (public static, z42.json)
    ├─ Serialize(o) / SerializePretty(o)  ──►  _toJson(o): JsonValue  ──► JsonWriter (既有)
    ├─ Deserialize<T>(json)  ──►  typeof(T) ──► Deserialize(Type, json)
    └─ Deserialize(Type, json)  ──► JsonValue.Parse ──► JsonBinder._fromJson(Type, JsonValue)

  JsonMember (z42.json)   —— 字段 + 属性统一：GetFields()/GetProperties() → 键名/类型/读写谓词
  JsonBinder (z42.json)   —— _fromJson 反射核 + 构造绑定 + 数值 coercion
  JsonPropertyAttribute / JsonIgnoreAttribute (z42.json, : Attribute)

  反射底座（z42.core + runtime）
    ├─ Type.GetFields/GetProperties/GetConstructors/GetElementType/IsArray/FullName
    ├─ FieldInfo/PropertyInfo Get/SetValue + GetCustomAttributes（PropertyInfo attr 本次新增）
    ├─ ConstructorInfo.Invoke / Activator.CreateInstance
    └─ Array.CreateInstance/GetValue/SetValue/GetLength（本次新增）

  属性 attribute 载体（**无格式 bump**——复用既有 field_attributes）
    z42c: AttributeSynth 补 PropertyDecl 工厂 + ClassDescBuilder 把属性 attr 挂 __prop_X 背后字段
    runtime: __property_custom_attributes（剥 get_/set_ 前缀 → __prop_<Name> 查 field_attributes）
```

## Decisions

### Decision 1: 按目标 Type 分派，而非 JSON kind
**问题：** JSON number 统一解析为 Long（整数）或 Double；但目标成员可能是 int/long/double/其它宽度。
**决定：** `_fromJson(Type t, JsonValue v)` 以 `t.FullName` 为主分派轴：
- `Std.Int32/Int16/SByte/Byte/UInt16/UInt32/Int64/UInt64` → `(目标宽度)v.AsLong()`
- `Std.Double/Single` → `v.AsDouble()`（`AsDouble` 已把 Long-kind 提升为 double）
- `Std.Boolean` → `v.AsBool()`；`Std.String` → `v.AsString()`
- `t.IsArray` → 递归元素；class → 构造 + 成员绑定
产出**精确类型的 boxed 值**交 SetValue/Invoke，避免 SetValue 端做类型 coercion。

### Decision 2: 属性 attribute 挂背后字段 `__prop_X`（复用 field_attributes，**无格式 bump**）
**问题：** 属性 desugar 为「背后字段 `__prop_X` + get_/set_ 方法」，zbc 里**没有属性实体**可挂 attribute；
PropertyInfo 也纯由 get_/set_ 方法运行期派生（`PropertyInfo.z42` 头注：no persisted property metadata）。
**选项：**
- A **背后字段 attrs**（复用 zbc 1.14 起既有的 `field_attributes` 格式，无格式 bump）——计算属性
  （无背后字段）不覆盖；GetFields 有意排除 `__prop_*` 故 attr 经 PropertyInfo 反射通道读。
- B 新增独立 `property_attributes` 段（per-type name→attrRefs），格式 bump。
**决定（2026-08-23 定案，从 B 回退到 A）：** 选 **A**。最初按 User「最完整」裁决走 B，但**格式-bump
的 CI 两代自举链**（gen0 种子 stdlib → gen1/gen2 新格式）在多目录 workspace prelude 扫描下反复出现无法
本地复现、难以隔离的 gen1-stdlib 编译失败（`E0401: undefined: String` in 无关包），投入巨大仍未定位根因。
权衡后**回退到无格式 bump 的 A**——彻底避开两代自举复杂度、本地即可完整 GREEN，代价是计算属性不支持
attr（M2 已知限制，实测全仓无真实需求）。
**机制：** `__property_custom_attributes(getterQualified)` 剥 `get_`/`set_` 前缀 → 背后字段名
`__prop_<Name>` → 查该字段 `field_attributes`。**前提修复见 Decision 6。**

### Decision 3: 构造模型——无参优先，带参兜底
**问题：** 可变 DTO 用无参 ctor + setter；record / 只读 auto-prop 只能构造期赋值。
**决定：**
1. `GetConstructors()`；若存在**无参** ctor → `Activator.CreateInstance(t)` + 对每个 JSON 键按名
   `SetValue` 到可写成员。
2. 否则选**参数最多**的 ctor：按参数名从 JSON object 取值（缺失→`DefaultValue`/类型默认），递归
   `_fromJson(param.ParameterType, ...)`，`ConstructorInfo.Invoke(args)`；构造后把 JSON 中剩余、
   且成员可写的键再补 SetValue。

### Decision 4: 成员模型——字段 + 属性统一
**问题：** GetFields 排除 `__prop_*` 背后字段 → auto-property 只能经 GetProperties。
**决定：** `JsonMember` 抽象封装 `FieldInfo` 或 `PropertyInfo`：
- 字段：GetFields()，跳过 `IsStatic` / 非 `IsPublic`；CanRead=CanWrite=true。
- 属性：GetProperties()；序列化看 `CanRead`，反序列化看 `CanWrite`（只读 auto-prop 经 ctor 路径填）。
- `[JsonIgnore]` 成员整体排除；键名 = `[JsonProperty].Name` 或成员名。
- z42c 编译器**不跨包用 delegate**（见 memory z42c-no-cross-pkg-delegates）——但本代码在 stdlib 用户侧，
  `JsonMember` 用普通 `if (field != null)` 分派，不用 delegate 回调，规避该坑。

### Decision 5: 反射式数组（System.Array parity）
**问题：** `T[]` 反序列化时元素类型只以运行期 `Type` 已知，无法用静态 `new T[n]`。
**决定：** 新增 4 个 runtime builtin（复用 `heap().alloc_array_typed` + `ArrayObj::{len,get_boxed,set_boxed}`）：
- `Array.CreateInstance(Type elem, int n)` → 默认填充的 `elem[]`
- `Array.GetValue(object arr, int i)` / `Array.SetValue(object arr, int i, object v)` / `Array.GetLength(object arr)`
序列化读数组成员亦用 GetLength/GetValue（成员以 `object` 持有，无法静态 `.Length`）。

### Decision 6.5: 反射底座下沉 z42.json-local extern（自举 gen0 约束，实施期发现）
**问题：** 原计划把 array 反射 + property-attr 读取做成 `Std.Array`/`PropertyInfo` 的公开 API，z42.json 调用之。
**实测发现：** 冷启动 / download-bootstrap **gen0 用种子 stdlib 编译整套 stdlib（含 z42.json）**——种子的
`Std.Array`/`PropertyInfo` 无本次新 native → z42.json 引用其公开 API 会 `E0401`（axis② stdlib API 面约束）。
**决定：** 把新 runtime native（`__array_*` / `__property_custom_attributes`）声明为 **z42.json 自有
extern**（`JsonReflect.z42`），编译期仅 extern 声明、native 运行期由新 VM 解析，不依赖种子 prelude 的
新成员；属性 qualified 名由种子**已有**的 `PropertyInfo.__getterQualified` + `Name` 推导。**回退 z42.core
改动**（Array.z42 / PropertyInfo.z42 不动）。所有 serde 能力不变；公开反射 API 下沉作发布后 follow-up。
**次生坑（cross-pkg imported-type footgun）：** z42.json 跨包读 imported 反射成员返回**引用类型**
（`FieldType`/`PropertyType`→Type、`GetAttribute`→Attribute、`GetElementType`/`ParameterType`）会解析成
`unknown`（z42c 跨包 imported 成员类型解析 gap）——须逐处**显式 cast** `(Type)`/`(Attribute)` + 用 local
receiver（镜像 z42.core reflection.z42 测试的 `(FieldTagAttribute)` cast / 342 行 footgun 注）。

### Decision 6: 实施期发现并修复的三个反射 gap（`Deserialize<T>` 端到端所需）
serde 能 RUN 后（无格式 bump → 本地单代自举即可跑 [Test]）暴露三个此前未测的反射缺口：

1. **属性 attr 从未合成工厂**（Bug：`[JsonProperty]` on 属性不生效）。`AttributeSynth._processMembers`
   原只处理 `MethodDecl`/`FieldDecl` 的 store-meta attr，**漏 `PropertyDecl`** → 属性 attr 的
   `FactoryFunc==""` → `ClassDescBuilder._attrRefsFromList` 跳过 → 背后字段无 attr。**修：** 补
   `PropertyDecl` 分支（`prop$<Class>$<Name>` key）。
2. **反射成员类型的基元名 vs 装箱类型名**（Bug：primitive 成员反序列化 `no runtime handle`）。
   `FieldType`/`PropertyType` 反射的 `FullName` 用**规范基元名**（`"int"`/`"bool"`/`"string"`），而
   装箱运行期类型（`GetType`）用 `Std.Int32` 等 FQ——`FromJson` 须**两套词汇都接受**，否则基元成员
   落到 `_fromObject` → `Activator(int)` 无 handle。
3. **跨包泛型 `typeof(T)` 短名丢 handle**（Bug：`Deserialize<T>` 全线 `no runtime handle`）。见
   [`docs/book/src/stdlib/json-serde.md`](../../../book/src/stdlib/json-serde.md#跨包泛型-typeoft-的-handle-解析deserializet-依赖)
   ——`make_type_from_name` 加无点短名唯一简单名兜底解析。

## Implementation Notes

- **基元 FullName 集**：`Std.Int32/Int64/Int16/SByte/Byte/UInt16/UInt32/UInt64/Single/Double/Boolean/String`。
  char 不在 M2（Deferred）。
- **属性 attr 载体**：挂合成的 `__prop_<Name>` 背后字段的 `.Attrs`（既有 `field_attributes` 格式，
  **无新格式**）。运行期从 `PropertyInfo.__getterQualified`（`<Class>.get_<Name>`）在 **Rust 侧**剥
  `get_`/`set_` 前缀 → `__prop_<Name>` → 查该字段 `field_attributes`（z42 侧不对跨包字段做字符串方法
  调用——那会误 VCall，见实施期 Bug 1）。
- **byte-identical 自举**：z42c / stdlib / xtask 源零 `[Attr]` on 属性 → `__prop_X` 的 Attrs 恒空
  （除 z42.json 新代码）；**无格式 bump**，gen1==gen2 不动点不受影响。
- **数值 coercion 边界**：`AsLong` 对非 Long-kind 抛 JsonException；`AsDouble` 已容 Long→Double。int 目标
  遇 JSON Double（如 `1.5`）→ 走 `(int)(long)` 前需先判 kind，M2 直接 `(int)v.AsLong()`（Double→int 视为
  类型不匹配抛异常，符合「类型不匹配→抛」场景）。

## Testing Strategy

- **单元 [Test]**（`xtask test stdlib`）：`z42.json/tests/serialize.z42` + `deserialize.z42` 覆盖上述全部
  scenario（基元/嵌套/数组/null/无参 ctor/带参 ctor/缺键/多余键/改名/忽略/字段+属性）。
- **反射 [Test]**：`z42.core/tests/reflection.z42` 增 Array 反射 + PropertyInfo attr 往返。
- **example**：`examples/json_serde.z42` 端到端演示。
- **无格式 bump → 本地即可完整 GREEN**：单代自举跑 `xtask test`（stdlib z42.json 全绿 + 自举
  gen1==gen2 不动点 + `cargo test --lib`）。无需 fixture 重生、无两代自举墙。

## Deferred（登记 roadmap Deferred Backlog Index）

### json-serde-future-collections
- **来源**：本 spec Out of Scope
- **触发原因**：List<T>/Dictionary<K,V>/Set 需泛型容器运行期反射 + Add 反射
- **前置依赖**：泛型容器实例化 + 集合接口反射
- **触发条件**：serde 覆盖需扩展到集合类型时

### json-serde-future-enum-nullable-char
- **来源**：本 spec Out of Scope
- **触发原因**：enum（名/底层值）、`T?`、char 的映射策略未定
- **触发条件**：这些类型进入 serde 需求

### json-serde-future-casing-policy
- **来源**：本 spec Out of Scope
- **触发原因**：camelCase ↔ PascalCase 自动命名策略需配置面
- **当前 workaround**：逐成员 `[JsonProperty("name")]` 显式指定

### json-serde-future-public-reflection-api
- **来源**：Decision 6.5（实施期自举约束）
- **触发原因**：格式-bump gen0 用种子 stdlib 编 z42.json → 不能引用新 `Std.Array`/`PropertyInfo` API，
  故本 change 把反射底座放 z42.json-local extern（`JsonReflect.z42`）
- **前置依赖**：本 nightly（含 `__array_*`/`__property_custom_attributes` native）已发布
- **触发条件**：发布后作**非-format-bump** follow-up——把 `Std.Array.{CreateInstance,GetValue,SetValue,GetLength}`
  + `PropertyInfo.{GetCustomAttributes,GetAttribute}` 下沉为公开反射 API，并让 z42.json 改用之（去掉本地重复）
