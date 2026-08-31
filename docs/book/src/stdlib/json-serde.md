# JSON serde（对象 ↔ JSON）

> 对齐：2026-08-22（add-json-serde）。包路径 `src/libraries/z42.json/`；命名空间 `Std.Json`。
> DOM 层（`JsonValue`）见 [json 设计文档](../../../design/stdlib/json.md)。

`Std.Json.JsonSerializer` 经**反射**把用户对象与 JSON 互转，无需手写 DOM 代码。

## 公开 API

| 方法 | 说明 |
|------|------|
| `Serialize(object) → string` | 对象 → compact JSON |
| `SerializePretty(object) → string` | 对象 → 2 空格缩进 JSON |
| `Deserialize<T>(string) → T` | JSON → T（泛型薄壳，`typeof(T)` 驱动）|
| `Deserialize(Type, string) → object` | 非泛型入口（泛型薄壳转发到此）|

特性：`[JsonProperty("k")]` 改键名、`[JsonIgnore]` 跳过——**字段与属性均生效**（含计算属性）。

## 实现原理

### 分派轴：目标 Type，而非 JSON kind

反序列化核 `JsonBinder.FromJson(Type t, JsonValue v)` 以 **`t.FullName`** 为主分派轴（不是 JSON 值
的 kind）。原因：JSON number 统一解析为 `long`（整数）或 `double`，但目标成员可能是 `int`/`long`/
`double`。按目标类型分派才能正确 coerce：

```
FromJson(t, v):
  v.IsNull            → null
  t.FullName=="Std.Int32"   → (int)v.AsLong()
  t.FullName=="Std.Int64"   → v.AsLong()
  t.FullName=="Std.Double"  → v.AsDouble()     # AsDouble 已容 long→double 提升
  t.FullName=="Std.Boolean" → v.AsBool()
  t.FullName=="Std.String"  → v.AsString()
  t.IsArray           → 反射建 elem[]，逐元素递归
  baseFn=="Std.Collections.List"       → 反射建 List<T>，逐元素递归
  baseFn=="Std.Collections.Dictionary" → 反射建 Dictionary<K,V>
  else (class)        → 构造 + 成员绑定
```

> **构造型泛型 FullName 含实参**（fix-type-reflection-names）：`typeof(List<int>).FullName` 现为
> `Std.Collections.List<Std.Int32>`，故集合检测按**去实参的基名** `baseFn`（`FullName` 截到首个 `<` 前）
> 匹配 `Std.Collections.List` / `Std.Collections.Dictionary`，而非整串 FullName。基元/字符串分派轴
> 不受影响（基元 FullName 无 `<…>`）。

序列化 `_toJson(object o)` 反向，按 `o.GetType().FullName` 分派（多态元素随其**运行期**类型；集合同样按基名）。

### 成员模型：字段 + 属性统一

`JsonMembers.For(Type)` 收集 public 非 static 字段（`GetFields`）+ 属性（`GetProperties`），封装成
`JsonMember`（隐藏 FieldInfo 或 PropertyInfo）。序列化看 `CanRead`，反序列化看 `CanWrite`。键名 =
`[JsonProperty].Name` 或成员名；`[JsonIgnore]` 排除。

> auto-property 脱糖为私有背后字段 `__prop_X`，`GetFields()` **有意排除**它 → 属性只能经
> `GetProperties()` 遍历。

### 构造模型：无参优先，带参兜底

```
ctors = t.GetConstructors()
有无参 ctor（或无 ctor）→ Activator.CreateInstance + 对每个 JSON 键按名 SetValue 到可写成员
否则 → 选参数最多的 ctor，按参数名映射 JSON 键（递归 FromJson 到参数类型）ConstructorInfo.Invoke，
        再补齐剩余可写成员（覆盖 record / 只读 auto-prop）
```

缺键 → 成员保持默认值 / ctor 参数用 `DefaultValue`；多余键 → 忽略；类型不匹配 → `JsonException`。

### 属性 attribute 的格式载体（复用 field_attributes，**无格式 bump**）

属性 desugar 为「背后字段 `__prop_X` + get_/set_ 方法」→ zbc TYPE 段里**无独立属性实体**可挂
attribute。本 change **不新增格式**，而是把属性上的 attr **挂到合成的背后字段 `__prop_X` 的
`.Attrs`**（复用 zbc 1.14 起就存在的 `field_attributes` 格式）：

1. **AttributeSynth**（`_processMembers`）——原本只为 `MethodDecl`/`FieldDecl` 的 store-meta attr
   合成工厂并记 `FactoryFunc`，本 change 补 **`PropertyDecl` 分支**（`prop$<Class>$<Name>` key），
   否则属性 attr 因 `FactoryFunc==""` 被 `_attrRefsFromList` 跳过。
2. **ClassDescBuilder**——合成 `__prop_X` 备份字段时把该属性的 attr-refs 写进 `ibf.Attrs`。
3. **运行期** `__property_custom_attributes` 收 `PropertyInfo.__getterQualified`（`"<Class>.get_<Name>"`），
   在 Rust 侧剥 `get_`/`set_` 前缀 → 背后字段名 `__prop_<Name>` → 查该字段的 `field_attributes`。

> **已知限制（M2）**：计算属性（有 getter 方法体、无背后字段）无处挂 attr → 不支持在计算属性上
> 加 `[JsonProperty]`/`[JsonIgnore]`。若将来需要，再评估独立格式载体。因**零格式 bump**，两代自举 /
> download-bootstrap 均不受影响（避开了 format-bump 的 CI 两代自举复杂度）。

### 反射底座（已下沉为公开 API，add-array-property-reflection-api 2026-08-24）

serde 用到的反射式数组建/读写与属性 attribute 读取，现已是 **`Std.Array` / `Std.Reflection.PropertyInfo`
的公开反射 API**：

- **`Std.Array`（照搬 C# `System.Array`）**：静态 `CreateInstance(Type, int)`、实例 `GetValue(int)` /
  `SetValue(object value, int index)`、`.Length` 属性——反射式建/读写元素类型只以运行期 Type 已知的 `T[]`。
  绑既有 native `__array_create` / `__array_get` / `__array_set`（后者按 `(array, value, index)` 读，
  对齐 C# value-在前签名）。
- **`Std.Reflection.PropertyInfo`**：`GetCustomAttributes()` / `GetAttribute(Type)`（镜像 `FieldInfo`），
  绑 native `__property_custom_attributes`（Rust 侧从 accessor-qualified 名剥 `get_`/`set_` 前缀 → backing
  field `__prop_<Name>` 取 attr，无格式 bump）。

serde（`JsonBinder` / `JsonSerializer` / `JsonMember`）改用这些公开 API；`JsonReflect.z42` 仅保留**集合
反射辅助**（`GenericArg`/`List*`/`Dict*`，封装已公开的反射 API，见下节）。

> **历史（M2 → 下沉的自举约束）**：serde M2 初版把上述 native 声明为 **z42.json 自有 extern**，出于
> 「gen0 用上一 nightly 种子 stdlib 编译 z42.json、种子无新 API 会 `E0401`」的保守顾虑。**后核实该顾虑
> 对本类改动不成立**：`bootstrap-seed.md` axis② 豁免 stdlib 源（stdlib 由自建的当前 z42c 编），且
> `xtask build stdlib` 的 workspace 构建把全体成员 fresh dist 排在种子 `Z42_LIBS` 之前
> （`z42c.driver/src/Main.z42`），拓扑序保证 z42.json 后建时解析到 fresh z42.core（含新 API）。故此次下沉
> **零格式 bump、单 PR、本地 warm 可验**。

### 跨包泛型 `typeof(T)` 的 handle 解析（`Deserialize<T>` 依赖）

`Deserialize<T>` 经 `typeof(T)` 拿目标 Type 交 `Activator.CreateInstance`——需该 Type 携运行期
handle（`NativeData::TypeHandle`）。方法级 `typeof(T)` 运行期读 `frame.method_type_args[i]` 的**名字**
再 `make_type_from_name` 解析。跨包泛型静态调用（`z42.json` 的 `Deserialize<UserType>`，经 imported
null-receiver vcall 派发）在类型实参**非唯一限定**时可能只带**短名**（如 `"Point"`），令按 FQ 键的
`type_registry` 落空 → 退化成无 handle 的 synthetic Type → Activator 报 `no runtime handle`。

故 `make_type_from_name` 加**兜底**：无点短名在 FQ 查找失败后，按**简单名**在已加载类型
（entry module + lazy loader）里唯一匹配则解析为真 handle；零/多义则维持 synthetic（不误绑）。用户程序
里目标类型通常唯一 → 稳解析。（根因是编译期跨包泛型实参限定，兜底在运行期最小且安全。）

### 集合类型（add-collection-serde）

`List<T>` ↔ JSON array，`Dictionary<string,V>` ↔ JSON object（字符串键）。**反射-only**（无新 native）：

- **检测**：`Type.FullName == "Std.Collections.List"` / `"Std.Collections.Dictionary"`（置于基元/数组之后、
  通用对象分派之前——否则 List 被当对象遍历其 items/Count 内部字段）。元素类型经 `GetGenericArguments()`。
- **序列化**：List 反射读 `Count`（公开字段）+ `get_Item(i)`；Dict 反射 `Keys()` + `get_Item(key)`（`JsonReflect`
  封装「遍历 `GetMethods()` 按名取」——z42 反射无 `GetMethod(name)`）。
- **反序列化**：`Activator.CreateInstance(memberType)`（非泛型，作用于构造泛型 Type）+ 逐元素 `FromJson` +
  反射 `Add`（List）/ `set_Item`（Dict）。Dict 键非 string → `JsonException`。

> **两个通用反射修复**（字段路径反序列化所需，非集合特有）：
> 1. **`split_generic_args` trim**：字段 type_tag 用源拼写、逗号后带空格（`Dictionary<string, int>`，见 z42c
>    `_typeSourceName` 的 `", "`）→ 原生 split 得 `" int"`（前导空格）→ `make_type_from_name` 落空丢 handle。
>    修 = 每实参 trim（typeof 名无空格，此坑只中 member-type 反射的多实参泛型）。
> 2. **dotless 短基名 force-load 兜底**：字段 type_tag 基名是短名 `List`（非 FQN）。`typeof(List<int>)` 走 FQ
>    force-load 得 handle；字段短名若集合类型未被 `typeof`/`new` 触发加载则从已加载类型找不到 → 无 handle。
>    修 = 无点**类名**（大写首字母、非基元）找不到时一次性 `force_load_all_packages()` 再简单名唯一匹配
>    （gated 大写 → 基元不触发；force-load 幂等一次）。

## 类型覆盖与 Deferred

**覆盖**：基元（int/long/double/bool/string）+ 嵌套对象 + 定长数组 `T[]` + **`List<T>` + `Dictionary<string,V>`**。

**Deferred**（roadmap Deferred Backlog Index）：`Dictionary<K,V>` 非字符串键（→ array-of-pairs）、`Set`/`Queue`/
`Stack`、enum、nullable(`T?`)、char、camelCase↔PascalCase 命名策略。
